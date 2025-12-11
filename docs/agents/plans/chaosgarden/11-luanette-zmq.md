# 11-luanette-zmq: Luanette over ZMQ

**Prerequisite**: 10-hootenanny-zmq ✅
**Status**: Planning

## Goal

Luanette becomes the Lua orchestration layer, communicating over ZMQ with Holler (MCP gateway), Chaosgarden (real-time engine), and direct CLI access via `holler` subcommands. A single Luanette instance can orchestrate calls to any number of backend model services over HTTP. The filesystem (via shared CAS) is the bulk data plane; ZMQ carries only control messages and CAS hashes.

## Architecture

See [12-holler.md](./12-holler.md) for the full system architecture. Luanette's role:

```
┌─────────────────────────────────────────────────────────────────────┐
│                            LUANETTE                                  │
│                    (Lua Orchestration Layer)                         │
│                                                                      │
│  ZMQ ROUTER :5570 ◄─────────────────────────────────────────────────┤
│  (binds, accepts connections from Holler, Chaosgarden, holler CLI)  │
│                                                                      │
│  Embedded:                           Lua stdlib:                     │
│  ├─ cas crate (filesystem access)    ├─ midi.* (pure, local)        │
│  ├─ mlua runtime                     ├─ abc.* (pure, local)         │
│  └─ Job system (async execution)     ├─ cas.* (via embedded crate)  │
│                                      ├─ orpheus.* → HTTP            │
│                                      ├─ musicgen.* → HTTP           │
│                                      └─ deepseek.* → HTTP           │
│  Future:                                                             │
│  └─ chaosgarden.* (timeline queries via ZMQ to Chaosgarden)         │
└─────────────────────────────────────────────────────────────────────┘
         ▲
         │ ZMQ DEALER connections from:
         ├─ Holler (MCP gateway via `holler serve`)
         ├─ Chaosgarden (real-time triggers)
         └─ holler CLI (direct access via `holler lua`, etc.)
```

Luanette calls model services over HTTP:
```
┌───────────┐  ┌────────────┐  ┌───────────┐
│ Orpheus   │  │ MusicGen   │  │ LLM APIs  │
│ (GPU)     │  │ (GPU)      │  │ (remote)  │
└───────────┘  └────────────┘  └───────────┘
```

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Protocol crate | `hooteproto` | Unified protocol for all ZMQ messages |
| Scaling model | One Luanette orchestrates many model backends | Luanette ≠ GPU; it's the scripting layer |
| Topology | Luanette binds ROUTER :5570 | Direct paths from Holler, Chaosgarden, holler CLI |
| Job ownership | Luanette owns async jobs | Scripts are the unit of work |
| CAS access | Embedded crate, shared filesystem | Bulk data stays on disk |

## The Chaosgarden→Luanette Flow

Timeline-driven Lua execution for latent/generative moments:

```lua
-- Chaosgarden hits a "generate transition" marker in timeline
-- Sends ZMQ request to Luanette:
{
  "type": "job_execute",
  "script_hash": "abc123...",  -- pre-stored transition script
  "params": {
    "current_section": "cas:deadbeef...",  -- CAS hash of current MIDI
    "next_section": "cas:cafebabe...",
    "beats_available": 8,
    "tempo": 120
  }
}

-- Luanette runs the Lua script, which might:
-- 1. Read both MIDI files from CAS (local fs)
-- 2. Call orpheus.bridge() for AI transition
-- 3. Store result in CAS
-- 4. Return the new CAS hash

-- Chaosgarden receives hash, loads audio, queues for playback
```

## Protocol Design (hooteproto crate)

### Socket Pattern

| Socket | Who Binds | Who Connects | Purpose |
|--------|-----------|--------------|---------|
| ROUTER | Luanette :5570 | Holler, Chaosgarden, holler CLI (DEALER) | Request/reply |
| PUSH | Luanette :5571 | (future workers) PULL | Async job dispatch |
| PUB | Luanette :5572 | (future) SUB | Broadcasts |

### Message Types

```rust
// crates/hooteproto/src/lib.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Worker announces itself to the hub
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRegistration {
    pub worker_id: Uuid,
    pub worker_type: WorkerType,
    pub worker_name: String,
    pub capabilities: Vec<String>,
    pub hostname: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerType {
    Luanette,
    Chaosgarden,
}

/// Envelope for all ZMQ messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: Uuid,
    pub traceparent: Option<String>,
    pub payload: Payload,
}

/// All message types in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Payload {
    // === Worker Management ===
    Register(WorkerRegistration),
    Ping,
    Pong { worker_id: Uuid, uptime_secs: u64 },
    Shutdown { reason: String },

    // === Lua Tools (Holler → Luanette) ===
    LuaEval { code: String, params: Option<serde_json::Value> },
    LuaDescribe { script_hash: String },
    ScriptStore { content: String, tags: Option<Vec<String>>, creator: Option<String> },
    ScriptSearch { tag: Option<String>, creator: Option<String>, vibe: Option<String> },

    // === Job System (any → Luanette) ===
    JobExecute { script_hash: String, params: serde_json::Value, tags: Option<Vec<String>> },
    JobStatus { job_id: String },
    JobPoll { job_ids: Vec<String>, timeout_ms: u64, mode: PollMode },
    JobCancel { job_id: String },
    JobList { status: Option<String> },

    // === Resources ===
    ReadResource { uri: String },
    ListResources,

    // === Prompts ===
    GetPrompt { name: String, arguments: HashMap<String, String> },
    ListPrompts,

    // === Completions ===
    Complete { context: String, partial: String },

    // === Chaosgarden Events (Chaosgarden → Luanette) ===
    TimelineEvent { event_type: TimelineEventType, position_beats: f64, tempo: f64, metadata: serde_json::Value },

    // === CAS Tools (Holler → Hootenanny) ===
    CasStore { data: Vec<u8>, mime_type: Option<String> },
    CasInspect { hash: String },
    CasGet { hash: String },

    // === Artifact Tools (Holler → Hootenanny) ===
    ArtifactGet { id: String },
    ArtifactList { tag: Option<String>, creator: Option<String> },
    ArtifactCreate { cas_hash: String, tags: Vec<String>, creator: Option<String>, metadata: serde_json::Value },

    // === Graph Tools (Holler → Hootenanny) ===
    GraphQuery { query: String, variables: serde_json::Value },
    GraphBind { identity: String, hints: Vec<String> },

    // === Transport Tools (Holler → Chaosgarden) ===
    TransportPlay,
    TransportStop,
    TransportSeek { position_beats: f64 },
    TransportStatus,

    // === Timeline Tools (Holler → Chaosgarden) ===
    TimelineQuery { from_beats: Option<f64>, to_beats: Option<f64> },
    TimelineAddMarker { position_beats: f64, marker_type: String, metadata: serde_json::Value },

    // === Tool Discovery (Holler → any backend) ===
    ListTools,
    ToolList { tools: Vec<ToolInfo> },

    // === Responses ===
    Success { result: serde_json::Value },
    Error { code: String, message: String, details: Option<serde_json::Value> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PollMode {
    Any,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineEventType {
    SectionChange,
    BeatMarker,
    CuePoint,
    GenerateTransition,
}

/// Broadcast messages via PUB/SUB
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Broadcast {
    ConfigUpdate { key: String, value: serde_json::Value },
    Shutdown { reason: String },
    ScriptInvalidate { hash: String },
}
```

## What Changes in Luanette

### Remove
- `main.rs` axum server setup
- `handler.rs` baton::Handler impl
- `clients/` MCP client code (hootenanny proxies now)
- Direct HTTP server

### Keep (unchanged)
- `runtime.rs` - Lua VM execution
- `stdlib/` - midi.*, abc.*, temp.*
- `otel_bridge.rs` - OpenTelemetry
- `error.rs` - Error formatting
- `job_system.rs` - Job tracking (Luanette owns jobs)
- `telemetry.rs` - OTLP setup

### Add
- `worker.rs` - ZMQ socket management, message loop
- `dispatch.rs` - Route Payload variants to handlers
- Dependency on `hooteproto` crate
- Dependency on `cas` crate (embedded, for direct file access)

### Modify
- `main.rs` - ZMQ worker startup instead of HTTP
- `handler.rs` → `dispatch.rs` - Same logic, different interface
  - Instead of `async fn call_tool(name, args) -> CallToolResult`
  - Now `async fn handle(payload: Payload) -> Payload`

## What Changes in Hootenanny

Hootenanny becomes a ZMQ service (no longer HTTP). See [12-holler.md](./12-holler.md) for details.

### Add crate dependency
- `hooteproto` - Shared message types

### Add
- `zmq/server.rs` - ROUTER socket (binds :5580)
- `zmq/dispatch.rs` - Route Payload variants to handlers

### Keep
- CAS, artifact store, graph queries - now exposed via ZMQ instead of HTTP

## What Changes in Chaosgarden

### Add crate dependencies
- `hooteproto` - Shared message types (for `Envelope`, `Payload`, `TimelineEventType`)
- `cas` - Direct filesystem access (already planned in 09a)

### Add
- `zmq_client.rs` - DEALER socket connecting to Luanette's ROUTER
- Timeline event emission for generative triggers
- Marker types in timeline for `GenerateTransition`, `CuePoint`, etc.

## Implementation Plan

### Phase 1: hooteproto Crate
See [12-holler.md](./12-holler.md) Phase 1 - hooteproto is the shared foundation.

### Phase 2: Luanette ZMQ Server
- [ ] Add hooteproto dependency
- [ ] Create `server.rs` with ZMQ ROUTER socket (binds :5570)
- [ ] Create `dispatch.rs` routing Payload → handlers
- [ ] Refactor handlers from baton::Handler to dispatch functions
- [ ] Add cas crate dependency for direct file access
- [ ] Update `main.rs` for ZMQ server startup
- [ ] Remove HTTP/MCP code

### Phase 3: Hootenanny ZMQ Service
- [ ] Add hooteproto dependency
- [ ] Create `zmq/server.rs` with ROUTER socket (binds :5580)
- [ ] Create `zmq/dispatch.rs` routing Payload → handlers
- [ ] Expose CAS, artifacts, graph via ZMQ
- [ ] Remove HTTP server code

### Phase 4: Holler MCP Gateway
See [12-holler.md](./12-holler.md) for full plan.
- [ ] Create `crates/holler/`
- [ ] MCP Streamable HTTP server
- [ ] ZMQ DEALER connections to Luanette, Hootenanny, Chaosgarden
- [ ] Tool routing based on prefix

### Phase 5: Chaosgarden Integration
- [ ] Add hooteproto dependency to chaosgarden
- [ ] Create `zmq_client.rs` with DEALER socket (connects to Luanette)
- [ ] Add timeline marker types for generative triggers
- [ ] Fire `TimelineEvent` payloads at musical boundaries
- [ ] Test: timeline marker → Lua script → CAS result

### Phase 6: Future Scaling (deferred)
- [ ] Add PUSH socket in Luanette for sub-worker dispatch
- [ ] Add PULL socket in sub-workers
- [ ] Route long-running jobs through PUSH for load balancing

### Phase 7: Testing & Polish
- [ ] Integration test: Holler → Luanette → result
- [ ] Integration test: Chaosgarden → Luanette → CAS hash
- [ ] Integration test: `holler lua` → Luanette (direct ZMQ)
- [ ] Test client disconnect/reconnect
- [ ] Test job cancellation across boundary
- [ ] Verify OTEL traces propagate through ZMQ

## CLI

### Luanette (binds ROUTER)
```bash
luanette \
    --bind tcp://0.0.0.0:5570 \
    --name "lua-orchestrator" \
    --cas-root /path/to/cas \
    --otlp-endpoint 127.0.0.1:35991
```

### Hootenanny (binds ROUTER - no longer HTTP)
```bash
hootenanny \
    --bind tcp://0.0.0.0:5580 \
    --cas-root /path/to/cas
```

### Chaosgarden (binds ROUTER, connects DEALER to Luanette)
```bash
chaosgarden \
    --bind tcp://0.0.0.0:5555 \
    --luanette tcp://127.0.0.1:5570 \
    --cas-root /path/to/cas
```

### Holler (MCP gateway, connects to all backends)
```bash
holler serve \
    --port 8080 \
    --luanette tcp://127.0.0.1:5570 \
    --hootenanny tcp://127.0.0.1:5580 \
    --chaosgarden tcp://127.0.0.1:5555
```

## Port Allocation

| Port | Protocol | Who Binds | Purpose |
|------|----------|-----------|---------|
| 8080 | HTTP | Holler | MCP gateway for clients |
| 5555 | ZMQ ROUTER | Chaosgarden | Real-time engine |
| 5570 | ZMQ ROUTER | Luanette | Lua orchestration |
| 5580 | ZMQ ROUTER | Hootenanny | CAS, artifacts, graph |

## Acceptance Criteria

- [ ] Luanette binds ROUTER on :5570, accepts connections
- [ ] Holler connects DEALER to Luanette, proxies lua_* MCP tools
- [ ] Chaosgarden connects DEALER to Luanette for real-time triggers
- [ ] Timeline events fire Lua scripts, return CAS hashes
- [ ] `holler lua` can connect directly to Luanette (bypass `holler serve`)
- [ ] CAS hashes flow through control plane, files stay on disk
- [ ] OTEL traces propagate through ZMQ boundary (traceparent in Envelope)
- [ ] Graceful shutdown: Luanette drains in-flight requests

## Resolved Questions

| Question | Answer |
|----------|--------|
| Shared crate for messages? | Yes - `hooteproto` crate |
| Luanette scaling model? | One Luanette orchestrates many backends via HTTP |
| CAS access? | Embed `cas` crate, filesystem is data plane |
| MCP gateway? | Holler - thin bridge, routes by tool prefix |
| Hootenanny role? | ZMQ service for CAS/artifacts/graph, no HTTP |
| Direct access? | `holler` CLI can bypass `holler serve` and talk ZMQ to any backend |

## Future Work

- Multiple Luanette workers with load balancing (PUSH/PULL ready)
- Worker auto-scaling based on queue depth
- Job priorities and preemption
- `chaosgarden.*` Lua stdlib for timeline queries
- Hot reload of Lua scripts
- Worker groups for capability isolation
