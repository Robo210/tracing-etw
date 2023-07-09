use tracing::{event, span, Level};
use tracing_etw::{EtwLayerBuilder};
use tracing_subscriber::{self, fmt::format::FmtSpan, prelude::*};

fn main() {
    // let subscriber = tracing_subscriber::fmt::fmt()
    //     .with_span_events(FmtSpan::FULL)
    //     .without_time();

    let subscriber = tracing_subscriber::registry()
        .with(EtwLayerBuilder::new("test").build_with_layer_filter())
        .with(EtwLayerBuilder::new("test2").with_common_schema_events().build_with_layer_filter())
        .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::ACTIVE));
    let _sub = subscriber.try_init();

    #[allow(non_snake_case)]
    let fieldB = "asdf";
    event!(
        Level::INFO,
        fieldC = b'x',
        fieldB,
        fieldA = 7,
        "inside {}!",
        "main"
    );

    // Construct a new span named "my span" with trace log level.
    let span = span!(Level::INFO, "my_span", field3 = 4.5, field2 = 0, field1 = 0);

    // Enter the span, returning a guard object.
    let _enter = span.enter();
    span.record("field1", 12345);

    span.record("field2", "9876f64");

    span!(Level::INFO, "nested_span", key = "value").in_scope(|| {
        event!(Level::ERROR, "oh noes!");
    });

    drop(_enter);
    drop(span);
}
