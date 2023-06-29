use std::borrow::Cow;

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

// pub fn EtwFilter() -> FilterFn {
//     FilterFn::new(|metadata| { true })
// }

pub(crate) struct GuidWrapper([u8; 16]);

impl From<&tracelogging::Guid> for GuidWrapper {
    fn from(value: &tracelogging::Guid) -> Self {
        Self(*value.as_bytes_raw())
    }
}

impl From<&eventheader::Guid> for GuidWrapper {
    fn from(value: &eventheader::Guid) -> Self {
        Self(*value.as_bytes_raw())
    }
}

impl From<GuidWrapper> for tracelogging::Guid {
    fn from(value: GuidWrapper) -> Self {
        unsafe { core::mem::transmute(value.0) }
    }
}

impl From<GuidWrapper> for eventheader::Guid {
    fn from(value: GuidWrapper) -> Self {
        unsafe { core::mem::transmute(value.0) }
    }
}

#[derive(Clone)]
pub(crate) enum ProviderGroup {
    Unset,
    #[allow(dead_code)]
    Windows(tracelogging::Guid),
    #[allow(dead_code)]
    Linux(Cow<'static, str>),
}

pub(crate) trait AddFieldAndValue {
    fn add_field_value(&mut self, fv: &crate::values::FieldAndValue);
}
