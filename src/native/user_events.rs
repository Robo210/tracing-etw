use crate::{
    activities::Activities,
    providerwrapper::{ProviderWrapper}, values::*
};
use chrono::{Datelike, Timelike};
use std::{cell::RefCell, fmt::Write, pin::Pin, time::SystemTime};
use eventheader::*;
use eventheader_dynamic::EventBuilder;
use tracing::{field};
use tracing_subscriber::registry::{LookupSpan, SpanRef};

thread_local! {static EBW: std::cell::RefCell<EventBuilder>  = RefCell::new(EventBuilder::new());}

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

        add_field_value(self.eb, &FieldAndValue { field_name: field.name(), value: ValueTypes::from(string) })
    }

    fn record_f64(&mut self, field: &field::Field, value: f64) {
        add_field_value(self.eb, &FieldAndValue { field_name: field.name(), value: ValueTypes::from(value) })
    }

    fn record_i64(&mut self, field: &field::Field, value: i64) {
        add_field_value(self.eb, &FieldAndValue { field_name: field.name(), value: ValueTypes::from(value) })
    }

    fn record_u64(&mut self, field: &field::Field, value: u64) {
        add_field_value(self.eb, &FieldAndValue { field_name: field.name(), value: ValueTypes::from(value) })
    }

    fn record_i128(&mut self, field: &field::Field, value: i128) {
        add_field_value(self.eb, &FieldAndValue { field_name: field.name(), value: ValueTypes::from(value) })
    }

    fn record_u128(&mut self, field: &field::Field, value: u128) {
        add_field_value(self.eb, &FieldAndValue { field_name: field.name(), value: ValueTypes::from(value) })
    }

    fn record_bool(&mut self, field: &field::Field, value: bool) {
        add_field_value(self.eb, &FieldAndValue { field_name: field.name(), value: ValueTypes::from(value) })
    }

    fn record_str(&mut self, field: &field::Field, value: &str) {
        add_field_value(self.eb, &FieldAndValue { field_name: field.name(), value: ValueTypes::from(value.to_string()) })
    }

    fn record_error(&mut self, field: &field::Field, value: &(dyn std::error::Error + 'static)) {}
}

fn add_field_value(eb: &mut EventBuilder, fv: &FieldAndValue) {
    match fv.value {
        ValueTypes::None => (),
        ValueTypes::v_u64(u) => { eb.add_value(fv.field_name, u, FieldFormat::Default, 0); }
        ValueTypes::v_i64(i) => { eb.add_value(fv.field_name, i, FieldFormat::SignedInt, 0); }
        ValueTypes::v_u128(u) => unsafe {
            eb.add_value_sequence(
                fv.field_name,
                core::slice::from_raw_parts(&u.to_le_bytes() as *const u8 as *const u64, 2),
                FieldFormat::HexInt,
                0,
            );
        }
        ValueTypes::v_i128(i) => unsafe {
            eb.add_value_sequence(
                fv.field_name,
                core::slice::from_raw_parts(&i.to_le_bytes() as *const u8 as *const u64, 2),
                FieldFormat::HexInt,
                0,
            );
        }
        ValueTypes::v_f64(f) => { eb.add_value(fv.field_name, f, FieldFormat::Float, 0); }
        ValueTypes::v_bool(b) => { eb.add_value(fv.field_name, b as i32, FieldFormat::Boolean, 0); }
        ValueTypes::v_str(ref s) => { eb.add_str(fv.field_name, s.as_ref(), FieldFormat::Default, 0); }
        ValueTypes::v_char(c) => { eb.add_value(fv.field_name, c as u8, FieldFormat::String8, 0); }
    }
}

impl ProviderWrapper {
    pub(crate) fn span_start<'a, R>(
        self: Pin<&Self>,
        span: &SpanRef<'a, R>,
        timestamp: SystemTime,
        activities: &Activities,
        data: &[crate::values::FieldAndValue],
        level: u8,
        keyword: u64,
        event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
        let span_name = span.name();

        EBW.with(|eb| {
            let mut eb = eb.borrow_mut();

            eb.reset(span_name, event_tag as u16);
            eb.opcode(Opcode::ActivityStart);

            eb.add_value(
                "start time",
                timestamp
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                        FieldFormat::Time,
                0,
            );

            for fv in data {
                add_field_value(&mut eb, fv);
            }

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

    pub(crate) fn span_stop<'a, R>(
        self: Pin<&Self>,
        span: &SpanRef<'a, R>,
        timestamp: SystemTime,
        activities: &Activities,
        data: &[crate::values::FieldAndValue],
        level: u8,
        keyword: u64,
        event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
        let span_name = span.name();

        EBW.with(|eb| {
            let mut eb = eb.borrow_mut();

            eb.reset(span_name, event_tag as u16);
            eb.opcode(Opcode::ActivityStop);

            eb.add_value(
                "stop time",
                timestamp
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                        FieldFormat::Time,
                0,
            );

            for fv in data {
                add_field_value(&mut eb, fv);
            }

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

            eb.reset(&event_name, 0);
            eb.opcode(Opcode::Info);

            eb.add_value(
                "time",
                timestamp
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                        FieldFormat::Time,
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
