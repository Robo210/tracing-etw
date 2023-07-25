mod layer;
mod native;
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

#[macro_export]
macro_rules! etw_event {
    (target: $target:expr, name: $name:expr, $lvl:expr, $kw:expr, { $($fields:tt)* } )=> ({
        use tracing::Callsite;
        use const_format::concatcp;
        const ETW_LEVEL: u8 = $crate::map_level(&$lvl);
        static CALLSITE: tracing::callsite::DefaultCallsite =
            tracing::callsite::DefaultCallsite::new(
            {
                const EVENT_NAME: &'static str = concatcp!("event etw:", $name, ":", ($kw as u64));
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
            { message = format_args!($($arg)+), $($fields)* }
        )
    );
    (target: $target:expr, name: $name:expr, $lvl:expr, $kw:expr, $($k:ident).+ = $($fields:tt)* ) => (
        $crate::etw_event!(target: $target, name: $name, $lvl, $kw, { $($k).+ = $($fields)* })
    );
    (target: $target:expr, name: $name:expr, $lvl:expr, $kw:expr, $($arg:tt)+ ) => (
        $crate::etw_event!(target: $target, name: $name, $lvl, $kw, { $($arg)+ })
    );
    (name: $name:expr, $lvl:expr, $kw:expr, { $($fields:tt)* }, $($arg:tt)+ ) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            { message = format_args!($($arg)+), $($fields)* }
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, { $($fields:tt)* }, $($arg:tt)+ ) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            { message = format_args!($($arg)+), $($fields)* }
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, $($k:ident).+ = $($field:tt)*) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            { $($k).+ = $($field)*}
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, $($k:ident).+, $($field:tt)*) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            { $($k).+, $($field)*}
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, ?$($k:ident).+, $($field:tt)*) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            { ?$($k).+, $($field)*}
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, %$($k:ident).+, $($field:tt)*) => (
        $crate::etw_event!(
            target: module_path!(),
            name: $name,
            $lvl,
            $kw,
            { %$($k).+, $($field)*}
        )
    );
    (name: $name:expr, $lvl:expr, $kw:expr, ?$($k:ident).+) => (
        $crate::etw_event!(name: $name, $lvl, $kw, ?$($k).+,)
    );
    (name: $name:expr, $lvl:expr, $kw:expr, %$($k:ident).+) => (
        $crate::etw_event!(name: $name, $lvl, $kw, %$($k).+,)
    );
    (name: $name:expr, $lvl:expr, $kw:expr, $($k:ident).+) => (
        $crate::etw_event!(name: $name, $lvl, $kw, $($k).+,)
    );
    (name: $name:expr, $lvl:expr, $kw:expr, $($arg:tt)+ ) => (
        $crate::etw_event!(target: module_path!(), name: $name, $lvl, $kw, { $($arg)+ })
    );
}
