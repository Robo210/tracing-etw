use std::marker::PhantomData;
use std::{pin::Pin, sync::Arc};

use tracelogging::Guid;
use tracing::{span, Subscriber};
use tracing_subscriber::filter::Filtered;
use tracing_subscriber::layer::Filter;
use tracing_subscriber::{registry::LookupSpan, Layer};

use crate::native::{EventMode, EventWriter};
use crate::values::*;
use crate::{map_level, native};

// This version of the struct is packed such that all the fields are references into a single,
// contiguous allocation. The first byte of the allocation is `layout`.
#[cfg(feature = "unsafe")]
#[repr(C)]
struct EtwLayerData {
    fields: &'static [&'static str],
    values: &'static mut [ValueTypes],
    indexes: &'static mut [u8],
    activity_id: [u8; 16], // // if set, byte 0 is 1 and 64-bit span ID in the lower 8 bytes
    related_activity_id: [u8; 16], // if set, byte 0 is 1 and 64-bit span ID in the lower 8 bytes
    _p: std::ptr::NonNull<std::alloc::Layout>,
}

#[cfg(feature = "unsafe")]
impl Drop for EtwLayerData {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self._p.as_ptr() as *mut u8, *self._p.as_ptr()) }
    }
}

// Everything in the struct is Send + Sync except the ptr::NonNull.
// We never deref the ptr::NonNull except in drop, for which safety comes from safe usage of the struct itself.
// Therefore we can safely tag the whole thing as Send + Sync.
#[cfg(feature = "unsafe")]
unsafe impl Send for EtwLayerData {}
#[cfg(feature = "unsafe")]
unsafe impl Sync for EtwLayerData {}

// This version of the struct uses only safe code, but has 3 separate heap allocations.
#[cfg(not(feature = "unsafe"))]
struct EtwLayerData {
    fields: Box<[&'static str]>,
    values: Box<[ValueTypes]>,
    indexes: Box<[u8]>,
    activity_id: [u8; 16], // // if set, byte 0 is 1 and 64-bit span ID in the lower 8 bytes
    related_activity_id: [u8; 16], // if set, byte 0 is 1 and 64-bit span ID in the lower 8 bytes
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
        self.provider_group = ProviderGroup::Linux(std::borrow::Cow::Owned(name.to_owned()));
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
    pub fn build_with_global_filter<S>(self) -> EtwLayer<S, Mode>
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

    #[cfg(not(feature = "global_filter"))]
    pub fn build_with_layer_filter<S>(
        self,
    ) -> Filtered<EtwLayer<S, Mode::Provider>, EtwFilter<S, Mode::Provider>, S>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
        Mode::Provider: EventWriter + 'static,
    {
        self.validate_config();

        let layer = EtwLayer::<S, Mode::Provider> {
            provider: Mode::Provider::new(
                &self.provider_name,
                &self.provider_id,
                &self.provider_group,
                self.default_keyword,
            ),
            default_keyword: self.default_keyword,
            _p: PhantomData,
        };

        let filter = EtwFilter::<S, _> {
            provider: layer.provider.clone(),
            default_keyword: self.default_keyword,
            _p: PhantomData,
        };

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

        // We want to pack the data that needs to be stored on the span as tightly as possible,
        // in order to accommodate as many active spans as possible.
        // This is off by default though, since unsafe code is unsafe.
        #[cfg(feature = "unsafe")]
        let mut data = unsafe {
            let layout_layout = std::alloc::Layout::new::<std::alloc::Layout>();
            let fields_layout = std::alloc::Layout::array::<&str>(n).unwrap();
            let values_layout = std::alloc::Layout::array::<ValueTypes>(n).unwrap();
            let indexes_layout = std::alloc::Layout::array::<u8>(n).unwrap();

            let (layout, fields_offset) = layout_layout.extend(fields_layout).unwrap();
            let (layout, values_offset) = layout.extend(values_layout).unwrap();
            let (layout, indexes_offset) = layout.extend(indexes_layout).unwrap();

            let block = std::alloc::alloc_zeroed(layout);
            if block.is_null() {
                panic!();
            }

            let mut layout_field =
                std::ptr::NonNull::new(block as *mut std::alloc::Layout).unwrap();
            let fields: &mut [&str] =
                std::slice::from_raw_parts_mut(block.add(fields_offset) as *mut &str, n);
            let values: &mut [ValueTypes] =
                std::slice::from_raw_parts_mut(block.add(values_offset) as *mut ValueTypes, n);
            let indexes: &mut [u8] = std::slice::from_raw_parts_mut(block.add(indexes_offset), n);

            *layout_field.as_mut() = layout;

            let mut i = 0;
            for field in metadata.fields().iter() {
                fields[i] = field.name();
                values[i] = ValueTypes::None;
                indexes[i] = i as u8;
                i += 1;
            }

            indexes.sort_by_key(|idx| fields[*idx as usize]);

            EtwLayerData {
                fields,
                values,
                indexes,
                activity_id: [0; 16],
                related_activity_id: [0; 16],
                _p: std::ptr::NonNull::new(block as *mut std::alloc::Layout).unwrap(),
            }
        };

        #[cfg(not(feature = "unsafe"))]
        let mut data = {
            EtwLayerData {
                fields: vec![""; n].into_boxed_slice(),
                values: vec![ValueTypes::None; n].into_boxed_slice(),
                indexes: ([
                    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,
                    22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
                ])[..n]
                    .to_vec()
                    .into_boxed_slice(),
                activity_id: [0; 16],
                related_activity_id: [0; 16],
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
            fields: &data.fields,
            values: &mut data.values,
            indexes: &mut data.indexes,
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
            &data.values,
            map_level(metadata.level()),
            self.default_keyword,
            0,
        );
    }

    fn on_exit(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was exited
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

        self.provider.as_ref().span_stop(
            &span,
            timestamp,
            &data.activity_id,
            &data.related_activity_id,
            &data.fields,
            &data.values,
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
            fields: &data.fields,
            values: &mut data.values,
            indexes: &mut data.indexes,
        });
    }
}
