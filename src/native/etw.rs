use crate::{values::*};
use chrono::{Datelike, Timelike};
use std::{cell::RefCell, ops::DerefMut, pin::Pin, sync::Arc, time::SystemTime};
use tracelogging::*;
use tracelogging_dynamic::EventBuilder;
use tracing_subscriber::registry::{LookupSpan, SpanRef};

use super::ProviderGroup;

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

pub(crate) struct EventBuilderWrapper<'a> {
    pub(crate) eb: &'a mut tracelogging_dynamic::EventBuilder,
}

impl AddFieldAndValue for EventBuilderWrapper<'_> {
    fn add_field_value(&mut self, fv: &FieldAndValue) {
        match fv.value {
            ValueTypes::None => (),
            ValueTypes::v_u64(u) => {
                self.eb.add_u64(fv.field_name, *u, OutType::Unsigned, 0);
            }
            ValueTypes::v_i64(i) => {
                self.eb.add_i64(fv.field_name, *i, OutType::Signed, 0);
            }
            ValueTypes::v_u128(u) => unsafe {
                self.eb.add_u64_sequence(
                    fv.field_name,
                    core::slice::from_raw_parts(&(u.to_le_bytes()) as *const u8 as *const u64, 2),
                    OutType::Hex,
                    0,
                );
            },
            ValueTypes::v_i128(i) => unsafe {
                self.eb.add_u64_sequence(
                    fv.field_name,
                    core::slice::from_raw_parts(&(i.to_le_bytes()) as *const u8 as *const u64, 2),
                    OutType::Hex,
                    0,
                );
            },
            ValueTypes::v_f64(f) => {
                self.eb.add_f64(fv.field_name, *f, OutType::Signed, 0);
            }
            ValueTypes::v_bool(b) => {
                self.eb
                    .add_bool32(fv.field_name, *b as i32, OutType::Boolean, 0);
            }
            ValueTypes::v_str(ref s) => {
                self.eb
                    .add_str8(fv.field_name, s.as_ref(), OutType::String, 0);
            }
            ValueTypes::v_char(c) => {
                self.eb.add_u8(fv.field_name, *c as u8, OutType::String, 0);
            }
        }
    }
}

pub(crate) struct ProviderWrapper {
    provider: tracelogging_dynamic::Provider,
}

impl ProviderWrapper {
    pub(crate) fn new(
        provider_name: &str,
        provider_id: &tracelogging::Guid,
        provider_group: &ProviderGroup,
    ) -> Pin<Arc<Self>> {
        let mut options = tracelogging_dynamic::Provider::options();
        if let ProviderGroup::Windows(guid) = provider_group {
            options = *options.group_id(guid);
        }

        let wrapper = Arc::pin(ProviderWrapper {
            provider: tracelogging_dynamic::Provider::new_with_id(
                provider_name,
                &options,
                provider_id,
            ),
        });
        unsafe {
            wrapper.as_ref().get_provider().register();
        }

        wrapper
    }

    #[inline]
    pub(crate) fn enabled(&self, level: u8, keyword: u64) -> bool {
        return self
            .provider
            .enabled(tracelogging::Level::from_int(level), keyword);
    }

    fn get_provider(self: Pin<&Self>) -> Pin<&tracelogging_dynamic::Provider> {
        unsafe { self.map_unchecked(|s| &s.provider) }
    }

    pub(crate) fn span_start<'a, 'b, R>(
        self: Pin<&Self>,
        span: &'b SpanRef<'a, R>,
        timestamp: SystemTime,
        activity_id: &[u8; 16],
        related_activity_id: &[u8; 16],
        fields: &'b [&'static str],
        values: &'b [ValueTypes],
        level: u8,
        keyword: u64,
        event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
        let span_name = span.name();

        EBW.with(|eb| {
            let mut eb = eb.borrow_mut();

            eb.reset(span_name, level.into(), keyword, event_tag);
            eb.opcode(Opcode::Start);

            eb.add_systemtime(
                "start time",
                &Into::<Win32SystemTime>::into(timestamp).st,
                OutType::DateTimeUtc,
                0,
            );

            let mut ebw = EventBuilderWrapper { eb: eb.deref_mut() };

            for (f, v) in fields.iter().zip(values.iter()) {
                ebw.add_field_value(&FieldAndValue {
                    field_name: f,
                    value: v,
                });
            }

            let related = tracelogging_dynamic::Guid::from_bytes_le(related_activity_id);
            let _ = eb.write(
                &self.get_provider(),
                Some(&tracelogging_dynamic::Guid::from_bytes_le(&activity_id)),
                if related_activity_id[0] != 0 {
                    Some(&related)
                } else {
                    None
                },
            );
        });
    }

    pub(crate) fn span_stop<'a, 'b, R>(
        self: Pin<&Self>,
        span: &'b SpanRef<'a, R>,
        timestamp: SystemTime,
        activity_id: &[u8; 16],
        related_activity_id: &[u8; 16],
        fields: &'b [&'static str],
        values: &'b [ValueTypes],
        level: u8,
        keyword: u64,
        event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
        let span_name = span.name();

        EBW.with(|eb| {
            let mut eb = eb.borrow_mut();

            eb.reset(span_name, level.into(), keyword, event_tag);
            eb.opcode(Opcode::Stop);

            eb.add_systemtime(
                "stop time",
                &Into::<Win32SystemTime>::into(timestamp).st,
                OutType::DateTimeUtc,
                0,
            );

            let mut ebw = EventBuilderWrapper { eb: eb.deref_mut() };

            for (f, v) in fields.iter().zip(values.iter()) {
                ebw.add_field_value(&FieldAndValue {
                    field_name: f,
                    value: v,
                });
            }

            let related = tracelogging_dynamic::Guid::from_bytes_le(related_activity_id);
            let _ = eb.write(
                &self.get_provider(),
                Some(&tracelogging_dynamic::Guid::from_bytes_le(&activity_id)),
                if related_activity_id[0] != 0 {
                    Some(&related)
                } else {
                    None
                },
            );
        });
    }

    pub(crate) fn write_record(
        self: Pin<&Self>,
        timestamp: SystemTime,
        activity_id: &[u8; 16],
        related_activity_id: &[u8; 16],
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

            event.record(&mut EventBuilderWrapper { eb: eb.deref_mut() });

            let related = tracelogging_dynamic::Guid::from_bytes_le(related_activity_id);
            let _ = eb.write(
                &self.get_provider(),
                Some(&tracelogging_dynamic::Guid::from_bytes_le(&activity_id)),
                if related_activity_id[0] != 0 {
                    Some(&related)
                } else {
                    None
                },
            );
        });
    }
}
