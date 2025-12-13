//! OpenTelemetry initialization for holler.
//!
//! Connects to OTLP endpoint (default localhost:4317) for traces, logs, metrics.

use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler};
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize OpenTelemetry with OTLP exporters.
///
/// Default endpoint is localhost:4317 (standard OTLP gRPC port).
pub fn init(otlp_endpoint: &str) -> Result<()> {
    let resource = Resource::builder_empty()
        .with_service_name("holler")
        .with_attributes(vec![
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("deployment.environment", "development"),
        ])
        .build();

    let endpoint = if otlp_endpoint.starts_with("http") {
        otlp_endpoint.to_string()
    } else {
        format!("http://{}", otlp_endpoint)
    };

    // Configure OTLP trace exporter
    let trace_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .build()
        .context("Failed to create OTLP span exporter")?;

    let batch_span_processor =
        opentelemetry_sdk::trace::BatchSpanProcessor::builder(trace_exporter).build();

    let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_span_processor(batch_span_processor)
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource.clone())
        .build();

    let tracer = tracer_provider.tracer("holler");
    global::set_tracer_provider(tracer_provider);

    // Configure OTLP log exporter
    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .build()
        .context("Failed to create OTLP log exporter")?;

    let log_processor =
        opentelemetry_sdk::logs::BatchLogProcessor::builder(log_exporter).build();

    let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
        .with_log_processor(log_processor)
        .with_resource(resource.clone())
        .build();

    // Configure OTLP metrics exporter
    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .context("Failed to create OTLP metric exporter")?;

    let metric_reader =
        opentelemetry_sdk::metrics::PeriodicReader::builder(metric_exporter).build();

    let meter_provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
        .with_reader(metric_reader)
        .with_resource(resource)
        .build();

    global::set_meter_provider(meter_provider);

    // Create tracing layers
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let log_appender =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,holler=debug"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(telemetry_layer)
        .with(log_appender)
        .init();

    tracing::info!(
        "🔭 OpenTelemetry initialized with OTLP endpoint: {}",
        otlp_endpoint
    );

    Ok(())
}
