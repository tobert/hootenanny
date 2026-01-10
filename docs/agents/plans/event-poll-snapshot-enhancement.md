# Event Poll Enhancement: Snapshot + Time-Window Query

## Context

The `event_poll` tool was implemented in commit `37e7b9f`. It provides cursor-based pagination over broadcast events. This enhancement adds:

1. **Snapshot data** - Current system state included in every response
2. **Time-window query** (`since_ms`) - Alternative to cursor-based polling

## Current Implementation

File: `crates/hootenanny/src/event_buffer.rs`

```rust
// Current response structure
struct PollResult {
    events: Vec<BufferedEvent>,
    cursor: u64,
    has_more: bool,
    latest_beat: Option<BeatTickInfo>,  // Already tracks latest beat tick
    buffer: BufferStats,
    server_time_ms: u64,
}
```

## Proposed Changes

### 1. Add Snapshot Tracking to EventBuffer

Track latest state from broadcasts (similar to `latest_beat`):

```rust
// In EventBuffer struct, add:
latest_transport: Option<TransportInfo>,
device_count: u32,

// New types:
struct TransportInfo {
    state: String,        // "playing", "paused", "stopped"
    position_beats: f64,
    tempo_bpm: f64,
    timestamp_ms: u64,
}

struct Snapshot {
    transport: Option<TransportInfo>,
    latest_beat: Option<BeatTickInfo>,
    active_jobs: JobSummary,      // Requires JobStore access
    device_count: u32,
}

struct JobSummary {
    pending: usize,
    running: usize,
}
```

Update `push()` to capture TransportStateChanged and DeviceConnected/Disconnected.

### 2. Add `since_ms` Parameter

Alternative to cursor - get events from the last N milliseconds:

```rust
// In poll(), add since_ms handling:
pub fn poll(
    &self,
    cursor: Option<u64>,
    since_ms: Option<u64>,      // NEW
    types: Option<&[String]>,
    limit: usize,
) -> Result<PollResult, PollError> {
    // If since_ms provided, filter by timestamp instead of cursor
    let events = if let Some(since) = since_ms {
        let cutoff = current_time_ms().saturating_sub(since);
        self.get_events_since(cutoff, types, limit)
    } else if let Some(cursor_seq) = cursor {
        self.get_events_after(cursor_seq, types, limit)
    } else {
        self.get_tail_events(types, limit)
    };
    // ...
}

fn get_events_since(&self, cutoff_ms: u64, types: Option<&[String]>, limit: usize) -> Vec<BufferedEvent> {
    self.events
        .iter()
        .filter(|e| e.timestamp_ms >= cutoff_ms)
        .filter(|e| /* type filter */)
        .take(limit)
        .cloned()
        .collect()
}
```

### 3. Include Snapshot in Response

Modify `event_poll_typed()` in `service_typed.rs` to include snapshot:

```rust
// Query job store for active jobs
let job_summary = self.job_store.summary();  // Need to add this method

// Build snapshot from buffer + job store
let snapshot = Snapshot {
    transport: buffer.latest_transport().cloned(),
    latest_beat: buffer.latest_beat().cloned(),
    active_jobs: job_summary,
    device_count: buffer.device_count(),
};
```

### 4. Update Protocol Types

In `crates/hooteproto/src/request.rs`:
```rust
pub struct EventPollRequest {
    pub cursor: Option<u64>,
    pub since_ms: Option<u64>,     // NEW
    pub types: Option<Vec<String>>,
    pub timeout_ms: Option<u64>,
    pub limit: Option<usize>,
}
```

In `crates/hooteproto/src/responses.rs`:
```rust
pub struct EventPollResponse {
    pub events: Vec<BufferedEvent>,
    pub cursor: u64,
    pub has_more: bool,
    pub snapshot: Snapshot,         // CHANGED from latest_beat
    pub buffer: BufferStats,
    pub server_time_ms: u64,
}

pub struct Snapshot {
    pub transport: Option<TransportInfo>,
    pub latest_beat: Option<BeatTickInfo>,
    pub active_jobs: JobSummary,
    pub device_count: u32,
}

pub struct TransportInfo {
    pub state: String,
    pub position_beats: f64,
    pub tempo_bpm: f64,
    pub timestamp_ms: u64,
}

pub struct JobSummary {
    pub pending: usize,
    pub running: usize,
}
```

### 5. Update MCP Schema

In `crates/holler/src/manual_schemas.rs`, add `since_ms` to schema:
```rust
"since_ms": {
    "type": ["integer", "null"],
    "minimum": 0,
    "description": "Get events from the last N milliseconds (alternative to cursor)"
}
```

## Files to Modify

1. `crates/hootenanny/src/event_buffer.rs` - Add TransportInfo tracking, since_ms query
2. `crates/hootenanny/src/job_system.rs` - Add `summary()` method to JobStore
3. `crates/hootenanny/src/api/service_typed.rs` - Build snapshot in response
4. `crates/hooteproto/src/request.rs` - Add since_ms field
5. `crates/hooteproto/src/responses.rs` - Add Snapshot, TransportInfo, JobSummary types
6. `crates/holler/src/dispatch.rs` - Pass since_ms to request
7. `crates/holler/src/manual_schemas.rs` - Add since_ms to schema

## API Examples

### Initial poll with snapshot
```json
// Request
{ "limit": 50 }

// Response
{
  "events": [...],
  "cursor": 1050,
  "has_more": true,
  "snapshot": {
    "transport": { "state": "playing", "position_beats": 105892.7, "tempo_bpm": 120.0 },
    "latest_beat": { "beat": 105892, "position_beats": 105892.5, "tempo_bpm": 120.0 },
    "active_jobs": { "pending": 0, "running": 2 },
    "device_count": 3
  },
  "buffer": { ... },
  "server_time_ms": 1704307200000
}
```

### Time-window query (last 10ms)
```json
// Request - useful for real-time UIs
{ "since_ms": 10, "limit": 100 }

// Response - events from last 10ms only
{
  "events": [...],
  "cursor": 1055,
  "snapshot": { ... },
  ...
}
```

## Testing

Add tests for:
1. `latest_transport` updates from TransportStateChanged broadcasts
2. `device_count` updates from DeviceConnected/Disconnected broadcasts
3. `since_ms` filtering returns correct time window
4. Snapshot includes current job summary from JobStore
