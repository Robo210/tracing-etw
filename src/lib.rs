mod layer;
pub mod native;
mod values;

pub use layer::*;

#[inline]
#[doc(hidden)]
pub const fn map_level(level: &tracing::Level) -> u8 {
    match *level {
        tracing::Level::ERROR => tracelogging::Level::Error.as_int(),
        tracing::Level::WARN => tracelogging::Level::Warning.as_int(),
        tracing::Level::INFO => tracelogging::Level::Informational.as_int(),
        tracing::Level::DEBUG => tracelogging::Level::Verbose.as_int(),
        tracing::Level::TRACE => tracelogging::Level::Verbose.as_int() + 1,
    }
}

#[doc(hidden)]
pub struct EtwEventMetadata {
    pub kw: u64,
    pub identity: tracing::callsite::Identifier,
    pub event_tag: u32,
}

#[macro_export]
macro_rules! etw_event {
    (target: $target:expr, name: $name:expr, $lvl:expr, $kw:expr, $tags:expr, { $($fields:tt)* } )=> ({
        use tracing::Callsite;
        use const_format::concatcp;
        use paste::paste;

        static CALLSITE: tracing::callsite::DefaultCallsite =
            tracing::callsite::DefaultCallsite::new(
            {
                const EVENT_NAME: &'static str = $name;
                static META: tracing::metadata::Metadata =
                    tracing::metadata::Metadata::new(
                        EVENT_NAME,
                        $target,
                        $lvl,
                        Some(file!()),
                        Some(line!()),
                        Some(module_path!()),
                        tracing::field::FieldSet::new(tracing::fieldset!( $($fields)* ), tracing_core::identify_callsite!(&CALLSITE)),
                        tracing::metadata::Kind::EVENT,
                );
                &META
            }
        );

        static ETW_META: $crate::EtwEventMetadata = $crate::EtwEventMetadata{
            kw: $kw,
            identity: tracing_core::identify_callsite!(&CALLSITE),
            event_tag: $tags as u32
        };

        #[cfg(target_os = "linux")]
        #[link_section = "_etw_kw"]
        #[allow(non_upper_case_globals)]
        paste! {
            static mut [<ETW_META_PTR $name>]: *const $crate::EtwEventMetadata = &ETW_META;
        }

        #[cfg(target_os = "windows")]
        #[link_section = ".rsdata$zRSETW5"]
        #[allow(non_upper_case_globals)]
        paste! {
            static mut [<ETW_META_PTR $name>]: *const $crate::EtwEventMetadata = &ETW_META;
        }

        let enabled = tracing::level_enabled!($lvl) && {
            let interest = CALLSITE.interest();
            !interest.is_never() && tracing::__macro_support::__is_enabled(CALLSITE.metadata(), interest)
        };
        if enabled {
            (|value_set: tracing::field::ValueSet| {
                let meta = CALLSITE.metadata();
                // event with contextual parent
                tracing::Event::dispatch(
                    meta,
                    &value_set
                );
                tracing::__tracing_log!(
                    $lvl,
                    CALLSITE,
                    &value_set
                );
            })(tracing::valueset!(CALLSITE.metadata().fields(), $($fields)*));
        } else {
            tracing::__tracing_log!(
                $lvl,
                CALLSITE,
                &tracing::valueset!(CALLSITE.metadata().fields(), $($fields)*)
            );
        }
    });
    (target: $target:expr, name: $name:expr, $lvl:expr, $kw:expr, { $($fields:tt)* }, $($arg:tt)+ ) => (
        $crate::etw_event!(
            target: $target,
            name: $name,
            $lvl,
            $kw,
            0,
            { message = format_args!($($arg)+), $($fields)* }
        )
    );
    (target: $target:expr, name: $name:expr, $lvl:expr, $kw:expr, $($k:ident).+ = $($fields:tt)* ) => (
        $crate::etw_event!(target: $target, name: $name, $lvl, $kw, 0, { $($k).+ = $($fields)* })
    );
    (target: $target:expr, name: $name:expr, $lvl:expr, $kw:expr, $($arg:tt)+ ) => (
        $crate::etw_event!(target: $target, name: $name, $lvl, $kw, 0, { $($arg)+ })
    );
    (name: $name:expr, $lvl:expr, $kw:expr, { $($fields:tt)* }, $($arg:tt)+ ) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            0,
            { message = format_args!($($arg)+), $($fields)* }
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, { $($fields:tt)* }, $($arg:tt)+ ) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            0,
            { message = format_args!($($arg)+), $($fields)* }
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, $($k:ident).+ = $($field:tt)*) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            0,
            { $($k).+ = $($field)*}
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, $($k:ident).+, $($field:tt)*) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            0,
            { $($k).+, $($field)*}
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, ?$($k:ident).+, $($field:tt)*) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            0,
            { ?$($k).+, $($field)*}
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, %$($k:ident).+, $($field:tt)*) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            0,
            { %$($k).+, $($field)*}
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, ?$($k:ident).+) => (
        $crate::etw_event!(name: $name, $lvl, $kw, 0, ?$($k).+,)
    );
    (name: $name:expr, $lvl:expr, $kw:expr, %$($k:ident).+) => (
        $crate::etw_event!(name: $name, $lvl, $kw, 0, %$($k).+,)
    );
    (name: $name:expr, $lvl:expr, $kw:expr, $($k:ident).+) => (
        $crate::etw_event!(name: $name, $lvl, $kw, 0, $($k).+,)
    );
    (name: $name:expr, $lvl:expr, $kw:expr, $($arg:tt)+ ) => (
        $crate::etw_event!(target: module_path!(), name: $name, $lvl, $kw, 0, { $($arg)+ })
    );
}
