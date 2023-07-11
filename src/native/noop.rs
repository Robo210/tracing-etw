use std::{pin::Pin, sync::Arc, time::SystemTime};

use tracing_subscriber::registry::{LookupSpan, SpanRef};

use crate::values::*;

use crate::native::ProviderGroup;

pub(crate) struct EventBuilderWrapper<'a> {
    _p: core::marker::PhantomData<&'a u8>,
}

impl<T> AddFieldAndValue<T> for EventBuilderWrapper<'_> {
    fn add_field_value(&mut self, _fv: &FieldAndValue) {}
}

#[doc(hidden)]
pub struct Provider;

impl crate::native::EventWriter for Provider {
    fn new<G>(
        _provider_name: &str,
        _provider_id: &G,
        _provider_group: &ProviderGroup,
        _default_keyword: u64,
    ) -> Pin<Arc<Self>>
    where
        for<'a> &'a G: Into<crate::native::GuidWrapper>,
    {
        Arc::pin(Self)
    }

    #[inline(always)]
    fn enabled(&self, _level: u8, _keyword: u64) -> bool {
        false
    }

    #[inline(always)]
    fn supports_enable_callback() -> bool {
        false
    }

    fn span_start<'a, 'b, R>(
        self: Pin<&Self>,
        _span: &'b SpanRef<'a, R>,
        _timestamp: SystemTime,
        _activity_id: &[u8; 16],
        _related_activity_id: &[u8; 16],
        _fields: &'b [&'static str],
        _values: &'b [ValueTypes],
        _level: u8,
        _keyword: u64,
        _event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
    }

    fn span_stop<'a, 'b, R>(
        self: Pin<&Self>,
        _span: &'b SpanRef<'a, R>,
        _timestamp: SystemTime,
        _activity_id: &[u8; 16],
        _related_activity_id: &[u8; 16],
        _fields: &'b [&'static str],
        _values: &'b [ValueTypes],
        _level: u8,
        _keyword: u64,
        _event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
    }

    fn write_record(
        self: Pin<&Self>,
        _timestamp: SystemTime,
        _current_span: u64,
        _parent_span: u64,
        _event_name: &str,
        _level: u8,
        _keyword: u64,
        _event: &tracing::Event<'_>,
    ) {
    }
}
