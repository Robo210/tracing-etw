use std::{borrow::Cow, collections::HashMap, pin::Pin, sync::Arc};
use crossbeam_utils::sync::ShardedLock;
use tracelogging::Guid;

// Providers go in, but never come out.
// On Windows this cannot be safely compiled into a dylib, since the providers will never be dropped.
lazy_static! {
    pub(crate) static ref PROVIDER_CACHE: ShardedLock<HashMap<String, Pin<Arc<ProviderWrapper>>>> =
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
            let es = self
                .provider
                .read()
                .unwrap()
                .find_set(level.into(), keyword);
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
    pub(crate) fn get_provider(
        self: Pin<&Self>,
    ) -> Pin<&std::sync::RwLock<eventheader_dynamic::Provider>> {
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
    pub(crate) fn new(
        provider_name: &str,
        _: &Guid,
        provider_group: &ProviderGroup,
    ) -> Pin<Arc<Self>> {
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
                &options,
            )),
        })
    }
}
