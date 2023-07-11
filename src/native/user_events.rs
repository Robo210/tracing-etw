use crate::{map_level, values::*};
use eventheader::*;
use eventheader_dynamic::EventBuilder;
use std::{cell::RefCell, ops::DerefMut, pin::Pin, sync::Arc, time::SystemTime};
use tracing_subscriber::registry::{LookupSpan, SpanRef};

use super::ProviderGroup;

thread_local! {static EBW: std::cell::RefCell<EventBuilder>  = RefCell::new(EventBuilder::new());}

pub(crate) struct PayloadFieldVisitor<'a> {
    pub(crate) eb: &'a mut eventheader_dynamic::EventBuilder,
}

impl<'a> PayloadFieldVisitor<'a> {
    fn make_visitor(
        eb: &'a mut eventheader_dynamic::EventBuilder,
    ) -> VisitorWrapper<PayloadFieldVisitor<'a>> {
        VisitorWrapper::from(PayloadFieldVisitor { eb })
    }
}

impl<T> AddFieldAndValue<T> for PayloadFieldVisitor<'_> {
    fn add_field_value(&mut self, fv: &FieldAndValue) {
        match fv.value {
            ValueTypes::None => (),
            ValueTypes::v_u64(u) => {
                self.eb
                    .add_value(fv.field_name, *u, FieldFormat::Default, 0);
            }
            ValueTypes::v_i64(i) => {
                self.eb
                    .add_value(fv.field_name, *i, FieldFormat::SignedInt, 0);
            }
            ValueTypes::v_u128(u) => {
                self.eb
                    .add_value(fv.field_name, u.to_le_bytes(), FieldFormat::Default, 0);
            }
            ValueTypes::v_i128(i) => {
                self.eb
                    .add_value(fv.field_name, i.to_le_bytes(), FieldFormat::Default, 0);
            }
            ValueTypes::v_f64(f) => {
                self.eb.add_value(fv.field_name, *f, FieldFormat::Float, 0);
            }
            ValueTypes::v_bool(b) => {
                self.eb
                    .add_value(fv.field_name, *b, FieldFormat::Boolean, 0);
            }
            ValueTypes::v_str(ref s) => {
                self.eb
                    .add_str(fv.field_name, s.as_ref(), FieldFormat::Default, 0);
            }
            ValueTypes::v_char(c) => {
                self.eb
                    .add_value(fv.field_name, *c, FieldFormat::StringUtf, 0);
            }
        }
    }
}

#[doc(hidden)]
pub struct Provider {
    provider: std::sync::RwLock<eventheader_dynamic::Provider>,
}

impl Provider {
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

    fn get_provider(self: Pin<&Self>) -> Pin<&std::sync::RwLock<eventheader_dynamic::Provider>> {
        unsafe { self.map_unchecked(|s| &s.provider) }
    }
}

impl crate::native::EventWriter for Provider {
    fn new<G>(
        provider_name: &str,
        _: &G,
        provider_group: &ProviderGroup,
        default_keyword: u64,
    ) -> Pin<Arc<Self>>
    where
        for<'a> &'a G: Into<crate::native::GuidWrapper>,
    {
        let mut options = eventheader_dynamic::Provider::new_options();
        if let ProviderGroup::Linux(ref name) = provider_group {
            options = *options.group_name(&name);
        }
        let mut provider = eventheader_dynamic::Provider::new(provider_name, &options);

        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::ERROR)),
            default_keyword,
        );
        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::WARN)),
            default_keyword,
        );
        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::INFO)),
            default_keyword,
        );
        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::DEBUG)),
            default_keyword,
        );
        provider.register_set(
            eventheader_dynamic::Level::from_int(map_level(&tracing::Level::TRACE)),
            default_keyword,
        );

        Arc::pin(Provider {
            provider: std::sync::RwLock::new(provider),
        })
    }

    #[inline]
    fn enabled(&self, level: u8, keyword: u64) -> bool {
        let es = self
            .provider
            .read()
            .unwrap()
            .find_set(eventheader_dynamic::Level::from_int(level), keyword);
        return if let Some(s) = es { s.enabled() } else { false };
    }

    #[inline(always)]
    fn supports_enable_callback() -> bool {
        false
    }

    fn span_start<'a, 'b, R>(
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

            let mut pfv = PayloadFieldVisitor { eb: eb.deref_mut() };

            for (f, v) in fields.iter().zip(values.iter()) {
                <PayloadFieldVisitor<'_> as AddFieldAndValue<PayloadFieldVisitor<'_>>>::add_field_value(&mut pfv, &FieldAndValue {
                    field_name: f,
                    value: v,
                });
            }

            let _ = eb.write(
                &es,
                if activity_id[0] != 0 {
                    Some(activity_id)
                } else {
                    None
                },
                if related_activity_id[0] != 0 {
                    Some(&related_activity_id)
                } else {
                    None
                },
            );
        });
    }

    fn span_stop<'a, 'b, R>(
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

            let mut pfv = PayloadFieldVisitor { eb: eb.deref_mut() };

            for (f, v) in fields.iter().zip(values.iter()) {
                <PayloadFieldVisitor<'_> as AddFieldAndValue<PayloadFieldVisitor<'_>>>::add_field_value(&mut pfv, &FieldAndValue {
                    field_name: f,
                    value: v,
                });
            }

            let _ = eb.write(
                &es,
                if activity_id[0] != 0 {
                    Some(activity_id)
                } else {
                    None
                },
                if related_activity_id[0] != 0 {
                    Some(&related_activity_id)
                } else {
                    None
                },
            );
        });
    }

    fn write_record(
        self: Pin<&Self>,
        timestamp: SystemTime,
        current_span: u64,
        parent_span: u64,
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

        let mut activity_id: [u8; 16] = [0; 16];
        activity_id[0] = if current_span != 0 {
            let (_, half) = activity_id.split_at_mut(8);
            half.copy_from_slice(&current_span.to_le_bytes());
            1
        } else {
            0
        };

        let mut related_activity_id: [u8; 16] = [0; 16];
        related_activity_id[0] = if parent_span != 0 {
            let (_, half) = related_activity_id.split_at_mut(8);
            half.copy_from_slice(&parent_span.to_le_bytes());
            1
        } else {
            0
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

            let mut visitor = PayloadFieldVisitor::make_visitor(eb.deref_mut());
            event.record(&mut visitor);

            let _ = eb.write(
                &es,
                if activity_id[0] != 0 {
                    Some(&activity_id)
                } else {
                    None
                },
                if related_activity_id[0] != 0 {
                    Some(&related_activity_id)
                } else {
                    None
                },
            );
        });
    }
}
