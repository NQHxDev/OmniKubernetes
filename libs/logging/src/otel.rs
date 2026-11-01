//! OpenTelemetry OTLP tracing pipeline configuration for Grafana Tempo / OpenTelemetry Collector.

use opentelemetry::{global, trace::TracerProvider as _};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    propagation::TraceContextPropagator,
    trace::{self as sdktrace, SdkTracerProvider, Tracer},
};
use tracing_opentelemetry::OpenTelemetryLayer;

use crate::config::LoggingConfig;

/// Initializes the global W3C `TraceContext` propagator.
pub fn init_propagator() {
    global::set_text_map_propagator(TraceContextPropagator::new());
}

/// Initializes the OpenTelemetry OTLP gRPC tracing exporter and returns a batch tracer provider.
pub fn init_tracer_provider(
    config: &LoggingConfig,
    endpoint: &str,
) -> Result<(Tracer, SdkTracerProvider), Box<dyn std::error::Error + Send + Sync>> {
    init_propagator();

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;

    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attribute(opentelemetry::KeyValue::new(
            "service.environment",
            config.environment.clone(),
        ))
        .build();

    let provider = sdktrace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer(config.service_name.clone());
    global::set_tracer_provider(provider.clone());

    Ok((tracer, provider))
}

/// Constructs an OpenTelemetry tracing layer for any tracing subscriber registry.
pub fn create_otel_layer<S>(tracer: Tracer) -> OpenTelemetryLayer<S, Tracer>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    tracing_opentelemetry::layer().with_tracer(tracer)
}
