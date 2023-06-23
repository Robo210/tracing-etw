use std::{borrow::Cow, pin::Pin, sync::Arc, collections::HashMap};

use tracing::{Subscriber, span};
use tracing_subscriber::{Layer}; // filter::{FilterFn}
use tracelogging::Guid;
use crossbeam_utils::sync::ShardedLock;

// Providers go in, but never come out.
// On Windows this cannot be safely compiled into a dylib, since the providers will never be dropped.
lazy_static! {
    static ref PROVIDER_CACHE: ShardedLock<HashMap<String, Pin<Arc<ProviderWrapper>>>> =
        ShardedLock::new(HashMap::new());
}

pub(crate) fn map_level(level: &tracing::Level) -> u8 {
    match level {
        &tracing::Level::ERROR => tracelogging::Level::Error.as_int(),
        &tracing::Level::WARN => tracelogging::Level::Warning.as_int(),
        &tracing::Level::INFO => tracelogging::Level::Informational.as_int(),
        &tracing::Level::DEBUG => tracelogging::Level::Verbose.as_int(),
        &tracing::Level::TRACE => tracelogging::Level::Verbose.as_int() + 1,
    }
}

// pub fn EtwFilter() -> FilterFn {
//     FilterFn::new(|metadata| { true })
// }

#[derive(Clone)]
pub(crate) enum ProviderGroup {
    Unset,
    #[allow(dead_code)]
    Windows(Guid),
    #[allow(dead_code)]
    Linux(Cow<'static, str>),
}

pub(crate) struct ProviderWrapper {
    #[cfg(any(target_os = "windows"))]
    provider: tracelogging_dynamic::Provider,
    #[cfg(any(target_os = "linux"))]
    provider: std::sync::RwLock<eventheader_dynamic::Provider>,
}

impl ProviderWrapper {
    pub(crate) fn enabled(&self, level: u8, keyword: u64) -> bool {
        #[cfg(any(target_os = "windows"))]
        return self.provider.enabled(level.into(), keyword);

        #[cfg(any(target_os = "linux"))]
        {
            let es = self.provider.read().unwrap().find_set(level.into(), keyword);
            if es.is_some() {
                es.unwrap().enabled()
            } else {
                false
            }
        }
    }

    #[cfg(any(target_os = "windows"))]
    pub(crate) fn get_provider(self: Pin<&Self>) -> Pin<&tracelogging_dynamic::Provider> {
        unsafe { self.map_unchecked(|s| &s.provider) }
    }

    #[cfg(any(target_os = "linux"))]
    pub(crate) fn get_provider(self: Pin<&Self>) -> Pin<&std::sync::RwLock<eventheader_dynamic::Provider>> {
        unsafe { self.map_unchecked(|s| &s.provider) }
    }

    #[cfg(all(target_os = "windows"))]
    pub(crate) fn new(
        provider_name: &str,
        provider_id: &Guid,
        provider_group: &ProviderGroup,
    ) -> Pin<Arc<Self>> {
        let mut options = tracelogging_dynamic::Provider::options();
        if let ProviderGroup::Windows(guid) = provider_group {
            options = *options.group_id(guid);
        }

        let wrapper = Arc::pin(ProviderWrapper {
            provider: tracelogging_dynamic::Provider::new_with_id(
                provider_name,
                &options,
                provider_id,
            ),
        });
        unsafe {
            wrapper.as_ref().get_provider().register();
        }

        wrapper
    }

    #[cfg(all(target_os = "linux"))]
    pub(crate) fn new(provider_name: &str, _: &Guid, provider_group: &ProviderGroup) -> Pin<Arc<Self>> {
        let mut options = eventheader_dynamic::Provider::new_options();
        if let ProviderGroup::Linux(ref name) = provider_group {
            options = *options.group_name(&name);
        }
        let mut provider = eventheader_dynamic::Provider::new(provider_name, &options);

        for lvl in log::Level::iter() {
            provider.register_set(map_level(lvl).into(), 1);
        }

        Arc::pin(ProviderWrapper {
            provider: std::sync::RwLock::new(eventheader_dynamic::Provider::new(
                provider_name,
                &options
            )),
        })
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
                    Guid::from_name(target_provider_name),
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
                    ProviderWrapper::new(provider_name, &provider_id, provider_group),
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

        let provider_name = if target_provider_name.is_empty() {
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

    fn get_or_create_provider_from_metadata(&self, metadata: &tracing::Metadata<'_>) -> Pin<Arc<ProviderWrapper>> {
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

impl<S: Subscriber> Layer<S> for EtwLayer {
    fn on_register_dispatch(&self, collector: &tracing::Dispatch) {
        // Late init when the layer is installed as a subscriber
        self.validate_config();
    }

    fn on_layer(&mut self, subscriber: &mut S) {
        // Late init when the layer is attached to a subscriber
        self.validate_config();
    }

    fn register_callsite(&self, metadata: &'static tracing::Metadata<'static>) -> tracing::subscriber::Interest {
        let _ = self.get_or_create_provider_from_metadata(metadata);

        // Returning "sometimes" means the enabled function will be called every time an event or span is created from the callsite.
        // This will let us perform a global "is enabled" check each time.
        //
        // A more complicated design can check for provider enablement here and call rebuild_interest_cache when the provider
        // callback is invoked. Then we can propagate the provider enablement and level/keyword into tracing's cache.
        // This will only work for ETW though, as user_events does not get a provider callback.
        tracing::subscriber::Interest::sometimes()
    }

    fn enabled(&self, metadata: &tracing::Metadata<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) -> bool {
        let provider = self.get_or_create_provider_from_metadata(metadata);
        provider.enabled(map_level(metadata.level()), 0)
    }

    fn event_enabled(&self, _event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) -> bool {
        // Whether or not an event is enabled, after its fields have been constructed.
        true
    }

    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let provider = self.get_or_create_provider_from_metadata(event.metadata());
        provider.as_ref().write_record(std::time::SystemTime::now(), "Event", 1, event, self);
    }

    fn on_enter(&self, _id: &span::Id, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was started
    }

    fn on_exit(&self, _id: &span::Id, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was exited
    }

    fn on_close(&self, _id: span::Id, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // A span was closed
        // Good for knowing when the log a summary event?
    }
}
