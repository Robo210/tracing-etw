#[cfg(target_os = "windows")]
mod etw;
#[cfg(target_os = "linux")]
mod user_events;
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod noop;
