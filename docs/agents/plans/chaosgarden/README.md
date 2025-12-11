# Chaosgarden: Realtime Audio Daemon

**Location:** `crates/chaosgarden`
**Status:** 04-playback Complete, 05-external-io Next

---

## What Chaosgarden Is

The **realtime audio component** of the halfremembered system. A standalone daemon with RT priority that handles playback, graph state, and PipeWire I/O.

Not a DAW. Not an orchestrator. Pure **realtime audio focus**.

| Concern | Chaosgarden | Hootenanny (control plane) |
|---------|-------------|---------------------------|
| Audio playback | ✓ RT priority | - |
| Graph state | ✓ Timeline, regions | - |
| PipeWire | ✓ Owns connection | - |
| Job dispatch | - | ✓ Worker pool |
| CAS storage | - | ✓ Artifacts |
| Lua scripts | - | ✓ Luanette merged |
| Generation models | - | ✓ Via workers |

Chaosgarden receives commands from hootenanny, plays audio, and publishes events. It doesn't know about models, scripts, or job queues—just playback.

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                              hrmcp                                   │
│  (MCP proxy — translates HTTP/SSE to ZMQ, thin glue)                │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ ZMQ
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                           HOOTENANNY                                 │
│  (control plane daemon — orchestration, CAS, jobs, luanette)        │
│                                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐   │
│  │ Job Dispatch │  │     CAS      │  │   Luanette (scripts)     │   │
│  │ Worker Pool  │  │  Artifacts   │  │   Workflow engine        │   │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘   │
│                                                                      │
│  ZMQ: ROUTER for workers, DEALER to chaosgarden                     │
└───────────────────────────────┬─────────────────────────────────────┘
        │                       │                       │
        │ ZMQ                   │ ZMQ                   │ ZMQ
        ▼                       ▼                       ▼
┌───────────────┐      ┌───────────────┐      ┌───────────────┐
│  CHAOSGARDEN  │      │   Worker 1    │      │   Worker N    │
│  (RT audio)   │      │   (GPU)       │      │   (GPU)       │
│               │      │               │      │               │
│ - Playback    │      │ - orpheus_*   │      │ - rave_*      │
│ - Graph       │      │ - musicgen    │      │ - beatthis    │
│ - PipeWire    │      │               │      │               │
│ - Timeline    │      │               │      │               │
│ - Trustfall   │      │               │      │               │
└───────────────┘      └───────────────┘      └───────────────┘
```

**Component roles:**
- **hrmcp** — MCP-to-ZMQ proxy. Thin. Stateless.
- **hootenanny** — Control plane. CAS, jobs, scripts, worker registry.
- **chaosgarden** — RT audio. Playback, graph, PipeWire. This document.
- **workers** — GPU inference. Connect to hootenanny, pull jobs.

**ZMQ is the universal protocol.** All internal communication is ZMQ. MCP is just one external interface via hrmcp.

---

## Chaosgarden's Scope

```
┌────────────────────────────────────────────────────────────────────┐
│                      CHAOSGARDEN DAEMON                             │
│                      (RT priority, single instance)                 │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                      ZMQ INTERFACE                             │ │
│  │  Control (ROUTER) — stop, pause, shutdown                      │ │
│  │  Shell (ROUTER) — commands from hootenanny                     │ │
│  │  IOPub (PUB) — events to all subscribers                       │ │
│  │  Heartbeat (REP) — liveness                                    │ │
│  │  Query (REP) — Trustfall queries                               │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                              │                                      │
│                              ▼                                      │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                      TIMELINE STATE                            │ │
│  │  Regions (position, duration, behavior)                        │ │
│  │  Latent tracking (job_id, progress, resolved artifact)         │ │
│  │  Transport (playing, position, tempo)                          │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                              │                                      │
│                              ▼                                      │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                      PLAYBACK ENGINE                           │ │
│  │  Compiled graph (allocation-free hot path)                     │ │
│  │  Buffer management                                             │ │
│  │  PDC (latency compensation)                                    │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                              │                                      │
│                              ▼                                      │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                      PIPEWIRE I/O                              │ │
│  │  Audio output to hardware                                      │ │
│  │  MIDI input/output                                             │ │
│  │  Device enumeration                                            │ │
│  └───────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────┘
```

**What chaosgarden does:**
- Receives artifact references from hootenanny (content hashes)
- Loads audio/MIDI from CAS path or via hootenanny request
- Manages timeline state (regions, transport)
- Runs the realtime playback loop
- Publishes events (position, latent state changes)
- Answers Trustfall queries about graph state

**What chaosgarden does NOT do:**
- Job dispatch (hootenanny)
- Model inference (workers)
- Lua script execution (hootenanny/luanette)
- CAS management (hootenanny)
- MCP protocol (hrmcp)

---

## Crate Structure

```
crates/chaosgarden/
├── src/
│   ├── lib.rs
│   ├── ipc.rs            # 00: ZeroMQ sockets, protocol, messages
│   ├── primitives.rs     # 01: Time, Signal, Node, Region
│   ├── graph.rs          # 02: Graph topology
│   ├── latent.rs         # 03: Latent lifecycle, resolution, mixing-in
│   ├── playback.rs       # 04: Realtime engine, buffer management
│   ├── external_io.rs    # 05: PipeWire (feature-gated)
│   ├── query.rs          # 06: Trustfall adapter
│   ├── patterns.rs       # 07: Track, Bus, Timeline (optional ergonomics)
│   ├── capabilities.rs   # 08: Participant capabilities
│   └── daemon.rs         # Main daemon entry point
├── bin/
│   └── chaosgarden.rs    # Daemon binary
└── Cargo.toml
```

---

## Progress Tracking

| Task | Status | Notes |
|------|--------|-------|
| 00-ipc | ✅ complete | ZeroMQ 5-socket protocol, MessagePack wire format, 12 tests passing |
| 01-primitives | ✅ complete | Time, Signal, Node, Region, Lifecycle — 14 tests passing |
| 02-graph | ✅ complete | petgraph DAG, topology, traversal — 12 tests passing |
| 03-latent | ✅ complete | Latent lifecycle, HITL approval, mixing-in — 10 tests passing |
| 04-playback | ✅ complete | CompiledGraph, PlaybackEngine, render_to_file — 9 tests passing |
| 05-external-io | ✅ complete | ExternalIOManager, I/O nodes, RingBuffer — 16 tests passing |
| 06-query | ✅ complete | ChaosgardenAdapter, 14 query tests passing |
| 07-patterns | ✅ complete | Track, Bus, Section, Timeline, Project — 22 tests passing |
| 08-capabilities | ✅ complete | CapabilityRegistry, Participant, identity matching — 23 tests passing |
| 09-audio-integration | ✅ complete | AudioFileNode + CAS + region wiring — 21 tests passing |

## Current Status

- **Completed**: Core modules (00-09) + Enhanced Demo 🎉
- **In Progress**: None
- **Next Up**: 10-hootenanny-zmq (add ZMQ server to control plane)
- **Blocked**: None

### Demo Showcase

The demo (`cargo run -p chaosgarden --bin demo`) demonstrates all major features:

```bash
cargo run -p chaosgarden --bin demo           # Compact output
cargo run -p chaosgarden --bin demo -- -v     # Verbose with ASCII art
cargo run -p chaosgarden --bin demo -- -l 80  # 80 beats (~40s)
```

| Feature | Demo Coverage |
|---------|---------------|
| Audio playback | 5 tracks, MemoryResolver, region→AudioFileNode |
| Kick drum | Punchy synthesis with click transient, four-on-the-floor |
| Tempo change | 120→130 BPM at beat 8, verified by Beat This |
| Latent lifecycle | job→progress→resolve→approve simulation |
| Mix-in scheduling | Crossfade strategy at beat boundary |
| Trustfall queries | Regions, latent status, time conversion |
| Capabilities | Model/human/device participants |
| Dynamic looping | Regions auto-extend to match --length |

### Audio Integration Complete (09)

| Subtask | Status | Notes |
|---------|--------|-------|
| 09a AudioFileNode | ✅ done | symphonia decode, WAV always, MP3/FLAC via feature flag |
| 09b CAS integration | ✅ done | ContentResolver trait, FileCasClient, MemoryResolver |
| 09c Playback wiring | ✅ done | Region activate/deactivate, seek, gain, mixing |

### 00-ipc Acceptance Criteria (2025-12-11)

- [x] `GardenServer::bind()` creates all 5 sockets
- [x] `GardenClient::connect()` connects to running daemon
- [x] Shell request/reply round-trip works
- [x] Control messages bypass shell queue
- [x] IOPub events broadcast to all subscribers
- [x] Heartbeat ping/pong works with timeout
- [x] Query socket handles requests
- [x] MessagePack serialization works
- [x] Tests pass with ipc:// transport (12 tests)

### 01-primitives Acceptance Criteria (2025-12-11)

- [x] All types compile with derives as shown
- [x] TempoMap converts tick↔beat↔second↔sample accurately with tempo changes
- [x] `cargo test` passes for time conversion round-trips (5 tests)
- [x] Node trait is object-safe (`Box<dyn Node>` works)
- [x] Region serializes/deserializes via serde_json
- [x] Latent state transitions work correctly
- [x] `is_playable()` returns true for PlayContent and approved Latent regions
- [x] Region lifecycle methods delegate correctly to `self.lifecycle`
- [x] Tombstoned regions report `is_alive() == false`
- [x] Signal buffer tests (AudioBuffer mix, MidiBuffer merge, ControlBuffer interpolation)

### 02-graph Acceptance Criteria (2025-12-11)

- [x] `add_node` + `connect` creates edges in petgraph
- [x] `processing_order` returns topologically sorted nodes
- [x] `CycleDetected` error when cycle would form
- [x] `upstream`/`downstream` traverse correctly
- [x] Port type mismatch returns `TypeMismatch` error
- [x] `bypass_node` preserves signal flow around bypassed node
- [x] `sources`/`sinks` find graph endpoints
- [x] `signal_path` finds path between nodes
- [x] `find_by_type` filters by type prefix
- [x] `snapshot` creates serializable graph state

### 03-latent Acceptance Criteria (2025-12-11)

- [x] `handle_job_started` updates region state and emits event
- [x] `handle_progress` updates progress and emits event
- [x] `handle_resolved` creates pending approval or auto-approves
- [x] `handle_failed` transitions to failed state
- [x] `approve()` / `reject()` transitions state correctly with decision tracking
- [x] `schedule_mix_in()` produces valid schedule
- [x] Events emitted for all state transitions
- [x] Auto-approve works for configured tools
- [x] `max_concurrent_jobs` is respected
- [x] Decision log tracks who made each decision

### 04-playback Acceptance Criteria (2025-12-11)

- [x] `CompiledGraph::compile` builds from Graph
- [x] `process()` calls nodes in topological order
- [x] `ProcessError::Skipped` outputs silence, continues
- [x] `ProcessError::Failed` marks node failed, skips in future
- [x] `queue_mix_in()` schedules content introduction
- [x] HardCut mix-in works at beat boundary
- [x] Crossfade mix-in schedules blend
- [x] `render_to_file` produces valid WAV
- [x] Transport controls (play/stop/seek) work correctly

### 05-external-io Acceptance Criteria (2025-12-11)

- [x] Feature gate compiles with and without `pipewire`
- [x] `ExternalIOManager::create_output` registers output
- [x] `ExternalIOManager::create_input` registers input
- [x] `ExternalIOManager::register_midi` registers MIDI device
- [x] `RingBuffer` write/read with wraparound works
- [x] `ExternalOutputNode` implements Node trait
- [x] `ExternalInputNode` implements Node trait
- [x] `MidiInputNode` captures and sorts events
- [x] `MidiOutputNode` queues events for callback
- [x] Active/inactive state controls processing

### 06-query Acceptance Criteria (2025-12-11)

- [x] Schema parses without errors
- [x] `resolve_starting_vertices` returns regions/nodes/jobs
- [x] `resolve_property` extracts all scalar fields
- [x] `resolve_neighbors` traverses upstream/downstream
- [x] Latent state fields resolve correctly
- [x] Job and Approval queries work
- [x] Time conversion queries return accurate results
- [x] Example queries execute and return correct results

### 07-patterns Acceptance Criteria (2025-12-11)

- [x] Track/Bus/Section serialize via serde_json
- [x] `Timeline::build_graph()` produces valid Graph
- [x] Routing respects TrackOutput and BusOutput
- [x] Sends wire correctly with gain
- [x] `Project::save/load` round-trips correctly
- [x] SectionHints available via `hints_at()`
- [x] `add_latent()` convenience creates properly-formed latent regions

### 08-capabilities Acceptance Criteria (2025-12-11)

- [x] `CapabilityUri::matches_prefix` correctly filters URIs
- [x] `Participant::can_satisfy` checks all requirements
- [x] `Constraint::satisfies` handles Exact, Range, Min, Max, Enum cases
- [x] `CapabilityRegistry::register` adds participant
- [x] `CapabilityRegistry::find_satisfying` returns correct participants
- [x] `CapabilityRegistry::query_capabilities` filters by prefix
- [x] Serialization round-trips correctly for all types
- [x] Thread-safe access via `RwLock`
- [x] `IdentityHints::match_score` returns sensible scores
- [x] `find_identity_matches` returns Exact for high-confidence matches
- [x] `find_by_tag` returns participants with matching tag
- [x] Lifecycle management (touch, tombstone, stale_since) works

---

## Design Principles

1. **RT audio daemon** — chaosgarden runs with RT priority; hootenanny orchestrates, chaosgarden plays
2. **ZMQ everywhere** — internal communication is ZMQ; MCP is just one external interface
3. **Latent as first-class** — intentions visible before realization
4. **Generation ≠ playback** — hootenanny generates, chaosgarden plays; connected by artifact resolution
5. **Queryable everything** — Trustfall enables reasoning about graph state
6. **Time is musical** — beats, not samples
7. **Content is immutable** — CAS (in hootenanny) for lineage and deduplication
8. **Capabilities over roles** — participants declare what they can do
9. **Soft deletes via tombstones** — grooming without data loss; recovery is possible
10. **Scripts as graphs** — Lua scripts in hootenanny/luanette handle generative workflows
11. **Jupyter-inspired IPC** — 5 ZMQ sockets (Control, Shell, IOPub, Heartbeat, Query)

---

## Dependencies

```toml
[dependencies]
# IPC
zeromq = "0.4"
rmp-serde = "1"              # MessagePack for wire format

# Core
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"

# Graph
petgraph = "0.6"

# Query
trustfall = "0.8"

# Async
tokio = { version = "1", features = ["full"] }

# Audio
hound = "3.5"
midly = "0.5"
rustysynth = "1.3"

# Observability
tracing = "0.1"

[features]
default = []
pipewire = ["dep:pipewire"]

[[bin]]
name = "chaosgarden"
path = "src/bin/chaosgarden.rs"
```

**Note:** Chaosgarden has no direct dependency on hootenanny. They communicate via ZMQ. CAS artifacts are accessed by filesystem path (hootenanny tells chaosgarden where to find them).

---

## Success Metrics

We'll know we've succeeded when:
- [ ] Latent region receives progress updates from hootenanny, tracks state, resolves
- [ ] Human can audition resolved content and approve/reject via Shell commands
- [ ] Approved content mixes into playback at beat boundary
- [ ] Multiple latent regions can be tracked simultaneously
- [ ] Trustfall queries (via Query socket) let agents reason about the performance
- [ ] Graph renders valid audio to file and to speakers (via PipeWire)

---

## Open Questions

- [x] Capability registry/discovery system design (08-capabilities)
- [x] Latent dependency chains (Scripts as Graphs via `luanette`)
- [ ] Voice/text HITL interface design
- [ ] Crossfade algorithm selection (simple defaults first)

---

## Signoffs & Decisions

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-12-08 | Initial design docs | Claude/Gemini/Human collaboration |
| 2025-12-10 | Separated generation/playback | Conceptual clarity, matches async reality |
| 2025-12-10 | Latent as first-class primitive | Makes intent visible, enables HITL |
| 2025-12-10 | URI-namespaced capabilities | Extensible without central coordination; declaration over negotiation |
| 2025-12-10 | Generational tracking + tombstones | Enables grooming without immediate deletion; soft deletes are forgiving |
| 2025-12-10 | Repair-first device identity | IdentityHints + match scoring; easy reconnection over rigid matching |
| 2025-12-10 | Tags on Participant and Region | Flexible organization: "backup", "locked", "week-2", "overture" |
| 2025-12-11 | ZMQ for all internal IPC | Jupyter-inspired; process isolation; horizontal scaling |
| 2025-12-11 | Chaosgarden as RT daemon | Playback isolated from orchestration; RT priority for audio |
| 2025-12-11 | Hootenanny as control plane | CAS, jobs, luanette merged; orchestrates chaosgarden and workers |
| 2025-12-11 | hrmcp as thin MCP proxy | MCP is one external interface; ZMQ is internal protocol |
| 2025-12-11 | No Lua in chaosgarden | RT safety; GC pauses unacceptable; dispatch to hootenanny instead |
| 2025-12-11 | 00-ipc implementation complete | GardenServer, GardenClient, MessagePack, 12 tests passing |
| 2025-12-11 | 01-primitives implementation complete | Time, Signal, Node, Region, Lifecycle — 14 tests passing |
| 2025-12-11 | 02-graph implementation complete | petgraph DAG, topology, traversal — 12 tests passing |
| 2025-12-11 | 03-latent implementation complete | Latent lifecycle, HITL approval, mixing-in — 10 tests passing |
| 2025-12-11 | 04-playback implementation complete | CompiledGraph, PlaybackEngine, render_to_file — 9 tests passing |
| 2025-12-11 | 05-external-io implementation complete | ExternalIOManager, I/O nodes, RingBuffer — 16 tests passing |
| 2025-12-11 | 06-query implementation complete | ChaosgardenAdapter Trustfall adapter — 14 tests passing |
| 2025-12-11 | 07-patterns implementation complete | Track, Bus, Section, Timeline, Project — 22 tests passing |
| 2025-12-11 | 08-capabilities implementation complete | CapabilityRegistry, Participant, identity matching — 23 tests passing |
| 2025-12-11 | **Core modules complete!** | 132 unit tests + 5 integration tests = 137 total |
| 2025-12-11 | 09 audio-integration complete | AudioFileNode, ContentResolver, region wiring — 21 tests, 158 total |
| 2025-12-11 | Demo enhanced | Full showcase: latent lifecycle, Trustfall queries, tempo change, kick drum |
| 2025-12-11 | Tempo change verified | Beat This confirms 120→130 BPM transition at beat 8 |

---

## Documents

| Document | Focus | Read When |
|----------|-------|-----------|
| [DETAIL.md](./DETAIL.md) | Full design rationale, philosophy | Deep revision sessions |
| [00-ipc](./00-ipc.md) | ZMQ sockets, protocol | Implementing ipc.rs |
| [01-primitives](./01-primitives.md) | Time, Signal, Node, Region | Implementing primitives.rs |
| [02-graph](./02-graph.md) | DAG topology, petgraph usage | Implementing graph.rs |
| [03-latent](./03-latent.md) | Latent lifecycle, resolution, mixing-in | Implementing latent.rs |
| [04-playback](./04-playback.md) | Realtime engine, buffers | Implementing playback.rs |
| [05-external-io](./05-external-io.md) | PipeWire integration | Implementing external_io.rs |
| [06-query](./06-query.md) | Trustfall adapter | Implementing query.rs |
| [07-patterns](./07-patterns.md) | Track, Bus, Section | Implementing patterns.rs |
| [08-capabilities](./08-capabilities.md) | Participant capabilities | Implementing capabilities.rs |

---

## Phase 2: Refactoring Existing Crates

These tasks come AFTER chaosgarden is working. Don't rip up working code until we've proven the new architecture.

| Task | Status | Notes |
|------|--------|-------|
| 10-hootenanny-zmq | future | Add ZMQ server to hootenanny, become control plane |
| 11-hootenanny-workers | future | Worker registry, PUSH/PULL job dispatch |
| 12-luanette-merge | future | Merge luanette into hootenanny as workflow engine |
| 13-hrmcp-proxy | future | Strip MCP server to thin ZMQ proxy |
| 14-integration | future | End-to-end: hrmcp → hootenanny → chaosgarden → audio |
| 15-midir | future | Direct MIDI via midir (ALSA seq/CoreMIDI/WinMM) - avoids PipeWire MIDI jitter |

### 10-hootenanny-zmq

Add ZMQ infrastructure to hootenanny:
- GardenClient to connect to chaosgarden
- ROUTER socket for worker registration
- PUSH socket for job dispatch
- SUB socket for worker results
- Forward IOPub events to interested parties (hrmcp, visualization)

### 11-hootenanny-workers

Worker pool management:
- Worker registration protocol (capabilities, resources)
- Job routing based on capabilities
- Heartbeat monitoring
- Worker crash recovery

### 12-luanette-merge

Merge luanette into hootenanny:
- Lua runtime becomes part of hootenanny
- Scripts dispatch jobs to workers via ZMQ
- OTLP tracing for script execution
- Script results flow to chaosgarden as resolved latents

### 13-hrmcp-proxy

Strip current MCP server to thin proxy:
- Keep HTTP/SSE interface
- Remove direct tool implementations
- Forward MCP calls to hootenanny via ZMQ
- Stream IOPub events back as SSE

### 14-integration

Full system integration test:
- Start all daemons (hootenanny, chaosgarden, worker)
- MCP call triggers generation
- Worker produces artifact
- Chaosgarden plays audio
- Verify end-to-end latency and correctness

### 15-midir

Direct MIDI hardware access via `midir` crate instead of PipeWire MIDI routing.

**Why midir over PipeWire MIDI:**
- PipeWire MIDI has known jitter issues (GitLab #3657) - timing quantized to audio buffer boundaries
- midir uses ALSA seq directly on Linux, CoreMIDI on macOS, WinMM on Windows
- Cross-platform: enables macOS/Windows/WebMIDI if needed
- Same ALSA seq backend as PipeWire but without the buffer-induced jitter

**Scope:**
- Feature-gated `midir` backend (default on, PipeWire MIDI available via feature flag)
- Wire midir callbacks to existing `MidiInputNode::push_event()` / `MidiOutputNode::drain_events()`
- Device enumeration and hot-plug handling
- Lock-free SPSC queue for RT safety (replace current `Arc<Mutex<Vec>>` in MidiInputNode)
