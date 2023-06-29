use std::{pin::Pin, time::SystemTime};

use tracing_subscriber::registry::{SpanRef, LookupSpan};

use crate::{activities::Activities, providerwrapper::ProviderWrapper};

impl ProviderWrapper {
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
