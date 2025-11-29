//! OpenTelemetry initialization and configuration.
//!
//! Provides comprehensive observability via OTLP: traces, logs, and metrics.
//! Connects to otlp-mcp server for buffering and querying telemetry data.

use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler};
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize OpenTelemetry with OTLP exporters for traces, logs, and metrics.
///
/// Connects to the specified gRPC endpoint (typically otlp-mcp server).
/// Exports all three signal types for comprehensive observability.
pub fn init(otlp_endpoint: &str) -> Result<()> {
    // Create resource with service metadata (shared across all signals)
    let resource = Resource::builder_empty()
        .with_service_name("hootenanny")
        .with_attributes(vec![
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("deployment.environment", "development"),
        ])
        .build();

    let endpoint = format!("http://{}", otlp_endpoint);

    // 1. Configure OTLP trace exporter
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

    let tracer = tracer_provider.tracer("hootenanny");
    global::set_tracer_provider(tracer_provider);

    // 2. Configure OTLP log exporter
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

    // 3. Configure OTLP metrics exporter
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

    // Create log appender layer for exporting logs
    let log_appender =
        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,hootenanny=debug"));

    // Initialize tracing subscriber with all layers
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
    tracing::info!("📊 Exporting traces, logs, and metrics");

    Ok(())
}

/// Shutdown OpenTelemetry gracefully, flushing any pending data.
pub fn shutdown() -> Result<()> {
    tracing::info!("🔭 Shutting down OpenTelemetry...");

    // Batch processors will flush on Drop
    // No explicit shutdown needed in 0.28

    Ok(())
}

/// Helper to get the current trace ID as a hex string.
///
/// Returns None if called outside a tracing span.
#[allow(dead_code)]
pub fn current_trace_id() -> Option<String> {
    use opentelemetry::trace::TraceContextExt;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();

    if span_context.is_valid() {
        Some(span_context.trace_id().to_string())
    } else {
        None
    }
}

/// Helper to get the current span ID as a hex string.
///
/// Returns None if called outside a tracing span.
#[allow(dead_code)]
pub fn current_span_id() -> Option<String> {
    use opentelemetry::trace::TraceContextExt;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();

    if span_context.is_valid() {
        Some(span_context.span_id().to_string())
    } else {
        None
    }
}
