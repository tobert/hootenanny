# GraphQL/Trustfall Observability Needs

## Current Architecture Summary

### Two Query Domains

1. **Audio Graph** (`audio-graph-mcp`) - Identities, Artifacts, PipeWire devices
2. **Garden State** (`GardenStateAdapter`) - Regions, Nodes, Transport, Jobs

### Data Flow

```
MCP tool call (garden_query / graph_query)
    ↓
hootenanny typed_dispatcher
    ↓
GardenStateCache (TTL: 30s, event-invalidated)
    ↓
Trustfall adapter evaluation
    ↓
JSON result
```

---

## Observability Gaps

### 1. Query Performance Visibility

**Problem:** No metrics on query execution time, result sizes, or hot paths.

**Need:**
- Query execution duration histogram
- Result row counts
- Starting vertex resolution time vs. property/neighbor traversal time
- Cache hit/miss rates (currently only logged, not metriced)

**Proposed metrics:**
```
trustfall_query_duration_seconds{schema="garden"|"audio_graph", entry_point="Region"|"Artifact"|...}
trustfall_query_rows_returned{schema, entry_point}
trustfall_cache_operations{operation="hit"|"miss"|"invalidate"|"refresh"}
```

### 2. Snapshot Lifecycle Tracking

**Problem:** Can't see when snapshots become stale, how often they're refreshed, or transfer costs.

**Need:**
- Snapshot version progression
- Time between invalidation and refresh
- Snapshot size in bytes (Cap'n Proto payload)
- Network round-trip time for GetSnapshot RPC

**Proposed metrics:**
```
garden_snapshot_version (gauge, monotonic)
garden_snapshot_age_seconds (gauge)
garden_snapshot_fetch_duration_seconds (histogram)
garden_snapshot_bytes (histogram)
garden_snapshot_regions (gauge)
garden_snapshot_nodes (gauge)
```

### 3. IOPub Event Flow

**Problem:** No visibility into event rates, types, or processing lag.

**Need:**
- Event counts by type
- Time from chaosgarden state change to hootenanny cache invalidation
- Event queue depth (if buffered)
- Correlation: which events trigger cache refreshes

**Proposed metrics:**
```
iopub_events_total{event_type="RegionCreated"|"LatentResolved"|...}
iopub_event_latency_seconds (histogram)
iopub_cache_invalidations_total{event_type}
```

### 4. Adapter Resolution Tracing

**Problem:** Complex queries traverse multiple vertex types - hard to debug slow queries.

**Need:**
- Span per `resolve_starting_vertices` call
- Span per `resolve_property` batch
- Span per `resolve_neighbors` edge traversal
- Attribute: query string (truncated), entry point, filter params

**Proposed spans:**
```
trustfall.resolve_starting_vertices
  attributes: schema, vertex_type, filter_params
trustfall.resolve_property
  attributes: schema, vertex_type, property_name, batch_size
trustfall.resolve_neighbors
  attributes: schema, edge_name, direction, batch_size
```

### 5. Cache Coherence Debugging

**Problem:** Hard to diagnose "stale data" issues - did cache not invalidate? Did event get lost?

**Need:**
- Log/trace when cache is marked stale (with reason)
- Log/trace when cache refresh is triggered (with version delta)
- Correlation ID between IOPub event and resulting cache operation
- Warning when query runs against stale cache (age > threshold)

---

## Implementation Strategy

### Phase 1: Metrics Foundation

Add OpenTelemetry metrics to:
- `GardenStateCache` - cache operations, snapshot stats
- `typed_dispatcher` - query timing at entry point
- `CapnpGardenServer` (chaosgarden) - snapshot build time

### Phase 2: Distributed Tracing

Add spans to:
- Query execution path (hootenanny side)
- Snapshot fetch RPC
- IOPub event processing

### Phase 3: Event Correlation

Add trace context propagation:
- IOPub events carry trace_id from originating operation
- Cache invalidation spans link to originating event
- Query execution spans link to cache refresh if triggered

---

## Questions to Answer with Observability

1. **Performance:** "Why is this query slow?" → See which resolution phase dominates
2. **Freshness:** "Is the data I'm seeing current?" → Check snapshot age, version, last invalidation
3. **Cost:** "How expensive is this query pattern?" → See rows returned, traversal depth
4. **Reliability:** "Why did I get stale data?" → Trace event flow, cache state at query time
5. **Capacity:** "Can we handle more queries?" → See cache hit rate, RPC latency, CPU in adapters

---

## Integration Points

### OTLP Endpoint

Use existing `otlp-mcp` server for collection:
```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:39677
```

### Rust Instrumentation

```rust
use tracing::{instrument, info_span};
use opentelemetry::metrics::{Counter, Histogram};

#[instrument(skip(self, snapshot), fields(schema = "garden", entry_point = %vertex_type))]
fn resolve_starting_vertices(&self, ...) { ... }
```

### Metric Export

```rust
// In hootenanny startup
let meter = global::meter("hootenanny");
let query_duration = meter.f64_histogram("trustfall_query_duration_seconds").init();
let cache_hits = meter.u64_counter("trustfall_cache_hits_total").init();
```

---

## Success Criteria

After implementation, we should be able to:

1. Dashboard showing query latency percentiles by schema/entry_point
2. Alert on cache hit rate dropping below 80%
3. Trace from "user clicked play" → IOPub event → cache invalidate → query returns fresh data
4. Identify the 3 slowest query patterns in production use
5. Measure snapshot transfer overhead vs. query evaluation time
