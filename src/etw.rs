use crate::{layer::{map_level, ProviderWrapper}, EtwLayer};
use chrono::{Datelike, Timelike};
use std::{cell::RefCell, pin::Pin, time::SystemTime};
use tracelogging::*;
use tracelogging_dynamic::EventBuilder;

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

impl ProviderWrapper {
    pub(crate) fn write_record(
        self: Pin<&Self>,
        timestamp: SystemTime,
        event_name: &str,
        keyword: u64,
        event: &tracing::Event<'_>,
        layer: &EtwLayer,
    ) {
        let level = map_level(event.metadata().level());

        if !self.enabled(level, keyword) {
            return;
        }

        EBW.with(|eb| {
            let mut eb = eb.borrow_mut();

            if !layer.emit_common_schema_events {
                eb.reset(&event_name, level.into(), keyword, 0);
                eb.opcode(Opcode::Info);

                eb.add_systemtime(
                    "time",
                    &Into::<Win32SystemTime>::into(timestamp).st,
                    OutType::DateTimeUtc,
                    0,
                );

                //let payload = format!("{}", record.args());
                //eb.add_str8("Payload", payload, OutType::Utf8, 0);
            }

            let _ = eb.write(&self.get_provider(), None, None);
        });
    }
}
