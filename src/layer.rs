use std::marker::PhantomData;
use std::time::SystemTime;
use std::{pin::Pin, sync::Arc};

use tracelogging::Guid;
use tracing::metadata::LevelFilter;
use tracing::{span, Subscriber};
use tracing_subscriber::filter::{combinator::And, FilterExt, Filtered, Targets};
use tracing_subscriber::layer::Filter;
use tracing_subscriber::{registry::LookupSpan, Layer};

use crate::native::ProviderGroup;

use crate::native::{EventMode, EventWriter};
use crate::values::*;
use crate::{map_level, native};

pub(crate) static GLOBAL_ACTIVITY_SEED: once_cell::sync::Lazy<[u8; 16]> =
    once_cell::sync::Lazy::new(|| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seed = (now >> 64) as u64 | now as u64;
        let mut data = [0; 16];
        let (seed_half, _) = data.split_at_mut(8);
        seed_half.copy_from_slice(&seed.to_le_bytes());
        data[0] = 0;
        data
    });

struct EtwLayerData {
    fields: Box<[FieldValueIndex]>,
    activity_id: [u8; 16], // // if set, byte 0 is 1 and 64-bit span ID in the lower 8 bytes
    related_activity_id: [u8; 16], // if set, byte 0 is 1 and 64-bit span ID in the lower 8 bytes
    start_time: SystemTime,
}

#[doc(hidden)]
pub struct EtwLayerBuilder<Mode> {
    pub(crate) provider_name: String,
    pub(crate) provider_id: tracelogging::Guid,
    pub(crate) provider_group: native::ProviderGroup,
    pub(crate) default_keyword: u64,
    _m: PhantomData<Mode>,
}

pub struct LayerBuilder {}

impl LayerBuilder {
    pub fn new(name: &str) -> EtwLayerBuilder<native::Provider> {
        EtwLayerBuilder::<native::Provider> {
            provider_name: name.to_owned(),
            provider_id: Guid::from_name(name),
            provider_group: native::ProviderGroup::Unset,
            default_keyword: 1,
            _m: PhantomData,
        }
    }

    /// For advanced scenarios.
    /// Emit events that follow the Common Schema 4.0 mapping.
    /// Recommended only for compatibility with specialized event consumers.
    /// Most ETW consumers will not benefit from events in this schema, and
    /// may perform worse. Common Schema events are much slower to generate
    /// and should not be enabled unless absolutely necessary.
    #[cfg(feature = "common_schema")]
    pub fn new_common_schema_events(
        name: &str,
    ) -> EtwLayerBuilder<native::common_schema::Provider> {
        EtwLayerBuilder::<native::common_schema::Provider> {
            provider_name: name.to_owned(),
            provider_id: Guid::from_name(name),
            provider_group: native::ProviderGroup::Unset,
            default_keyword: 1,
            _m: PhantomData,
        }
    }
}

impl<Mode> EtwLayerBuilder<Mode>
where
    Mode: EventMode,
{
    /// For advanced scenarios.
    /// Assign a provider ID to the ETW provider rather than use
    /// one generated from the provider name.
    pub fn with_provider_id(mut self, guid: tracelogging::Guid) -> Self {
        self.provider_id = guid;
        self
    }

    /// Get the current provider ID that will be used for the ETW provider.
    /// This is a convenience function to help with tools that do not implement
    /// the standard provider name to ID algorithm.
    pub fn get_provider_id(&self) -> tracelogging::Guid {
        self.provider_id
    }

    pub fn with_default_keyword(mut self, kw: u64) -> Self {
        self.default_keyword = kw;
        self
    }

    /// For advanced scenarios.
    /// Set the ETW provider group to join this provider to.
    #[cfg(any(target_os = "windows", doc))]
    pub fn with_provider_group(mut self, group_id: tracelogging::Guid) -> Self {
        self.provider_group = native::ProviderGroup::Windows(group_id);
        self
    }

    /// For advanced scenarios.
    /// Set the EventHeader provider group to join this provider to.
    #[cfg(any(target_os = "linux", doc))]
    pub fn with_provider_group(mut self, name: &str) -> Self {
        self.provider_group =
            native::ProviderGroup::Linux(std::borrow::Cow::Owned(name.to_owned()));
        self
    }

    fn validate_config(&self) {
        match &self.provider_group {
            native::ProviderGroup::Unset => (),
            native::ProviderGroup::Windows(guid) => {
                assert_ne!(guid, &Guid::zero(), "Provider GUID must not be zeroes");
            }
            native::ProviderGroup::Linux(name) => {
                assert!(
                    eventheader_dynamic::ProviderOptions::is_valid_option_value(name),
                    "Provider names must be lower case ASCII or numeric digits"
                );
            }
        }

        #[cfg(all(target_os = "linux"))]
        if self
            .provider_name
            .contains(|f: char| !f.is_ascii_alphanumeric())
        {
            // The perf command is very particular about the provider names it accepts.
            // The Linux kernel itself cares less, and other event consumers should also presumably not need this check.
            //panic!("Linux provider names must be ASCII alphanumeric");
        }
    }

    #[cfg(feature = "global_filter")]
    pub fn build<S>(self) -> EtwLayer<S, Mode>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        self.validate_config();

        EtwLayer::<S, native::Provider> {
            provider: native::Provider::new(
                &self.provider_name,
                &self.provider_id,
                &self.provider_group,
                self.default_keyword,
            ),
            default_keyword: self.default_keyword,
            _p: PhantomData,
        }
    }

    fn build_target_filter(&self, target: &'static str) -> Targets {
        let mut targets = Targets::new().with_target(&self.provider_name, LevelFilter::TRACE);

        match self.provider_group {
            ProviderGroup::Windows(_guid) => {}
            ProviderGroup::Linux(ref name) => {
                targets = targets.with_target(name.clone(), LevelFilter::TRACE);
            }
            _ => {}
        }

        if !target.is_empty() {
            targets = targets.with_target(target, LevelFilter::TRACE)
        }

        targets
    }

    fn build_layer<S>(&self) -> EtwLayer<S, Mode::Provider>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        Mode::Provider: EventWriter + 'static,
    {
        EtwLayer::<S, Mode::Provider> {
            provider: Mode::Provider::new(
                &self.provider_name,
                &self.provider_id,
                &self.provider_group,
                self.default_keyword,
            ),
            default_keyword: self.default_keyword,
            _p: PhantomData,
        }
    }

    fn build_filter<S, P>(&self, provider: Pin<Arc<P>>) -> EtwFilter<S, P>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        P: EventWriter + 'static,
    {
        EtwFilter::<S, _> {
            provider,
            default_keyword: self.default_keyword,
            _p: PhantomData,
        }
    }

    #[cfg(not(feature = "global_filter"))]
    pub fn build_with_target<S>(
        self,
        target: &'static str,
    ) -> Filtered<EtwLayer<S, Mode::Provider>, And<EtwFilter<S, Mode::Provider>, Targets, S>, S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        Mode::Provider: EventWriter + 'static,
    {
        self.validate_config();

        let layer = self.build_layer();

        let filter = self.build_filter(layer.provider.clone());

        let targets = self.build_target_filter(target);

        layer.with_filter(filter.and(targets))
    }

    #[cfg(not(feature = "global_filter"))]
    pub fn build<S>(self) -> Filtered<EtwLayer<S, Mode::Provider>, EtwFilter<S, Mode::Provider>, S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        Mode::Provider: EventWriter + 'static,
    {
        self.validate_config();

        let layer = self.build_layer();

        let filter = self.build_filter(layer.provider.clone());

        layer.with_filter(filter)
    }
}

pub struct EtwFilter<S, P> {
    provider: Pin<Arc<P>>,
    default_keyword: u64,
    _p: PhantomData<S>,
}

impl<S, P> Filter<S> for EtwFilter<S, P>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    P: EventWriter + 'static,
{
    fn callsite_enabled(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        if P::supports_enable_callback() {
            if self
                .provider
                .enabled(map_level(metadata.level()), self.default_keyword)
            {
                tracing::subscriber::Interest::always()
            } else {
                tracing::subscriber::Interest::never()
            }
        } else {
            // Returning "sometimes" means the enabled function will be called every time an event or span is created from the callsite.
            // This will let us perform a global "is enabled" check each time.
            tracing::subscriber::Interest::sometimes()
        }
    }

    fn enabled(
        &self,
        metadata: &tracing::Metadata<'_>,
        _cx: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        self.provider
            .enabled(map_level(metadata.level()), self.default_keyword)
    }

    fn event_enabled(
        &self,
        event: &tracing::Event<'_>,
        _cx: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        self.provider
            .enabled(map_level(event.metadata().level()), self.default_keyword)
    }
}

pub struct EtwLayer<S, P> {
    provider: Pin<Arc<P>>,
    default_keyword: u64,
    _p: PhantomData<S>,
}

impl<S, P> Layer<S> for EtwLayer<S, P>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    P: EventWriter + 'static,
{
    fn on_register_dispatch(&self, _collector: &tracing::Dispatch) {
        // Late init when the layer is installed as a subscriber
    }

    fn on_layer(&mut self, _subscriber: &mut S) {
        // Late init when the layer is attached to a subscriber
    }

    #[cfg(feature = "global_filter")]
    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        if ProviderWrapper::supports_enable_callback() {
            if self
                .provider
                .enabled(map_level(metadata.level()), self.default_keyword)
            {
                tracing::subscriber::Interest::always()
            } else {
                tracing::subscriber::Interest::never()
            }
        } else {
            // Returning "sometimes" means the enabled function will be called every time an event or span is created from the callsite.
            // This will let us perform a global "is enabled" check each time.
            tracing::subscriber::Interest::sometimes()
        }
    }

    #[cfg(feature = "global_filter")]
    fn enabled(
        &self,
        metadata: &tracing::Metadata<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        self.provider
            .enabled(map_level(metadata.level()), self.default_keyword)
    }

    #[cfg(feature = "global_filter")]
    fn event_enabled(
        &self,
        _event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        self.provider
            .enabled(map_level(event.metadata().level()), self.default_keyword)
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let timestamp = std::time::SystemTime::now();

        let current_span = ctx
            .event_span(event)
            .map(|evt| evt.id())
            .map_or(0, |id| (id.into_u64()));
        let parent_span = ctx
            .event_span(event)
            .map_or(0, |evt| evt.parent().map_or(0, |p| p.id().into_u64()));

        self.provider.as_ref().write_record(
            timestamp,
            current_span,
            parent_span,
            event.metadata().name(),
            map_level(event.metadata().level()),
            self.default_keyword,
            event,
        );
    }

    fn on_new_span(
        &self,
        attrs: &span::Attributes<'_>,
        id: &span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let span = if let Some(span) = ctx.span(id) {
            span
        } else {
            return;
        };

        if span.extensions().get::<EtwLayerData>().is_some() {
            return;
        }

        let metadata = span.metadata();

        let parent_span_id = if attrs.is_contextual() {
            attrs.parent().map_or(0, |id| id.into_u64())
        } else {
            0
        };

        let n = metadata.fields().len();

        let mut data = {
            let mut v: Vec<FieldValueIndex> = Vec::with_capacity(n);
            v.resize_with(n, Default::default);
            for i in 0..n {
                v[i].sort_index = i as u8;
            }

            EtwLayerData {
                fields: v.into_boxed_slice(),
                activity_id: *GLOBAL_ACTIVITY_SEED,
                related_activity_id: *GLOBAL_ACTIVITY_SEED,
                start_time: SystemTime::UNIX_EPOCH,
            }
        };

        let (_, half) = data.activity_id.split_at_mut(8);
        half.copy_from_slice(&id.into_u64().to_le_bytes());

        data.activity_id[0] = 1;
        data.related_activity_id[0] = if parent_span_id != 0 {
            let (_, half) = data.related_activity_id.split_at_mut(8);
            half.copy_from_slice(&parent_span_id.to_le_bytes());
            1
        } else {
            0
        };

        attrs.values().record(&mut ValueVisitor {
            fields: &mut data.fields,
        });

        // This will unfortunately box data. It would be ideal if we could avoid this second heap allocation,
        // but at least it's small.
        span.extensions_mut().replace(data);
    }

    fn on_enter(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was started
        let timestamp = std::time::SystemTime::now();

        let span = if let Some(span) = ctx.span(id) {
            span
        } else {
            return;
        };

        let metadata = span.metadata();

        let mut extensions = span.extensions_mut();
        let data = if let Some(data) = extensions.get_mut::<EtwLayerData>() {
            data
        } else {
            // We got a span that was entered without being new'ed?
            return;
        };

        self.provider.as_ref().span_start(
            &span,
            timestamp,
            &data.activity_id,
            &data.related_activity_id,
            &data.fields,
            map_level(metadata.level()),
            self.default_keyword,
            0,
        );

        data.start_time = timestamp;
    }

    fn on_exit(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was exited
        let stop_timestamp = std::time::SystemTime::now();

        let span = if let Some(span) = ctx.span(id) {
            span
        } else {
            return;
        };

        let metadata = span.metadata();

        let mut extensions = span.extensions_mut();
        let data = if let Some(data) = extensions.get_mut::<EtwLayerData>() {
            data
        } else {
            // We got a span that was entered without being new'ed?
            return;
        };

        self.provider.as_ref().span_stop(
            &span,
            (data.start_time, stop_timestamp),
            &data.activity_id,
            &data.related_activity_id,
            &data.fields,
            map_level(metadata.level()),
            self.default_keyword,
            0,
        );
    }

    fn on_close(&self, _id: span::Id, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was closed
        // Good for knowing when to log a summary event?
    }

    fn on_record(
        &self,
        id: &span::Id,
        values: &span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Values were added to the given span

        let span = if let Some(span) = ctx.span(id) {
            span
        } else {
            return;
        };

        let mut extensions = span.extensions_mut();
        let data = if let Some(data) = extensions.get_mut::<EtwLayerData>() {
            data
        } else {
            // We got a span that was entered without being new'ed?
            return;
        };

        values.record(&mut ValueVisitor {
            fields: &mut data.fields,
        });
    }
}
