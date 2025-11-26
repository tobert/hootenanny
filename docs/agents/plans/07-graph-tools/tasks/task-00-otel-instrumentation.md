# Task 00: OpenTelemetry Instrumentation

**Status**: 🟡 Not started
**Estimated effort**: 1-2 hours
**Prerequisites**: None (do early)
**Depends on**: Nothing
**Enables**: Observability for all subsequent tasks

## 🎯 Goal

Add OpenTelemetry instrumentation to audio-graph-mcp from the start. Create spans for all queries and operations so we can trace the full request path from agent → MCP tool → Trustfall → live sources.

**Why first?** Debugging identity matching and query performance without observability is painful. Gemini's review flagged this: *"Troubleshooting match logic is a nightmare without good tracing."*

**Integration context**: Downstream services (Orpheus, Hootenanny) already implement OTEL. Adding spans here completes the observability picture.

## 📋 Context

The HalfRemembered ecosystem uses OpenTelemetry:
- `otlp-mcp` server captures traces/logs/metrics
- Hootenanny emits spans for tool calls
- Orpheus (Python) emits spans for music generation

Audio-graph-mcp should emit spans for:
- Query execution (full Trustfall query)
- Source enumeration (ALSA, PipeWire)
- Identity matching (with match scores)
- Database operations

### Span Hierarchy Example

```
graph_query (MCP tool)
├── trustfall.execute_query
│   ├── resolve_starting_vertices (AlsaMidiDevice)
│   │   └── alsa.enumerate_devices
│   ├── resolve_property (name)
│   ├── resolve_neighbors (identity)
│   │   ├── identity.extract_fingerprints
│   │   └── identity.match
│   │       └── sqlite.find_hints_by_value
│   └── resolve_neighbors (tags)
│       └── sqlite.get_tags
└── result.serialize
```

## 📦 Dependencies (add to Cargo.toml)

```toml
[dependencies]
# OpenTelemetry
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-opentelemetry = "0.22"
opentelemetry = "0.21"
opentelemetry_sdk = { version = "0.21", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.14", features = ["grpc-tonic"] }
```

## 🔨 Implementation

### Initialize OTEL (src/telemetry.rs)

```rust
use opentelemetry::global;
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry() -> anyhow::Result<()> {
    // Get OTLP endpoint from environment (default to local otlp-mcp server)
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:4317".to_string());

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(&endpoint)
        )
        .with_trace_config(
            opentelemetry_sdk::trace::config()
                .with_resource(opentelemetry_sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", "audio-graph-mcp"),
                ]))
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("audio_graph_mcp=debug".parse()?))
        .with(telemetry_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

pub fn shutdown_telemetry() {
    global::shutdown_tracer_provider();
}
```

### Instrument Query Execution (src/mcp_tools/query.rs)

```rust
use tracing::{instrument, info_span, Instrument};

#[instrument(
    name = "graph_query",
    skip(adapter, variables),
    fields(
        query_preview = %query.chars().take(100).collect::<String>(),
        otel.kind = "client"
    )
)]
pub async fn graph_query(
    adapter: Arc<AudioGraphAdapter>,
    query: String,
    variables: Option<Value>,
) -> Result<Vec<Value>, String> {
    let results = execute_query(adapter.schema(), adapter.clone(), &query, vars)
        .map_err(|e| {
            tracing::error!(error = %e, "Query execution failed");
            format!("Query execution failed: {}", e)
        })?
        .collect();

    tracing::info!(result_count = results.len(), "Query completed");
    Ok(results)
}
```

### Instrument ALSA Enumeration (src/sources/alsa.rs)

```rust
use tracing::{instrument, info};

impl AlsaSource {
    #[instrument(name = "alsa.enumerate_devices", skip(self))]
    pub fn enumerate_devices(&self) -> Result<Vec<AlsaMidiDevice>> {
        let span = tracing::Span::current();

        let seq = alsa::Seq::open(None, None, false)
            .context("Failed to open ALSA sequencer")?;

        let devices = /* ... enumeration logic ... */;

        span.record("device_count", devices.len());
        info!(device_count = devices.len(), "ALSA enumeration complete");

        Ok(devices)
    }
}
```

### Instrument Identity Matching (src/matcher.rs)

```rust
use tracing::{instrument, debug, warn};

impl IdentityMatcher<'_> {
    #[instrument(
        name = "identity.match",
        skip(self, fingerprints),
        fields(fingerprint_count = fingerprints.len())
    )]
    pub fn match_device(&self, fingerprints: &[DeviceFingerprint]) -> Result<Vec<IdentityMatch>> {
        let mut candidates = /* ... */;

        for (identity_id, matched_hints) in &candidates {
            let score = self.compute_score(matched_hints);
            debug!(
                identity = %identity_id,
                score = score,
                hint_count = matched_hints.len(),
                "Candidate scored"
            );
        }

        if candidates.is_empty() {
            warn!(
                fingerprints = ?fingerprints.iter().map(|f| f.value.as_str()).collect::<Vec<_>>(),
                "No identity match found"
            );
        }

        Ok(results)
    }
}
```

### Instrument Database Operations (src/db/*.rs)

```rust
#[instrument(name = "sqlite.find_hints", skip(self), fields(hint_kind = %kind.as_str()))]
pub fn find_hints_by_kind_value(&self, kind: HintKind, value: &str) -> Result<Vec<IdentityHint>> {
    // ...
}

#[instrument(name = "sqlite.get_tags", skip(self))]
pub fn get_tags(&self, identity_id: &str) -> Result<Vec<Tag>> {
    // ...
}
```

## 🧪 Testing Telemetry

```rust
#[tokio::test]
async fn test_query_emits_spans() {
    // Use otlp-mcp server's snapshot feature
    // 1. Create snapshot "before-query"
    // 2. Run graph_query
    // 3. Create snapshot "after-query"
    // 4. Query spans between snapshots
    // 5. Assert span hierarchy exists
}
```

Or manually verify:
```bash
# Start otlp-mcp server
# Set endpoint
export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4317

# Run audio-graph-mcp
cargo run --bin audio-graph-mcp

# In another terminal, use mcp__otlp-mcp__query to see spans
```

## ✅ Acceptance Criteria

1. ✅ `init_telemetry()` connects to OTLP endpoint
2. ✅ `graph_query` creates parent span
3. ✅ ALSA/PipeWire enumeration creates child spans
4. ✅ Identity matching logs scores and decisions
5. ✅ Spans visible in otlp-mcp query results
6. ✅ Service name "audio-graph-mcp" in span attributes

## 💡 Key Attributes to Record

| Span | Attributes |
|------|------------|
| `graph_query` | `query_preview`, `result_count` |
| `alsa.enumerate` | `device_count` |
| `pipewire.enumerate` | `node_count`, `media_class` |
| `identity.match` | `fingerprint_count`, `best_score`, `confidence` |
| `sqlite.*` | `identity_id`, `row_count` |

## 📚 References

- OpenTelemetry Rust: https://opentelemetry.io/docs/instrumentation/rust/
- tracing crate: https://docs.rs/tracing/
- tracing-opentelemetry: https://docs.rs/tracing-opentelemetry/
- otlp-mcp server (local): Already running in HalfRemembered ecosystem

## 🎬 Next Task

After telemetry is set up: **[Task 01: SQLite Foundation](task-01-sqlite-foundation.md)**

With observability in place, you'll see exactly what's happening in the database layer.
