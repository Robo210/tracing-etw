#[cfg(target_os = "windows")]
mod etw;
#[cfg(target_os = "windows")]
pub(crate) use etw::ProviderWrapper;
#[cfg(target_os = "windows")]
pub(crate) use etw::EventBuilderWrapper;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod noop;
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub(crate) use noop::ProviderWrapper;
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub(crate) use noop::EventBuilderWrapper;

#[cfg(target_os = "linux")]
mod user_events;
#[cfg(target_os = "linux")]
pub(crate) use user_events::ProviderWrapper;
#[cfg(target_os = "linux")]
pub(crate) use user_events::EventBuilderWrapper;

pub(crate) struct GuidWrapper(u128);

impl From<&tracelogging::Guid> for GuidWrapper {
    fn from(value: &tracelogging::Guid) -> Self {
        Self(value.to_u128())
    }
}

impl From<&eventheader::Guid> for GuidWrapper {
    fn from(value: &eventheader::Guid) -> Self {
        Self(value.to_u128())
    }
}

impl From<GuidWrapper> for tracelogging::Guid {
    fn from(value: GuidWrapper) -> Self {
        tracelogging::Guid::from_u128(&value.0)
    }
}

impl From<GuidWrapper> for eventheader::Guid {
    fn from(value: GuidWrapper) -> Self {
        eventheader::Guid::from_u128(&value.0)
    }
}

#[derive(Clone)]
pub(crate) enum ProviderGroup {
    Unset,
    #[allow(dead_code)]
    Windows(tracelogging::Guid),
    #[allow(dead_code)]
    Linux(std::borrow::Cow<'static, str>),
}
