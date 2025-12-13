# 13: Wire the Daemon

**File:** `src/bin/chaosgarden.rs` → `src/daemon.rs`
**Focus:** Replace StubHandler with real state management
**Status:** Phase 1-3 Complete (Transport + Queries Working)

---

## The Problem

All core modules are implemented and tested (132+ unit tests passing):
- `primitives.rs` - Time, Signal, Node, Region
- `graph.rs` - DAG topology, petgraph
- `latent.rs` - Latent lifecycle, HITL approval
- `playback.rs` - CompiledGraph, PlaybackEngine
- `external_io.rs` - ExternalIOManager, I/O nodes
- `query.rs` - ChaosgardenAdapter (Trustfall)
- `patterns.rs` - Track, Bus, Section, Timeline
- `capabilities.rs` - CapabilityRegistry, Participant

But the daemon binary uses `StubHandler` that returns hardcoded responses:

```rust
// Current: src/bin/chaosgarden.rs
impl Handler for StubHandler {
    fn handle_shell(&self, req: ShellRequest) -> ShellReply {
        match req {
            ShellRequest::GetTransportState => ShellReply::TransportState {
                playing: false,
                position: Beat(0.0),
                tempo: 120.0,
            },
            ShellRequest::Play => ShellReply::Ok,
            // ... all stubs
        }
    }
    fn handle_query(&self, _req: QueryRequest) -> QueryReply {
        QueryReply::Error { error: "trustfall queries not yet implemented".to_string() }
    }
}
```

**Result:** MCP tools work but do nothing. Queries fail.

---

## The Goal

Replace stubs with real state:

```rust
// Target: src/daemon.rs
pub struct GardenDaemon {
    // Timeline state
    timeline: Timeline,
    regions: Vec<Region>,
    tempo_map: Arc<RwLock<TempoMap>>,

    // Processing graph
    graph: Graph,
    playback: Option<PlaybackEngine>,

    // External I/O
    io_manager: ExternalIOManager,

    // Latent management
    latent_manager: LatentManager,

    // Query adapter
    query_adapter: ChaosgardenAdapter,

    // Capabilities
    capability_registry: CapabilityRegistry,

    // Content resolution
    content_resolver: Arc<dyn ContentResolver>,
}

impl Handler for GardenDaemon {
    fn handle_shell(&self, req: ShellRequest) -> ShellReply {
        match req {
            ShellRequest::Play => {
                self.playback.as_mut().map(|p| p.play());
                ShellReply::Ok
            }
            ShellRequest::GetTransportState => {
                let state = self.playback.as_ref()
                    .map(|p| p.transport_state())
                    .unwrap_or_default();
                ShellReply::TransportState { ... }
            }
            // Real implementations
        }
    }

    fn handle_query(&self, req: QueryRequest) -> QueryReply {
        match self.query_adapter.execute(&req.query, &req.variables) {
            Ok(results) => QueryReply::Results { rows: results },
            Err(e) => QueryReply::Error { error: e.to_string() },
        }
    }
}
```

---

## Implementation Plan

### Phase 1: Daemon State Structure

Create `src/daemon.rs` with `GardenDaemon` struct that holds all state.

```rust
pub struct GardenDaemon {
    config: DaemonConfig,

    // Core state
    tempo_map: Arc<RwLock<TempoMap>>,
    regions: Arc<RwLock<Vec<Region>>>,
    graph: Arc<RwLock<Graph>>,

    // Subsystems
    playback: Arc<RwLock<Option<PlaybackEngine>>>,
    latent_manager: Arc<LatentManager>,
    io_manager: Arc<RwLock<ExternalIOManager>>,
    capability_registry: Arc<RwLock<CapabilityRegistry>>,

    // Query
    query_adapter: Arc<ChaosgardenAdapter>,

    // Content
    content_resolver: Arc<dyn ContentResolver>,

    // IOPub for events
    iopub_tx: tokio::sync::broadcast::Sender<LatentEvent>,
}
```

**Files:**
- Create `src/daemon.rs`
- Move `StubHandler` logic to real implementations

### Phase 2: Shell Commands

Wire up real shell command handlers:

| Command | Current | Target |
|---------|---------|--------|
| `GetTransportState` | Hardcoded `false, 0.0, 120.0` | Read from PlaybackEngine |
| `Play` | Returns `Ok` | Call `playback.play()` |
| `Pause` | Returns `Ok` | Call `playback.pause()` |
| `Stop` | Returns `Ok` | Call `playback.stop()` |
| `Seek { beat }` | Returns `Ok` | Call `playback.seek(beat)` |
| `SetTempo { bpm }` | Returns `Error` | Update tempo_map |
| `AddRegion { ... }` | Returns `Error` | Add to regions vec |
| `UpdateLatentProgress` | Returns `Error` | Forward to latent_manager |
| `ResolveLatent` | Returns `Error` | Forward to latent_manager |
| `ApproveLatent` | Returns `Error` | Forward to latent_manager |
| `RejectLatent` | Returns `Error` | Forward to latent_manager |

### Phase 3: Query Handler

Wire up the Trustfall adapter:

```rust
fn handle_query(&self, req: QueryRequest) -> QueryReply {
    let adapter = ChaosgardenAdapter::new(
        self.regions.read().unwrap().clone(),
        self.graph.read().unwrap().clone(),
        self.tempo_map.read().unwrap().clone(),
        self.latent_manager.pending_approvals(),
        self.latent_manager.active_jobs(),
    );

    match trustfall::execute_query(
        adapter.schema(),
        &adapter,
        &req.query,
        req.variables.unwrap_or_default(),
    ) {
        Ok(results) => {
            let rows: Vec<_> = results
                .take(req.limit.unwrap_or(100))
                .map(|r| r.into_iter().collect())
                .collect();
            QueryReply::Results { rows }
        }
        Err(e) => QueryReply::Error { error: e.to_string() },
    }
}
```

### Phase 4: Playback Integration

Create and manage PlaybackEngine:

```rust
impl GardenDaemon {
    pub fn initialize_playback(&mut self, sample_rate: u32, buffer_size: usize) -> Result<()> {
        let graph = self.graph.read().unwrap();
        let compiled = CompiledGraph::compile(&graph)?;

        let engine = PlaybackEngine::new(
            compiled,
            self.tempo_map.clone(),
            sample_rate,
            buffer_size,
        );

        *self.playback.write().unwrap() = Some(engine);
        Ok(())
    }

    fn process_audio(&mut self, output: &mut [f32]) {
        if let Some(ref mut engine) = *self.playback.write().unwrap() {
            engine.process(output);
        }
    }
}
```

### Phase 5: Content Resolution

Wire up content loading from CAS:

```rust
pub struct HootenannyCasResolver {
    hootenanny_url: String,
    cache: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl ContentResolver for HootenannyCasResolver {
    fn resolve(&self, content_hash: &str) -> Result<PathBuf> {
        // Check cache first
        if let Some(path) = self.cache.read().unwrap().get(content_hash) {
            return Ok(path.clone());
        }

        // Fetch from hootenanny CAS
        let path = self.fetch_from_cas(content_hash)?;
        self.cache.write().unwrap().insert(content_hash.to_string(), path.clone());
        Ok(path)
    }
}
```

### Phase 6: Event Publishing

Wire up IOPub broadcasts for latent events:

```rust
impl GardenDaemon {
    pub fn publish_event(&self, event: LatentEvent) {
        // Broadcast to all IOPub subscribers
        let _ = self.iopub_tx.send(event);
    }
}

// In latent_manager integration:
latent_manager.on_event(|event| {
    daemon.publish_event(event);
});
```

---

## File Changes

| File | Change |
|------|--------|
| `src/daemon.rs` | **New** - GardenDaemon struct + Handler impl |
| `src/bin/chaosgarden.rs` | Use GardenDaemon instead of StubHandler |
| `src/lib.rs` | Export daemon module |
| `src/query.rs` | Add `ChaosgardenAdapter::new()` factory |

---

## Testing Strategy

### Unit Tests (daemon.rs)

```rust
#[test]
fn test_transport_state_reflects_engine() {
    let daemon = GardenDaemon::new_test();
    daemon.initialize_playback(44100, 1024).unwrap();

    // Initially stopped
    let state = daemon.handle_shell(ShellRequest::GetTransportState);
    assert!(matches!(state, ShellReply::TransportState { playing: false, .. }));

    // After play
    daemon.handle_shell(ShellRequest::Play);
    let state = daemon.handle_shell(ShellRequest::GetTransportState);
    assert!(matches!(state, ShellReply::TransportState { playing: true, .. }));
}

#[test]
fn test_query_returns_regions() {
    let mut daemon = GardenDaemon::new_test();
    daemon.add_region(Region::play_audio(Beat(0.0), Beat(4.0), "hash123".into()));

    let reply = daemon.handle_query(QueryRequest {
        query: "{ Region { id @output position @output } }".into(),
        variables: None,
        limit: None,
    });

    assert!(matches!(reply, QueryReply::Results { rows } if !rows.is_empty()));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_full_ipc_roundtrip() {
    let daemon = Arc::new(GardenDaemon::new_test());
    let server = GardenServer::bind_with_handler(test_endpoints(), daemon.clone()).await?;

    let client = GardenClient::connect(test_endpoints()).await?;

    // Add region via shell
    client.shell(ShellRequest::AddRegion { ... }).await?;

    // Query it back
    let reply = client.query("{ Region { id @output } }").await?;
    assert!(!reply.rows.is_empty());
}
```

---

## Acceptance Criteria

- [x] `GardenDaemon` struct holds all state
- [x] `GetTransportState` returns real engine state
- [x] `Play`/`Pause`/`Stop`/`Seek` control playback
- [x] `SetTempo` updates tempo map (Arc<RwLock<TempoMap>> implemented)
- [x] `CreateRegion` creates regions in state
- [x] `DeleteRegion` removes regions from state
- [x] `MoveRegion` updates region position
- [x] Query socket returns real Trustfall results
- [x] Latent state updates flow through latent_manager
- [x] Approval operations (ApproveLatent/RejectLatent) work
- [x] GetPendingApprovals returns real pending approvals
- [ ] IOPub broadcasts latent events (NoOpPublisher for now)
- [x] Demo still works with real daemon
- [x] All existing tests still pass (171 tests)

---

## Progress (2025-12-13)

### Completed
- Created `src/daemon.rs` with `GardenDaemon` struct
- Wired transport: Play/Pause/Stop/Seek work via MCP
- Wired Trustfall queries via `ChaosgardenAdapter`
- Updated `src/bin/chaosgarden.rs` to use `GardenDaemon`
- **SetTempo**: Changed `TempoMap` to `Arc<RwLock<TempoMap>>`, added `set_base_tempo()` method
- **Region operations**: CreateRegion, DeleteRegion, MoveRegion all implemented
  - Behavior conversion from IPC types to internal types
  - Range filtering in GetRegions
- 11 new daemon tests covering region CRUD + tempo changes
- **Latent lifecycle**: Full lifecycle wired through LatentManager
  - UpdateLatentStarted/Progress/Resolved/Failed handlers
  - ApproveLatent/RejectLatent handlers
  - GetPendingApprovals returns real pending list
  - 6 new tests covering latent lifecycle
- MCP tools verified working:
  - `garden_status` → returns real transport state
  - `garden_play` → sets playing=true
  - `garden_seek` → updates position
  - `garden_stop` → resets to position=0
  - `garden_set_tempo` → updates base tempo
  - `garden_query` → executes Trustfall (regions now queryable!)

### Remaining
- IOPub event broadcasting (currently NoOpPublisher - will wire to actual ZMQ IOPub socket)

---

## Dependencies

This task depends on:
- All core modules (00-09) - already complete

This task blocks:
- 14-integration (end-to-end test)
- Actual music playback via MCP

---

## Estimated Scope

- **New code:** ~400 lines (daemon.rs)
- **Modified:** ~50 lines (bin/chaosgarden.rs, lib.rs)
- **Tests:** ~200 lines
- **Complexity:** Medium - mostly wiring, no new algorithms

---

## Notes

The demo (`cargo run -p chaosgarden --bin demo`) already exercises all the modules with in-memory state. This task is about making that state accessible via the ZMQ IPC interface.

Key insight: Don't try to run real PipeWire audio yet. Start with:
1. State management via IPC
2. Trustfall queries working
3. Render-to-file for verification

PipeWire integration (15-midir, external_io feature) is a separate concern.
