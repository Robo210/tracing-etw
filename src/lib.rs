#[macro_use]
extern crate lazy_static;

mod layer;
mod native;
mod values;

pub use layer::*;

#[inline]
pub(crate) const fn map_level(level: &tracing::Level) -> u8 {
    match level {
        &tracing::Level::ERROR => tracelogging::Level::Error.as_int(),
        &tracing::Level::WARN => tracelogging::Level::Warning.as_int(),
        &tracing::Level::INFO => tracelogging::Level::Informational.as_int(),
        &tracing::Level::DEBUG => tracelogging::Level::Verbose.as_int(),
        &tracing::Level::TRACE => tracelogging::Level::Verbose.as_int() + 1,
    }
}
