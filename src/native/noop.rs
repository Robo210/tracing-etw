use std::{pin::Pin, time::SystemTime, sync::Arc};

use tracing_subscriber::registry::{LookupSpan, SpanRef};

use crate::{activities::Activities, providerwrapper::{AddFieldAndValue, ProviderGroup}, values::FieldAndValue};

pub(crate) struct EventBuilderWrapper<'a> {
    _p: core::marker::PhantomData<&'a u8>
}

impl AddFieldAndValue for EventBuilderWrapper<'_> {
    fn add_field_value(&mut self, _fv: &FieldAndValue) {
    }
}

pub(crate) struct ProviderWrapper;

impl ProviderWrapper {
    pub(crate) fn new(
        _provider_name: &str,
        _provider_id: &tracelogging::Guid,
        _provider_group: &ProviderGroup,
    ) -> Pin<Arc<Self>> {
        Arc::pin(Self)
    }

    #[inline(always)]
    pub(crate) fn enabled(&self, _level: u8, _keyword: u64) -> bool {
        false
    }

    pub(crate) fn span_start<'a, R>(
        self: Pin<&Self>,
        _span: &SpanRef<'a, R>,
        _timestamp: SystemTime,
        _activities: &Activities,
        _data: &[crate::values::FieldAndValue],
        _level: u8,
        _keyword: u64,
        _event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
    }

    pub(crate) fn span_stop<'a, R>(
        self: Pin<&Self>,
        _span: &SpanRef<'a, R>,
        _timestamp: SystemTime,
        _activities: &Activities,
        _data: &[crate::values::FieldAndValue],
        _level: u8,
        _keyword: u64,
        _event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
    }

    pub(crate) fn write_record(
        self: Pin<&Self>,
        _timestamp: SystemTime,
        _activities: &Activities,
        _event_name: &str,
        _level: u8,
        _keyword: u64,
        _event: &tracing::Event<'_>,
    ) {
    }
}
