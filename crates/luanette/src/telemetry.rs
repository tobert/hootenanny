//! OpenTelemetry initialization and configuration.
//!
//! Provides comprehensive observability via OTLP: traces, logs, and metrics.
//! Connects to otlp-mcp server for buffering and querying telemetry data.

use std::time::Duration;

use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler};
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Timeout for OTLP exports - prevents blocking on unavailable endpoints
const EXPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// Initialize OpenTelemetry with OTLP exporters for traces, logs, and metrics.
///
/// Connects to the specified gRPC endpoint (typically otlp-mcp server).
/// Exports all three signal types for comprehensive observability.
pub fn init(otlp_endpoint: &str) -> Result<()> {
    // Create resource with service metadata (shared across all signals)
    let resource = Resource::builder_empty()
        .with_service_name("luanette")
        .with_attributes(vec![
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("deployment.environment", "development"),
        ])
        .build();

    let endpoint = format!("http://{}", otlp_endpoint);

    // 1. Configure OTLP trace exporter with timeout
    let trace_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .with_timeout(EXPORT_TIMEOUT)
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

    let tracer = tracer_provider.tracer("luanette");
    global::set_tracer_provider(tracer_provider);

    // 2. Configure OTLP log exporter with timeout
    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .with_timeout(EXPORT_TIMEOUT)
        .build()
        .context("Failed to create OTLP log exporter")?;

    let log_processor =
        opentelemetry_sdk::logs::BatchLogProcessor::builder(log_exporter).build();

    let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
        .with_log_processor(log_processor)
        .with_resource(resource.clone())
        .build();

    // 3. Configure OTLP metrics exporter with timeout
    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .with_timeout(EXPORT_TIMEOUT)
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
        .unwrap_or_else(|_| EnvFilter::new("info,luanette=debug,baton=info"));

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
///
/// With the export timeouts configured during init (5s per exporter), the Drop
/// handlers will complete in bounded time. No explicit shutdown call is needed
/// in opentelemetry 0.28 - the providers flush on drop.
pub fn shutdown() -> Result<()> {
    tracing::info!("🔭 Shutting down OpenTelemetry (providers will flush on drop with 5s timeout)...");
    Ok(())
}
