#[cfg(target_os = "windows")]
#[doc(hidden)]
pub mod etw_cs;
#[cfg(target_os = "windows")]
#[doc(hidden)]
pub use etw_cs::CommonSchemaProvider as Provider;

impl crate::native::EventMode for Provider {type Provider = Provider;}
