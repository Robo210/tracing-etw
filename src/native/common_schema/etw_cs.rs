use crate::values::*;
use std::{
    cell::RefCell,
    io::{Cursor, Write},
    mem::MaybeUninit,
    ops::DerefMut,
    pin::Pin,
    sync::Arc,
    time::SystemTime,
};
use tracelogging::*;
use tracelogging_dynamic::EventBuilder;
use tracing_subscriber::registry::{LookupSpan, SpanRef};

use crate::native::ProviderGroup;

thread_local! {static EBW: std::cell::RefCell<EventBuilder>  = RefCell::new(EventBuilder::new());}

pub(crate) struct CommonSchemaPartCBuilder<'a> {
    pub(crate) eb: &'a mut tracelogging_dynamic::EventBuilder,
}

impl<'a> CommonSchemaPartCBuilder<'a> {
    fn make_visitor(
        eb: &'a mut tracelogging_dynamic::EventBuilder,
    ) -> VisitorWrapper<CommonSchemaPartCBuilder<'a>> {
        VisitorWrapper::from(CommonSchemaPartCBuilder { eb })
    }
}

impl<T> AddFieldAndValue<T> for CommonSchemaPartCBuilder<'_> {
    fn add_field_value(&mut self, fv: &FieldAndValue) {
        let mut field_name: &'static str = fv.field_name;

        match field_name {
            "message" => {
                field_name = "Body";
                assert!(matches!(fv.value, ValueTypes::v_str(_)));
            }
            _ => (),
        };

        match fv.value {
            ValueTypes::None => (),
            ValueTypes::v_u64(u) => {
                self.eb.add_u64(field_name, *u, OutType::Default, 0);
            }
            ValueTypes::v_i64(i) => {
                self.eb.add_i64(field_name, *i, OutType::Default, 0);
            }
            ValueTypes::v_u128(u) => {
                // Or maybe add_binaryc?
                self.eb
                    .add_binary(field_name, u.to_le_bytes(), OutType::Default, 0);
            }
            ValueTypes::v_i128(i) => {
                // Or maybe add_binaryc?
                self.eb
                    .add_binary(field_name, i.to_le_bytes(), OutType::Default, 0);
            }
            ValueTypes::v_f64(f) => {
                self.eb.add_f64(field_name, *f, OutType::Default, 0);
            }
            ValueTypes::v_bool(b) => {
                // Or maybe add_u8 + OutType::Boolean?
                self.eb
                    .add_bool32(field_name, *b as i32, OutType::Default, 0);
            }
            ValueTypes::v_str(ref s) => {
                self.eb.add_str8(field_name, s.as_ref(), OutType::Utf8, 0);
            }
            ValueTypes::v_char(c) => {
                // Or add_str16 with a 1-char (BMP) or 2-char (surrogate-pair) string.
                self.eb.add_u16(field_name, *c as u16, OutType::String, 0);
            }
        }
    }
}

#[doc(hidden)]
pub struct CommonSchemaProvider {
    provider: tracelogging_dynamic::Provider,
}

fn callback_fn(
    _source_id: &Guid,
    _event_control_code: u32,
    _level: Level,
    _match_any_keyword: u64,
    _match_all_keyword: u64,
    _filter_data: usize,
    _callback_context: usize,
) {
    // Every time the enablement changes, reset the event-enabled cache
    tracing::callsite::rebuild_interest_cache();
}

impl CommonSchemaProvider {
    #[inline(always)]
    fn get_provider(self: Pin<&Self>) -> Pin<&tracelogging_dynamic::Provider> {
        unsafe { self.map_unchecked(|s| &s.provider) }
    }
}

impl crate::native::EventWriter for CommonSchemaProvider {
    fn new<G>(
        provider_name: &str,
        provider_id: &G,
        provider_group: &ProviderGroup,
        _default_keyword: u64,
    ) -> Pin<Arc<Self>>
    where
        for<'a> &'a G: Into<crate::native::GuidWrapper>,
    {
        let mut options = tracelogging_dynamic::Provider::options();
        if let ProviderGroup::Windows(guid) = provider_group {
            options.group_id(guid);
        }

        options.callback(callback_fn, 0);

        let wrapper = Arc::pin(Self {
            provider: tracelogging_dynamic::Provider::new_with_id(
                provider_name,
                &options,
                &provider_id.into().into(),
            ),
        });
        unsafe {
            wrapper.as_ref().get_provider().register();
        }

        wrapper
    }

    #[inline]
    fn enabled(&self, level: u8, keyword: u64) -> bool {
        self.provider
            .enabled(tracelogging::Level::from_int(level), keyword)
    }

    #[inline(always)]
    fn supports_enable_callback() -> bool {
        true
    }

    fn span_start<'a, 'b, R>(
        self: Pin<&Self>,
        _span: &'b SpanRef<'a, R>,
        _timestamp: SystemTime,
        _activity_id: &[u8; 16],
        _related_activity_id: &[u8; 16],
        _fields: &'b [crate::values::FieldValueIndex],
        _level: u8,
        _keyword: u64,
        _event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
    }

    fn span_stop<'a, 'b, R>(
        self: Pin<&Self>,
        span: &'b SpanRef<'a, R>,
        start_stop_times: (std::time::SystemTime, std::time::SystemTime),
        _activity_id: &[u8; 16],
        _related_activity_id: &[u8; 16],
        fields: &'b [crate::values::FieldValueIndex],
        level: u8,
        keyword: u64,
        event_tag: u32,
    ) where
        R: LookupSpan<'a>,
    {
        let span_name = span.name();

        let span_id = unsafe {
            let mut span_id = MaybeUninit::<[u8; 16]>::uninit();
            let mut cur = Cursor::new((&mut *span_id.as_mut_ptr()).as_mut_slice());
            write!(&mut cur, "{:16x}", span.id().into_u64()).expect("!write");
            span_id.assume_init()
        };

        EBW.with(|eb| {
            let mut eb = eb.borrow_mut();

            eb.reset(span_name, level.into(), keyword, event_tag);
            eb.opcode(Opcode::Info);

            // Promoting values from PartC to PartA extensions is apparently just a draft spec
            // and not necessary / supported by consumers.
            // let exts = json::extract_common_schema_parta_exts(attributes);

            eb.add_u16("__csver__", 0x0401, OutType::Signed, 0);
            eb.add_struct("PartA", 2 /* + exts.len() as u8*/, 0);
            {
                let time: String =
                    chrono::DateTime::to_rfc3339(&chrono::DateTime::<chrono::Utc>::from(start_stop_times.1));
                eb.add_str8("time", time, OutType::Utf8, 0);

                eb.add_struct("ext_dt", 2, 0);
                {
                    eb.add_str8("traceId", "", OutType::Utf8, 0); // TODO
                    eb.add_str8("spanId", &span_id, OutType::Utf8, 0);
                }
            }

            // if !span_data.links.is_empty() {
            //     self.add_struct("PartB", 5, 0);
            //     {
            //         self.add_str8("_typeName", "SpanLink", OutType::Utf8, 0);
            //         self.add_str8("fromTraceId", &traceId, OutType::Utf8, 0);
            //         self.add_str8("fromSpanId", &spanId, OutType::Utf8, 0);
            //         self.add_str8("toTraceId", "SpanLink", OutType::Utf8, 0);
            //         self.add_str8("toSpanId", "SpanLink", OutType::Utf8, 0);
            //     }
            // }

            let span_parent = span.parent();
            let partb_field_count = 3 + if span_parent.is_some() { 1 } else { 0 };

            eb.add_struct("PartB", partb_field_count, 0);
            {
                eb.add_str8("_typeName", "Span", OutType::Utf8, 0);

                if let Some(parent) = span_parent {
                    let parent_span_id = unsafe {
                        let mut span_id = MaybeUninit::<[u8; 16]>::uninit();
                        let mut cur = Cursor::new((&mut *span_id.as_mut_ptr()).as_mut_slice());
                        write!(&mut cur, "{:16x}", parent.id().into_u64()).expect("!write");
                        span_id.assume_init()
                    };

                    eb.add_str8("parentId", &parent_span_id, OutType::Utf8, 0);
                }

                eb.add_str8("name", span_name, OutType::Utf8, 0);

                eb.add_str8(
                    "startTime",
                    &chrono::DateTime::to_rfc3339(&chrono::DateTime::<chrono::Utc>::from(
                        start_stop_times.0,
                    )),
                    OutType::Utf8,
                    0,
                );
            }

            let partc_field_count = span.fields().len() as u8;

            eb.add_struct("PartC", partc_field_count, 0);
            {
                let mut pfv = CommonSchemaPartCBuilder { eb: eb.deref_mut() };

                for f in fields {
                    <CommonSchemaPartCBuilder<'_> as AddFieldAndValue<
                        CommonSchemaPartCBuilder<'_>,
                    >>::add_field_value(
                        &mut pfv,
                        &FieldAndValue {
                            field_name: f.field,
                            value: &f.value,
                        },
                    );
                }
            }

            let _ = eb.write(&self.get_provider(), None, None);
        });
    }

    fn write_record(
        self: Pin<&Self>,
        timestamp: SystemTime,
        current_span: u64,
        _parent_span: u64,
        event_name: &str,
        level: u8,
        keyword: u64,
        event: &tracing::Event<'_>,
    ) {
        EBW.with(|eb| {
            let mut eb = eb.borrow_mut();

            eb.reset(event_name, level.into(), keyword, 0);
            eb.opcode(Opcode::Info);

            // Promoting values from PartC to PartA extensions is apparently just a draft spec
            // and not necessary / supported by consumers.
            // let exts = json::extract_common_schema_parta_exts(attributes);

            eb.add_u16("__csver__", 0x0401, OutType::Signed, 0);
            eb.add_struct("PartA", 2 /* + exts.len() as u8*/, 0);
            {
                let time: String =
                    chrono::DateTime::to_rfc3339(&chrono::DateTime::<chrono::Utc>::from(timestamp));
                eb.add_str8("time", time, OutType::Utf8, 0);

                if current_span != 0 {
                    eb.add_struct("ext_dt", 2, 0);
                    {
                        let span_id = unsafe {
                            let mut span_id = MaybeUninit::<[u8; 16]>::uninit();
                            let mut cur = Cursor::new((&mut *span_id.as_mut_ptr()).as_mut_slice());
                            write!(&mut cur, "{:16x}", current_span).expect("!write");
                            span_id.assume_init()
                        };

                        eb.add_str8("traceId", "", OutType::Utf8, 0); // TODO
                        eb.add_str8("spanId", &span_id, OutType::Utf8, 0);
                    }
                }
            }

            eb.add_struct("PartB", 3, 0);
            {
                eb.add_str8("_typeName", "Log", OutType::Utf8, 0);
                eb.add_str8("name", event_name, OutType::Utf8, 0);

                eb.add_str8(
                    "eventTime",
                    &chrono::DateTime::to_rfc3339(&chrono::DateTime::<chrono::Utc>::from(
                        timestamp,
                    )),
                    OutType::Utf8,
                    0,
                );
            }

            let partc_field_count = event.fields().count() as u8;

            eb.add_struct("PartC", partc_field_count, 0);
            {
                let mut visitor = CommonSchemaPartCBuilder::make_visitor(eb.deref_mut());
                event.record(&mut visitor);
            }

            let _ = eb.write(&self.get_provider(), None, None);
        });
    }
}
