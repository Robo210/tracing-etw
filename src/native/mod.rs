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
