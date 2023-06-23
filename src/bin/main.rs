use tracing::{event, span, Level};
use tracing_etw::{EtwLayer, EtwFilter};
use tracing_subscriber::{self, fmt::format::FmtSpan, Layer, prelude::*};

fn main() {
    // let subscriber = tracing_subscriber::fmt::fmt()
    //     .with_span_events(FmtSpan::FULL)
    //     .without_time();
    let subscriber = tracing_subscriber::registry().with(EtwLayer::new("test")).with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::FULL));
    let _sub = subscriber.try_init();

    // Construct a new span named "my span" with trace log level.
    let span = span!(Level::INFO, "my_span");

    // Enter the span, returning a guard object.
    let _enter = span.enter();

    event!(Level::INFO, "inside my_function!");

    span!(Level::INFO, "nested_span", key = "value").in_scope(|| {
        event!(Level::ERROR, "oh noes!");
    });
}
