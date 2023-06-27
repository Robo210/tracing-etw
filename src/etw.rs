use crate::{
    activities::Activities,
    layer::{map_level, ProviderWrapper},
    EtwLayer, EventBuilderWrapper,
};
use chrono::{Datelike, Timelike};
use std::{cell::RefCell, fmt::Write, pin::Pin, time::SystemTime};
use tracelogging::*;
use tracelogging_dynamic::EventBuilder;
use tracing::{field, span};
use tracing_subscriber::registry::{LookupSpan, SpanRef};

thread_local! {static EBW: std::cell::RefCell<EventBuilder>  = RefCell::new(EventBuilder::new());}

struct Win32SystemTime {
    st: [u16; 8],
}

impl From<std::time::SystemTime> for Win32SystemTime {
    fn from(value: std::time::SystemTime) -> Self {
        let dt = chrono::DateTime::from(value);

        Win32SystemTime {
            st: [
                dt.year() as u16,
                dt.month() as u16,
                0,
                dt.day() as u16,
                dt.hour() as u16,
                dt.minute() as u16,
                dt.second() as u16,
                (dt.nanosecond() / 1000000) as u16,
            ],
        }
    }
}

struct ValueVisitor<'a> {
    eb: &'a mut EventBuilder,
}
impl<'a> field::Visit for ValueVisitor<'a> {
    fn record_debug(&mut self, field: &field::Field, value: &dyn std::fmt::Debug) {
        let mut string = String::with_capacity(10);
        if write!(string, "{:?}", value).is_err() {
            // TODO: Needs to do a heap allocation
            return;
        }

        self.eb.add_str8(field.name(), string, OutType::String, 0);
    }

    fn record_f64(&mut self, field: &field::Field, value: f64) {
        self.eb.add_f64(field.name(), value, OutType::Signed, 0);
    }

    fn record_i64(&mut self, field: &field::Field, value: i64) {
        self.eb.add_i64(field.name(), value, OutType::Signed, 0);
    }

    fn record_u64(&mut self, field: &field::Field, value: u64) {
        self.eb.add_u64(field.name(), value, OutType::Unsigned, 0);
    }

    fn record_i128(&mut self, field: &field::Field, value: i128) {
        unsafe {
            self.eb.add_u64_sequence(
                field.name(),
                core::slice::from_raw_parts(&value.to_le_bytes() as *const u8 as *const u64, 2),
                OutType::Hex,
                0,
            );
        }
    }

    fn record_u128(&mut self, field: &field::Field, value: u128) {
        unsafe {
            self.eb.add_u64_sequence(
                field.name(),
                core::slice::from_raw_parts(&value.to_le_bytes() as *const u8 as *const u64, 2),
                OutType::Hex,
                0,
            );
        }
    }

    fn record_bool(&mut self, field: &field::Field, value: bool) {
        self.eb
            .add_bool32(field.name(), value as i32, OutType::Boolean, 0);
    }

    fn record_str(&mut self, field: &field::Field, value: &str) {
        self.eb.add_str8(field.name(), value, OutType::String, 0);
    }

    fn record_error(&mut self, field: &field::Field, value: &(dyn std::error::Error + 'static)) {}
}

impl ProviderWrapper {
    pub(crate) fn new_span<'a, R>(
        self: Pin<&Self>,
        span: &SpanRef<'a, R>,
        attrs: &span::Attributes<'_>,
        level: u8,
        keyword: u64,
        event_tag: u32,
    ) -> EventBuilderWrapper
    where
        R: LookupSpan<'a>,
    {
        let span_name = span.name();

        let mut eb = tracelogging_dynamic::EventBuilder::new();
        eb.reset(span_name, level.into(), keyword, event_tag);

        attrs.values().record(&mut ValueVisitor { eb: &mut eb });

        EventBuilderWrapper { eb }
    }

    pub(crate) fn span_start<'a, R>(
        self: Pin<&Self>,
        eb: &mut EventBuilder,
        span: &SpanRef<'a, R>,
        timestamp: SystemTime,
        activities: &Activities,
        level: u8,
        keyword: u64,
        event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
        let span_name = span.name();

        eb.opcode(Opcode::Start);

        eb.add_systemtime(
            "start time",
            &Into::<Win32SystemTime>::into(timestamp).st,
            OutType::DateTimeUtc,
            0,
        );

        let _ = eb.write(
            &self.get_provider(),
            Some(&tracelogging_dynamic::Guid::from_bytes_le(
                &activities.activity_id,
            )),
            activities
                .parent_activity_id
                .map(|id| tracelogging_dynamic::Guid::from_bytes_le(&id))
                .as_ref(),
        );

        eb.reset(span_name, level.into(), keyword, event_tag);
        eb.opcode(Opcode::Stop);
    }

    pub(crate) fn span_stop<'a, R>(
        self: Pin<&Self>,
        eb: &mut EventBuilder,
        _span: &SpanRef<'a, R>,
        timestamp: SystemTime,
        activities: &Activities,
    ) where
        R: LookupSpan<'a>,
    {
        eb.add_systemtime(
            "stop time",
            &Into::<Win32SystemTime>::into(timestamp).st,
            OutType::DateTimeUtc,
            0,
        );

        let _ = eb.write(
            &self.get_provider(),
            Some(&tracelogging_dynamic::Guid::from_bytes_le(
                &activities.activity_id,
            )),
            activities
                .parent_activity_id
                .map(|id| tracelogging_dynamic::Guid::from_bytes_le(&id))
                .as_ref(),
        );
    }

    pub(crate) fn add_values(values: &span::Record<'_>, eb: &mut EventBuilder) {
        values.record(&mut ValueVisitor { eb });
    }

    pub(crate) fn write_record(
        self: Pin<&Self>,
        timestamp: SystemTime,
        activities: &Activities,
        event_name: &str,
        level: u8,
        keyword: u64,
        event: &tracing::Event<'_>,
    ) {
        EBW.with(|eb| {
            let mut eb = eb.borrow_mut();

            eb.reset(&event_name, level.into(), keyword, 0);
            eb.opcode(Opcode::Info);

            eb.add_systemtime(
                "time",
                &Into::<Win32SystemTime>::into(timestamp).st,
                OutType::DateTimeUtc,
                0,
            );

            event.record(&mut ValueVisitor { eb: &mut eb });

            let _ = eb.write(
                &self.get_provider(),
                Some(&tracelogging_dynamic::Guid::from_bytes_le(
                    &activities.activity_id,
                )),
                activities
                    .parent_activity_id
                    .map(|id| tracelogging_dynamic::Guid::from_bytes_le(&id))
                    .as_ref(),
            );
        });
    }
}
