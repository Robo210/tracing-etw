use std::collections::HashMap;
use std::{pin::Pin, sync::Arc};

use crossbeam_utils::sync::ShardedLock;
use smallvec::SmallVec;
use tracelogging::Guid;
use tracing::{span, Subscriber};
use tracing_subscriber::{registry::LookupSpan, Layer};

use crate::activities::{Activities, self}; // filter::{FilterFn}
use crate::{native::*, map_level};
use crate::values::*;

// Providers go in, but never come out.
// On Windows this cannot be safely compiled into a dylib, since the providers will never be dropped.
lazy_static! {
    pub(crate) static ref PROVIDER_CACHE: ShardedLock<HashMap<String, Pin<Arc<ProviderWrapper>>>> =
        ShardedLock::new(HashMap::new());
}

// struct EtwLayerData {
//     activities: Activities,
//     data: SmallVec::<[FieldAndValue; 5]>, // Original metadata order
//     indexes: arrayvec::ArrayVec::<u8, 32>, // Sorted indexes for the data array
// }

struct EtwLayerData {
    _p: usize,
    activities: &'static Activities,
    layout: &'static std::alloc::Layout,
    fields: &'static [&'static str],
    values: &'static mut [ValueTypes],
    indexes: &'static mut [u8],
}

impl Drop for EtwLayerData {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self._p as *mut u8, *self.layout)
        }
    }
}

pub struct EtwLayer {
    pub(crate) provider_name: String,
    pub(crate) provider_id: Guid,
    pub(crate) provider_group: ProviderGroup,
    pub(crate) emit_common_schema_events: bool,
}

impl EtwLayer {
    pub fn new(name: &str) -> Self {
        EtwLayer {
            provider_name: name.to_owned(),
            provider_id: Guid::from_name(name),
            provider_group: ProviderGroup::Unset,
            emit_common_schema_events: false,
        }
    }

    /// For advanced scenarios.
    /// Assign a provider ID to the ETW provider rather than use
    /// one generated from the provider name.
    pub fn with_provider_id(mut self, guid: Guid) -> Self {
        self.provider_id = guid;
        self
    }

    /// Get the current provider ID that will be used for the ETW provider.
    /// This is a convenience function to help with tools that do not implement
    /// the standard provider name to ID algorithm.
    pub fn get_provider_id(&self) -> Guid {
        self.provider_id
    }

    /// Override the default keywords and levels for events.
    /// Provide an implementation of the [`KeywordLevelProvider`] trait that will
    /// return the desired keywords and level values for each type of event.
    // pub fn with_custom_keywords_levels(
    //     mut self,
    //     config: impl KeywordLevelProvider + 'static,
    // ) -> Self {
    //     self.exporter_config = Some(Box::new(config));
    //     self
    // }

    /// For advanced scenarios.
    /// Emit extra events that follow the Common Schema 4.0 mapping.
    /// Recommended only for compatibility with specialized event consumers.
    /// Most ETW consumers will not benefit from events in this schema, and
    /// may perform worse.
    /// These events are emitted in addition to the normal ETW events,
    /// unless `without_realtime_events` is also called.
    /// Common Schema events are much slower to generate and should not be enabled
    /// unless absolutely necessary.
    pub fn with_common_schema_events(mut self) -> Self {
        self.emit_common_schema_events = true;
        self
    }

    /// For advanced scenarios.
    /// Set the ETW provider group to join this provider to.
    #[cfg(any(target_os = "windows", doc))]
    pub fn with_provider_group(mut self, group_id: Guid) -> Self {
        self.provider_group = ProviderGroup::Windows(group_id);
        self
    }

    /// For advanced scenarios.
    /// Set the EventHeader provider group to join this provider to.
    #[cfg(any(target_os = "linux", doc))]
    pub fn with_provider_group(mut self, name: &str) -> Self {
        self.provider_group = ProviderGroup::Linux(Cow::Owned(name.to_owned()));
        self
    }

    pub(crate) fn validate_config(&self) {
        match &self.provider_group {
            ProviderGroup::Unset => (),
            ProviderGroup::Windows(guid) => {
                assert_ne!(guid, &Guid::zero(), "Provider GUID must not be zeroes");
            }
            ProviderGroup::Linux(name) => {
                assert!(
                    eventheader_dynamic::ProviderOptions::is_valid_option_value(&name),
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

    fn get_or_create_provider(&self, target_provider_name: &str) -> Pin<Arc<ProviderWrapper>> {
        fn create_provider(
            this: &EtwLayer,
            target_provider_name: &str,
        ) -> Pin<Arc<ProviderWrapper>> {
            let mut guard = PROVIDER_CACHE.write().unwrap();

            let (provider_name, provider_id, provider_group) = if !target_provider_name.is_empty() {
                (
                    target_provider_name,
                    tracelogging::Guid::from_name(target_provider_name),
                    &ProviderGroup::Unset,
                ) // TODO
            } else {
                // Since the target defaults to module_path!(), we never actually get here unless the developer uses target: ""
                (
                    this.provider_name.as_str(),
                    this.provider_id,
                    &this.provider_group,
                )
            };

            // Check again to see if it has already been created before we got the write lock
            if let Some(provider) = guard.get(provider_name) {
                provider.clone()
            } else {
                guard.insert(
                    provider_name.to_string(),
                    ProviderWrapper::new(
                        provider_name,
                        &GuidWrapper::from(&provider_id).into(),
                        provider_group,
                    ),
                );

                if let Some(provider) = guard.get(provider_name) {
                    provider.clone()
                } else {
                    panic!()
                }
            }
        }

        fn get_provider(provider_name: &str) -> Option<Pin<Arc<ProviderWrapper>>> {
            PROVIDER_CACHE.read().unwrap().get(provider_name).cloned()
        }

        let provider_name = if !target_provider_name.is_empty() {
            target_provider_name
        } else {
            self.provider_name.as_str()
        };

        if let Some(provider) = get_provider(provider_name) {
            provider
        } else {
            create_provider(&self, target_provider_name)
        }
    }

    fn get_or_create_provider_from_metadata(
        &self,
        metadata: &tracing::Metadata<'_>,
    ) -> Pin<Arc<ProviderWrapper>> {
        let target = if let Some(mod_path) = metadata.module_path() {
            if metadata.target() == mod_path {
                self.provider_name.as_str()
            } else {
                metadata.target()
            }
        } else {
            self.provider_name.as_str()
        };

        self.get_or_create_provider(target)
    }
}

impl<S: Subscriber> Layer<S> for EtwLayer
where
    S: for<'lookup> LookupSpan<'lookup>,
{
    fn on_register_dispatch(&self, _collector: &tracing::Dispatch) {
        // Late init when the layer is installed as a subscriber
        self.validate_config();
    }

    fn on_layer(&mut self, _subscriber: &mut S) {
        // Late init when the layer is attached to a subscriber
        self.validate_config();
    }

    fn register_callsite(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        let _ = self.get_or_create_provider_from_metadata(metadata);

        // Returning "sometimes" means the enabled function will be called every time an event or span is created from the callsite.
        // This will let us perform a global "is enabled" check each time.
        //
        // A more complicated design can check for provider enablement here and call rebuild_interest_cache when the provider
        // callback is invoked. Then we can propagate the provider enablement and level/keyword into tracing's cache.
        // This will only work for ETW though, as user_events does not get a provider callback.
        tracing::subscriber::Interest::sometimes()
    }

    fn enabled(
        &self,
        metadata: &tracing::Metadata<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        let provider = self.get_or_create_provider_from_metadata(metadata);
        provider.enabled(map_level(metadata.level()), 0)
    }

    fn event_enabled(
        &self,
        _event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        // Whether or not an event is enabled, after its fields have been constructed.
        true
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

        let activities = Activities::generate(current_span, parent_span);

        let provider = self.get_or_create_provider_from_metadata(event.metadata());
        provider.as_ref().write_record(
            timestamp,
            &activities,
            event.metadata().name(),
            map_level(event.metadata().level()).into(),
            1,
            event,
        );
    }

    fn on_new_span(
        &self,
        attrs: &span::Attributes<'_>,
        id: &span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let span = ctx.span(id);
        if span.is_none() {
            return;
        }

        let span = span.unwrap();
        let metadata = span.metadata();

        let parent_span_id = if attrs.is_contextual() {
            attrs.parent().map_or(0, |id| id.into_u64())
        } else {
            0
        };

        let activities = Activities::generate(id.into_u64(), parent_span_id);

        let _ = self.get_or_create_provider_from_metadata(metadata);

        // We want to back the data that needs to be stored on the span as tightly as possible,
        // in order to accommodate as many active spans as possible.
        let data = unsafe {
            let n = attrs.fields().len();
            let activities_layout = std::alloc::Layout::for_value(&activities);
            let layout_layout = std::alloc::Layout::new::<std::alloc::Layout>();
            let fields_layout = std::alloc::Layout::array::<&str>(n).unwrap();
            let values_layout = std::alloc::Layout::array::<ValueTypes>(n).unwrap();
            let indexes_layout = std::alloc::Layout::array::<u8>(n).unwrap();

            let (layout, layout_offset) = activities_layout.extend(layout_layout).unwrap();
            let (layout, fields_offset) = layout.extend(fields_layout).unwrap();
            let (layout, values_offset) = layout.extend(values_layout).unwrap();
            let (layout, indexes_offset) = layout.extend(indexes_layout).unwrap();

            let block = std::alloc::alloc_zeroed(layout);
            let fields: &mut [&str] = std::slice::from_raw_parts_mut(block.add(fields_offset) as *mut &str, n);
            let values: &mut [ValueTypes] = std::slice::from_raw_parts_mut(block.add(values_offset) as *mut ValueTypes, n);
            let indexes: &mut [u8] = std::slice::from_raw_parts_mut(block.add(indexes_offset), n);

            *(block as *mut Activities) = activities;
            *(block.add(layout_offset) as *mut std::alloc::Layout) = layout;

            let mut i = 0;
            for field in attrs.fields().iter() {
                fields[i] = field.name();
                values[i] = ValueTypes::None;
                indexes[i] = i as u8;
                i += 1;
            }

            indexes.sort_by_key(|idx| fields[*idx as usize]);

            EtwLayerData {
                _p: block as usize,
                activities: &*(block as *const Activities) as &'static Activities,
                layout: &*(block.add(layout_offset) as *const std::alloc::Layout) as &'static std::alloc::Layout,
                fields,
                values,
                indexes,
            }
        };    

        attrs.values().record(&mut ValueVisitor { fields: data.fields, values: data.values, indexes: data.indexes });

        // This will unfortunately box data. It would be ideal if we could avoid this second heap allocation,
        // but at least it's small.
        span.extensions_mut().insert(data);
    }

    fn on_enter(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was started
        let timestamp = std::time::SystemTime::now();

        let span = ctx.span(id);
        if let None = span {
            return;
        }

        let span = span.unwrap();
        let metadata = span.metadata();

        let mut extensions = span.extensions_mut();
        let data = extensions.get_mut::<EtwLayerData>();
        if data.is_none() {
            // We got a span that was entered without being new'ed?
            return;
        }
        let data = data.unwrap();

        let provider = self.get_or_create_provider_from_metadata(metadata);

        provider.as_ref().span_start(
            &span,
            timestamp,
            data.activities,
            data.fields,
            data.values,
            map_level(metadata.level()),
            1,
            0,
        );
    }

    fn on_exit(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was exited
        let timestamp = std::time::SystemTime::now();

        let span = ctx.span(id);
        if let None = span {
            return;
        }

        let span = span.unwrap();
        let metadata = span.metadata();

        let mut extensions = span.extensions_mut();
        let data = extensions.get_mut::<EtwLayerData>();
        if data.is_none() {
            // We got a span that was entered without being new'ed?
            return;
        }
        let data = data.unwrap();

        let provider = self.get_or_create_provider_from_metadata(metadata);

        provider.as_ref().span_stop(
            &span,
            timestamp,
            data.activities,
            data.fields,
            data.values,
            map_level(metadata.level()),
            1,
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

        let span = ctx.span(id);
        if let None = span {
            return;
        }

        let span = span.unwrap();

        let mut extensions = span.extensions_mut();
        let data = extensions.get_mut::<EtwLayerData>();
        if data.is_none() {
            // We got a span that was entered without being new'ed?
            return;
        }
        let data = data.unwrap();

        values.record(&mut ValueVisitor {
            fields: data.fields,
            values: data.values,
            indexes: data.indexes,
        });
    }
}
