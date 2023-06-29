use crate::{
    activities::Activities,
    map_level,
    values::*,
};
use eventheader::*;
use eventheader_dynamic::EventBuilder;
use std::{cell::RefCell, ops::DerefMut, pin::Pin, sync::Arc, time::SystemTime};
use tracing_subscriber::registry::{LookupSpan, SpanRef};

use super::ProviderGroup;

thread_local! {static EBW: std::cell::RefCell<EventBuilder>  = RefCell::new(EventBuilder::new());}

pub(crate) struct EventBuilderWrapper<'a> {
    pub(crate) eb: &'a mut eventheader_dynamic::EventBuilder,
}

impl AddFieldAndValue for EventBuilderWrapper<'_> {
    fn add_field_value(&mut self, fv: &FieldAndValue) {
        match fv.value {
            ValueTypes::None => (),
            ValueTypes::v_u64(u) => {
                self.eb.add_value(fv.field_name, u, FieldFormat::Default, 0);
            }
            ValueTypes::v_i64(i) => {
                self.eb
                    .add_value(fv.field_name, i, FieldFormat::SignedInt, 0);
            }
            ValueTypes::v_u128(u) => unsafe {
                self.eb.add_value_sequence(
                    fv.field_name,
                    core::slice::from_raw_parts(&u.to_le_bytes() as *const u8 as *const u64, 2),
                    FieldFormat::HexInt,
                    0,
                );
            },
            ValueTypes::v_i128(i) => unsafe {
                self.eb.add_value_sequence(
                    fv.field_name,
                    core::slice::from_raw_parts(&i.to_le_bytes() as *const u8 as *const u64, 2),
                    FieldFormat::HexInt,
                    0,
                );
            },
            ValueTypes::v_f64(f) => {
                self.eb.add_value(fv.field_name, f, FieldFormat::Float, 0);
            }
            ValueTypes::v_bool(b) => {
                self.eb
                    .add_value(fv.field_name, b as i32, FieldFormat::Boolean, 0);
            }
            ValueTypes::v_str(ref s) => {
                self.eb
                    .add_str(fv.field_name, s.as_ref(), FieldFormat::Default, 0);
            }
            ValueTypes::v_char(c) => {
                self.eb
                    .add_value(fv.field_name, c as u8, FieldFormat::String8, 0);
            }
        }
    }
}

pub(crate) struct ProviderWrapper {
    provider: std::sync::RwLock<eventheader_dynamic::Provider>,
}

impl ProviderWrapper {
    fn find_set(
        self: Pin<&Self>,
        level: eventheader_dynamic::Level,
        keyword: u64,
    ) -> Option<Arc<eventheader_dynamic::EventSet>> {
        self.get_provider().read().unwrap().find_set(level, keyword)
    }

    fn register_set(
        self: Pin<&Self>,
        level: eventheader_dynamic::Level,
        keyword: u64,
    ) -> Arc<eventheader_dynamic::EventSet> {
        self.get_provider()
            .write()
            .unwrap()
            .register_set(level, keyword)
    }

    fn get_provider(
        self: Pin<&Self>,
    ) -> Pin<&std::sync::RwLock<eventheader_dynamic::Provider>> {
        unsafe { self.map_unchecked(|s| &s.provider) }
    }

    pub(crate) fn new(
        provider_name: &str,
        _: &eventheader::Guid,
        provider_group: &ProviderGroup,
    ) -> Pin<Arc<Self>> {
        let mut options = eventheader_dynamic::Provider::new_options();
        if let ProviderGroup::Linux(ref name) = provider_group {
            options = *options.group_name(&name);
        }
        let mut provider = eventheader_dynamic::Provider::new(provider_name, &options);

        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::ERROR)),
            1,
        );
        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::WARN)),
            1,
        );
        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::INFO)),
            1,
        );
        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::DEBUG)),
            1,
        );
        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::TRACE)),
            1,
        );

        Arc::pin(ProviderWrapper {
            provider: std::sync::RwLock::new(eventheader_dynamic::Provider::new(
                provider_name,
                &options,
            )),
        })
    }

    #[inline]
    pub(crate) fn enabled(&self, level: u8, keyword: u64) -> bool {
        let es = self
            .provider
            .read()
            .unwrap()
            .find_set(eventheader_dynamic::Level::from_int(level), keyword);
        if es.is_some() {
            es.unwrap().enabled()
        } else {
            false
        }
    }

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

        let es = if let Some(es) = self.find_set(level.into(), keyword) {
            es
        } else {
            self.register_set(level.into(), keyword)
        };

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

            let mut ebw = EventBuilderWrapper { eb: eb.deref_mut() };
            for fv in data {
                ebw.add_field_value(fv);
            }

            let _ = eb.write(
                &es,
                Some(&activities.activity_id),
                activities.parent_activity_id.as_ref(),
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

        let es = if let Some(es) = self.find_set(level.into(), keyword) {
            es
        } else {
            self.register_set(level.into(), keyword)
        };

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

            let mut ebw = EventBuilderWrapper { eb: eb.deref_mut() };
            for fv in data {
                ebw.add_field_value(fv);
            }

            let _ = eb.write(
                &es,
                Some(&activities.activity_id),
                activities.parent_activity_id.as_ref(),
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
        let es = if let Some(es) = self.find_set(level.into(), keyword) {
            es
        } else {
            self.register_set(level.into(), keyword)
        };

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

            event.record(&mut EventBuilderWrapper { eb: eb.deref_mut() });

            let _ = eb.write(
                &es,
                Some(&activities.activity_id),
                activities.parent_activity_id.as_ref(),
            );
        });
    }
}
