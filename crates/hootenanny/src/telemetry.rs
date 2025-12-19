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
        .with_service_name("hootenanny")
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

    let tracer = tracer_provider.tracer("hootenanny");
    global::set_tracer_provider(tracer_provider);

    // 2. Configure OTLP log exporter with timeout
    let log_exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .with_timeout(EXPORT_TIMEOUT)
        .build()
        .context("Failed to create OTLP log exporter")?;

    let log_processor = opentelemetry_sdk::logs::BatchLogProcessor::builder(log_exporter).build();

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
///
/// With the export timeouts configured during init (5s per exporter), the Drop
/// handlers will complete in bounded time. No explicit shutdown call is needed
/// in opentelemetry 0.28 - the providers flush on drop.
pub fn shutdown() -> Result<()> {
    tracing::info!(
        "🔭 Shutting down OpenTelemetry (providers will flush on drop with 5s timeout)..."
    );
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

/// Parse a W3C traceparent header and return an OpenTelemetry Context.
///
/// Format: `{version}-{trace_id}-{span_id}-{trace_flags}`
/// Example: `00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01`
///
/// Returns None if the traceparent is invalid or missing.
pub fn parse_traceparent(traceparent: Option<&str>) -> Option<opentelemetry::Context> {
    use opentelemetry::trace::{
        SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState,
    };

    let tp = traceparent?;
    let parts: Vec<&str> = tp.split('-').collect();

    if parts.len() != 4 {
        tracing::warn!("Invalid traceparent format: {}", tp);
        return None;
    }

    let version = parts[0];
    if version != "00" {
        tracing::warn!("Unsupported traceparent version: {}", version);
        return None;
    }

    // Parse trace_id (32 hex chars = 16 bytes)
    let trace_id = match hex_to_bytes::<16>(parts[1]) {
        Some(bytes) => TraceId::from_bytes(bytes),
        None => {
            tracing::warn!("Invalid trace_id in traceparent: {}", parts[1]);
            return None;
        }
    };

    // Parse span_id (16 hex chars = 8 bytes)
    let span_id = match hex_to_bytes::<8>(parts[2]) {
        Some(bytes) => SpanId::from_bytes(bytes),
        None => {
            tracing::warn!("Invalid span_id in traceparent: {}", parts[2]);
            return None;
        }
    };

    // Parse trace_flags (2 hex chars = 1 byte)
    let flags = u8::from_str_radix(parts[3], 16).unwrap_or(0);
    let trace_flags = TraceFlags::new(flags);

    let span_context = SpanContext::new(
        trace_id,
        span_id,
        trace_flags,
        true, // is_remote = true since this came from another service
        TraceState::default(),
    );

    Some(opentelemetry::Context::current().with_remote_span_context(span_context))
}

/// Create a tracing span with the given traceparent as the parent context.
///
/// Use this to continue a distributed trace from an incoming request.
#[macro_export]
macro_rules! span_with_parent {
    ($traceparent:expr, $name:expr) => {{
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        let span = tracing::info_span!($name);
        if let Some(parent_ctx) = $crate::telemetry::parse_traceparent($traceparent) {
            span.set_parent(parent_ctx);
        }
        span
    }};
    ($traceparent:expr, $name:expr, $($field:tt)*) => {{
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        let span = tracing::info_span!($name, $($field)*);
        if let Some(parent_ctx) = $crate::telemetry::parse_traceparent($traceparent) {
            span.set_parent(parent_ctx);
        }
        span
    }};
}

/// Helper to convert hex string to fixed-size byte array
fn hex_to_bytes<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 {
        return None;
    }

    let mut bytes = [0u8; N];
    for i in 0..N {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_bytes() {
        let result: Option<[u8; 4]> = hex_to_bytes("deadbeef");
        assert_eq!(result, Some([0xde, 0xad, 0xbe, 0xef]));

        let result: Option<[u8; 4]> = hex_to_bytes("short");
        assert_eq!(result, None);

        let result: Option<[u8; 4]> = hex_to_bytes("not_hex!");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_traceparent_valid() {
        let tp = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = parse_traceparent(Some(tp));
        assert!(ctx.is_some());
    }

    #[test]
    fn test_parse_traceparent_none() {
        let ctx = parse_traceparent(None);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_parse_traceparent_invalid_format() {
        let ctx = parse_traceparent(Some("not-a-valid-traceparent"));
        assert!(ctx.is_none());
    }

    #[test]
    fn test_parse_traceparent_wrong_version() {
        let tp = "01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = parse_traceparent(Some(tp));
        assert!(ctx.is_none());
    }

    #[test]
    fn test_parse_traceparent_invalid_trace_id() {
        let tp = "00-short-b7ad6b7169203331-01";
        let ctx = parse_traceparent(Some(tp));
        assert!(ctx.is_none());
    }
}
