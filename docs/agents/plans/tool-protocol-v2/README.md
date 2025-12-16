# Tool Protocol v2: Beyond MCP

**Status:** In Progress (paused mid-implementation)
**Goal:** A clean, strongly-typed tool protocol for internal communication. JSON only at MCP edge.

## Problem Statement

Current state:
- Tools defined with JSON schemas (MCP convention)
- Dispatch layer converts Payload → JSON → dispatch → JSON → Payload
- Wire format is MsgPack, but internal representation is JSON
- Timing (sync/async) is implicit, handled ad-hoc

This creates:
1. **Type erasure** - Rich Rust types become stringly-typed JSON
2. **Serialization overhead** - JSON parsing in hot paths
3. **Ambiguous timing** - Callers don't know if a tool blocks
4. **MCP coupling** - Internal design constrained by MCP's limitations

## Design Goals

1. **Types all the way down** - Rust types for requests AND responses
2. **MsgPack on the wire** - No JSON except at Holler/MCP boundary
3. **Explicit timing** - Protocol-level distinction: sync, async, fire-and-forget
4. **Streaming-ready** - Design that can accommodate future streaming
5. **Holler adapts** - MCP translation happens at edge, not in core

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         MCP Clients (Claude, etc.)                  │
│                              JSON/HTTP                              │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                              HOLLER                                 │
│                                                                     │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────┐ │
│  │ MCP Handler │───▶│ JSON ↔ Type │───▶│ ZMQ Client (MsgPack)    │ │
│  │             │    │  Conversion │    │                         │ │
│  └─────────────┘    └─────────────┘    └─────────────────────────┘ │
│                                                                     │
│  - Validates JSON against schemas                                   │
│  - Converts to typed ToolRequest                                    │
│  - Handles async polling for MCP clients                            │
│  - Converts ToolResponse back to JSON                               │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ ZMQ + MsgPack
                                    │ HOOT01 frames
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                           HOOTENANNY                                │
│                                                                     │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                    Tool Dispatcher                           │   │
│  │                                                              │   │
│  │  ToolRequest ──▶ match on variant ──▶ execute ──▶ ToolResponse│  │
│  │                                                              │   │
│  │  No JSON. Types only. MsgPack serialization at wire.         │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │
│  │  Job Store   │  │  CAS Store   │  │  Artifact Store          │  │
│  └──────────────┘  └──────────────┘  └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ ZMQ + MsgPack
                                    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          CHAOSGARDEN                                │
│                                                                     │
│  - Fire-and-forget requests                                         │
│  - IOPub subscription for job completion                            │
│  - Never blocks on tool responses                                   │
└─────────────────────────────────────────────────────────────────────┘
```

## Phase 1: Define Core Types

### 1.1 ToolRequest enum

Replace the sprawling `Payload` enum with focused request types:

```rust
// crates/hooteproto/src/request.rs

/// All tool requests. Each variant is a complete, typed request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum ToolRequest {
    // === CAS ===
    CasStore(CasStoreRequest),
    CasInspect(CasInspectRequest),
    CasGet(CasGetRequest),

    // === Artifacts ===
    ArtifactUpload(ArtifactUploadRequest),
    ArtifactGet(ArtifactGetRequest),
    ArtifactList(ArtifactListRequest),

    // === Orpheus ===
    OrpheusGenerate(OrpheusGenerateRequest),
    OrpheusContinue(OrpheusContinueRequest),
    OrpheusBridge(OrpheusBridgeRequest),
    OrpheusLoops(OrpheusLoopsRequest),
    OrpheusClassify(OrpheusClassifyRequest),

    // === Audio ===
    MidiToWav(MidiToWavRequest),
    MusicgenGenerate(MusicgenGenerateRequest),
    YueGenerate(YueGenerateRequest),
    ClapAnalyze(ClapAnalyzeRequest),
    BeatthisAnalyze(BeatthisAnalyzeRequest),

    // === ABC ===
    AbcParse(AbcParseRequest),
    AbcValidate(AbcValidateRequest),
    AbcTranspose(AbcTransposeRequest),
    AbcToMidi(AbcToMidiRequest),

    // === Garden ===
    GardenStatus,
    GardenPlay,
    GardenPause,
    GardenStop,
    GardenSeek(GardenSeekRequest),
    GardenSetTempo(GardenSetTempoRequest),
    GardenGetRegions(GardenGetRegionsRequest),
    GardenCreateRegion(GardenCreateRegionRequest),
    GardenDeleteRegion(GardenDeleteRegionRequest),

    // === Jobs ===
    JobStatus(JobStatusRequest),
    JobList(JobListRequest),
    JobPoll(JobPollRequest),
    JobCancel(JobCancelRequest),

    // === Graph ===
    GraphBind(GraphBindRequest),
    GraphTag(GraphTagRequest),
    GraphConnect(GraphConnectRequest),
    GraphFind(GraphFindRequest),
    GraphQuery(GraphQueryRequest),
    GraphContext(GraphContextRequest),

    // === Config ===
    ConfigGet(ConfigGetRequest),

    // === Annotations ===
    AddAnnotation(AddAnnotationRequest),
}
```

### 1.2 ToolResponse enum (already started)

The `ToolResponse` enum in `responses.rs` - extend to cover all tools.

### 1.3 Request/Response pairing

Each request type knows its response type:

```rust
pub trait Tool {
    type Request: Serialize + DeserializeOwned;
    type Response: Serialize + DeserializeOwned;

    const NAME: &'static str;
    const TIMING: ToolTiming;
}

// Example implementation
pub struct CasStoreTool;
impl Tool for CasStoreTool {
    type Request = CasStoreRequest;
    type Response = CasStoredResponse;
    const NAME: &'static str = "cas_store";
    const TIMING: ToolTiming = ToolTiming::AsyncShort;
}
```

## Phase 2: Clean Wire Protocol

### 2.1 HOOT01 frame structure (already done)

Keep the existing frame format - it's good:

```
Frame 0: "HOOT01"
Frame 1: Command (Request/Reply/Heartbeat/etc)
Frame 2: ContentType (MsgPack/RawBinary)
Frame 3: Request ID (UUID)
Frame 4: Service name
Frame 5: Traceparent
Frame 6: Body (MsgPack-encoded ToolRequest or ToolResponse)
```

### 2.2 Request envelope

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub request: ToolRequest,
    pub timing_hint: Option<ToolTiming>,  // Client can request specific handling
}
```

### 2.3 Response envelope

```rust
#[derive(Debug, Serialize, Deserialize)]
pub enum ResponseEnvelope {
    /// Immediate success with typed response
    Success(ToolResponse),

    /// Job started, poll for result
    JobStarted {
        job_id: String,
        tool: String,
        expected_timing: ToolTiming,
    },

    /// Error occurred
    Error {
        code: String,
        message: String,
        details: Option<ToolResponse>,  // Typed error details if available
    },
}
```

## Phase 3: Dispatcher Refactor

### 3.1 Typed dispatcher

```rust
// crates/hootenanny/src/api/dispatcher.rs

pub struct Dispatcher {
    server: Arc<EventDualityServer>,
}

impl Dispatcher {
    /// Main dispatch entry point - fully typed, no JSON
    pub async fn dispatch(&self, request: ToolRequest) -> ResponseEnvelope {
        let timing = request.timing();

        match timing {
            ToolTiming::Sync => self.dispatch_sync(request).await,
            ToolTiming::AsyncShort | ToolTiming::AsyncMedium => {
                self.dispatch_async_with_job(request).await
            }
            ToolTiming::AsyncLong => self.dispatch_async_return_job_id(request).await,
            ToolTiming::FireAndForget => self.dispatch_fire_and_forget(request).await,
        }
    }

    async fn dispatch_sync(&self, request: ToolRequest) -> ResponseEnvelope {
        match request {
            ToolRequest::AbcParse(req) => {
                match self.server.abc_parse_typed(req).await {
                    Ok(resp) => ResponseEnvelope::Success(ToolResponse::AbcParsed(resp)),
                    Err(e) => ResponseEnvelope::Error {
                        code: e.code,
                        message: e.message,
                        details: None,
                    },
                }
            }
            // ... other sync tools
        }
    }
}
```

### 3.2 Tool implementations return typed responses

```rust
// In EventDualityServer
impl EventDualityServer {
    /// Typed ABC parse - no JSON involved
    pub async fn abc_parse_typed(&self, request: AbcParseRequest) -> Result<AbcParsedResponse, ToolError> {
        match abc::parse(&request.abc) {
            Ok(tune) => Ok(AbcParsedResponse {
                valid: true,
                title: tune.title,
                key: tune.key.map(|k| format!("{:?}", k)),
                meter: tune.meter.map(|m| format!("{}/{}", m.numerator, m.denominator)),
                tempo: tune.tempo,
                notes_count: tune.body.len(),
            }),
            Err(e) => Ok(AbcParsedResponse {
                valid: false,
                title: None,
                key: None,
                meter: None,
                tempo: None,
                notes_count: 0,
            }),
        }
    }
}
```

## Phase 4: Holler MCP Adapter

### 4.1 Schema generation from types

Generate MCP JSON schemas from Rust types:

```rust
// In holler
fn generate_mcp_tool_schema<T: Tool>() -> McpToolSchema {
    McpToolSchema {
        name: T::NAME.to_string(),
        description: T::DESCRIPTION.to_string(),
        input_schema: schemars::schema_for!(T::Request),
    }
}
```

### 4.2 JSON ↔ Type conversion at edge

```rust
// In holler MCP handler
async fn handle_mcp_tool_call(name: &str, args: Value) -> McpResult {
    // Convert JSON to typed request
    let request = json_to_tool_request(name, args)?;

    // Send to hootenanny via ZMQ (MsgPack)
    let response = zmq_client.call(request).await?;

    // Convert typed response back to JSON for MCP
    let json = response.to_json();

    Ok(McpToolResult { content: json })
}

fn json_to_tool_request(name: &str, args: Value) -> Result<ToolRequest, McpError> {
    match name {
        "abc_parse" => {
            let req: AbcParseRequest = serde_json::from_value(args)?;
            Ok(ToolRequest::AbcParse(req))
        }
        // ... other tools
    }
}
```

### 4.3 Async handling for MCP

```rust
// Holler handles async tools transparently for MCP clients
async fn handle_async_tool(request: ToolRequest) -> McpResult {
    let response = zmq_client.call(request).await?;

    match response {
        ResponseEnvelope::JobStarted { job_id, expected_timing, .. } => {
            // Poll based on timing class
            match expected_timing {
                ToolTiming::AsyncShort => poll_with_timeout(&job_id, 30).await,
                ToolTiming::AsyncMedium => poll_with_timeout(&job_id, 120).await,
                ToolTiming::AsyncLong => {
                    // Return job_id, let MCP client manage
                    Ok(json!({ "job_id": job_id, "status": "started" }))
                }
            }
        }
        ResponseEnvelope::Success(resp) => Ok(resp.to_json()),
        ResponseEnvelope::Error { code, message, .. } => Err(McpError { code, message }),
    }
}
```

## Phase 5: Migration Path

### Step 1: Add new types alongside existing (Week 1)
- [x] Create `ToolResponse` enum in hooteproto
- [x] Add `ToolTiming` classification
- [ ] Create `ToolRequest` enum
- [ ] Keep existing `Payload` working

### Step 2: Typed dispatcher (Week 2)
- [ ] Create new `Dispatcher` that uses `ToolRequest`/`ToolResponse`
- [ ] Migrate tools one category at a time:
  - [ ] ABC tools (simplest, sync)
  - [ ] Garden tools (fire-and-forget)
  - [ ] CAS tools (sync with IO)
  - [ ] Job tools (sync)
  - [ ] Orpheus tools (async)
  - [ ] Graph tools (sync)

### Step 3: Update ZMQ server (Week 3)
- [ ] HooteprotoServer uses new dispatcher
- [ ] Returns `ResponseEnvelope` in HOOT01 frames
- [ ] Old `Payload::Success/Error` deprecated

### Step 4: Holler adapter (Week 4)
- [ ] JSON ↔ Type conversion at MCP boundary
- [ ] Async polling logic moved to Holler
- [ ] Schema generation from types

### Step 5: Cleanup (Week 5)
- [ ] Remove `Payload::Success { result: serde_json::Value }`
- [ ] Remove JSON from dispatch internals
- [ ] Update documentation

## File Changes Summary

### New files
- `crates/hooteproto/src/request.rs` - ToolRequest enum
- `crates/hooteproto/src/tool.rs` - Tool trait and registry
- `crates/hootenanny/src/api/dispatcher.rs` - New typed dispatcher
- `crates/holler/src/mcp_adapter.rs` - JSON ↔ Type conversion

### Modified files
- `crates/hooteproto/src/responses.rs` - Complete all response types
- `crates/hooteproto/src/lib.rs` - Export new types
- `crates/hootenanny/src/zmq/hooteproto_server.rs` - Use new dispatcher
- `crates/hootenanny/src/api/service.rs` - Add typed tool methods

### Deprecated (eventually removed)
- `crates/hootenanny/src/api/dispatch.rs` - Old JSON dispatch
- `Payload::Success { result: serde_json::Value }`
- `payload_to_tool_args()` conversion functions

## Success Criteria

1. **No JSON in hot path** - MsgPack ZMQ → typed dispatch → MsgPack ZMQ
2. **Type safety** - Compiler catches request/response mismatches
3. **Clear timing** - Every tool has explicit timing classification
4. **MCP compatibility** - Holler translates, external behavior unchanged
5. **Chaosgarden happy** - Fire-and-forget + IOPub still works

## Decisions Made

1. **Streaming** - Design now, implement later. See Streaming Design below.
2. **Binary payloads** - Keep `ContentType::RawBinary` for large binary (MIDI, audio, etc.)
3. **Error types** - Typed errors per tool category
4. **Versioning** - Breaking changes OK. All callers in this repo. Fix it right.

## Streaming Design (Future Implementation)

For tools that produce incremental output (musicgen audio chunks, long generation progress):

### Response Envelope Extension

```rust
pub enum ResponseEnvelope {
    Success(ToolResponse),
    JobStarted { job_id: String, tool: String, expected_timing: ToolTiming },
    Error(ToolError),

    // Streaming variants (future)
    StreamStart { stream_id: String, tool: String },
    StreamChunk { stream_id: String, chunk: StreamChunk },
    StreamEnd { stream_id: String, final_response: Option<ToolResponse> },
}

pub enum StreamChunk {
    Progress { percent: f32, message: String },
    PartialAudio { samples: Vec<f32>, sample_rate: u32 },
    PartialMidi { events: Vec<MidiEvent> },
    LogLine { level: LogLevel, message: String },
}
```

### Wire Protocol

Use multiple HOOT01 Reply frames with same request_id:

```
Request:  [HOOT01][Request][MsgPack][uuid-123][musicgen][...][body]
Reply 1:  [HOOT01][Reply][MsgPack][uuid-123][][...][StreamStart{...}]
Reply 2:  [HOOT01][Reply][MsgPack][uuid-123][][...][StreamChunk{progress:10%}]
Reply 3:  [HOOT01][Reply][MsgPack][uuid-123][][...][StreamChunk{audio:[...]}]
...
Reply N:  [HOOT01][Reply][MsgPack][uuid-123][][...][StreamEnd{final:...}]
```

### IOPub for Broadcast

Streaming chunks also published to IOPub for subscribers (chaosgarden):

```rust
pub enum IOPubEvent {
    JobStateChanged { job_id: String, state: JobState },
    StreamChunk { stream_id: String, chunk: StreamChunk },
    // ... existing events
}
```

This design slots in cleanly - callers that don't want streaming just wait for StreamEnd or use job polling.

## Decisions Made (2024-12-16)

### Fire-and-Forget → Job Model

**Decision:** Commands that have outcomes (success/failure) should be jobs, not fire-and-forget.

**Rationale:**
- Fire-and-forget commands DO have outcomes - we were just ignoring failures
- `garden_create_region` returns a `region_id` - that's a result, not fire-and-forget
- Jobs provide error visibility, unified monitoring, and consistent model
- Job overhead is negligible (in-memory HashMap insert)

**New Model:**
- **Pure queries (no job):** `garden_status`, `config_get`, `job_list`, `abc_parse`
- **Commands with outcomes (job):** `garden_play`, `garden_create_region`, `orpheus_generate`

**Timing Classification Updated:**
- `Sync` - No job, immediate typed response
- `AsyncShort/Medium/Long` - Job created, varying poll expectations
- `FireAndForget` - Job created, Ack with job_id returned immediately, client doesn't poll but can query failures

### Job Ownership Model

**Decision:** Hootenanny owns job lifecycle, chaosgarden treats job_id as opaque.

**Flow:**
```
1. Client → Hootenanny: garden_play
2. Hootenanny: Creates job (job_id = "job_xxx")
3. Hootenanny → Chaosgarden: { job_id: "job_xxx", command: "play" }
4. Chaosgarden: Executes, treats job_id as opaque
5. Chaosgarden → Hootenanny: { job_id: "job_xxx", status: "ok" }
                         or: { job_id: "job_xxx", status: "error", message: "..." }
6. Hootenanny: Updates job status
7. Hootenanny → Client: Ack with job_id (or error if immediate failure)
```

**Benefits:**
- Single source of truth for job state (hootenanny)
- Chaosgarden stays simple (just executes and reports)
- Failures are recorded and queryable via `job_list(status: "failed")`
- Jobs can have TTL (e.g., 60s for transport commands)

### What Needs a Job?

**Rule:** If we're tracking something or expecting an outcome beyond "request received", it's a job.

| Tool | Job? | Reason |
|------|------|--------|
| `garden_status` | No | Pure query, no side effects |
| `garden_play` | Yes | Can fail (no regions, daemon down) |
| `garden_create_region` | Yes | Returns region_id, can fail |
| `config_get` | No | Pure query |
| `abc_parse` | No | Stateless transform, errors returned inline |
| `orpheus_generate` | Yes | Long-running, has result |
| `job_status` | No | Meta-query about jobs |

## Session Notes (2024-12-16)

### Completed
- Created `crates/hooteproto/src/request.rs` - Full `ToolRequest` enum with all tools
- Created `crates/hooteproto/src/responses.rs` - `ToolResponse` enum with typed responses
- Created `crates/hooteproto/src/envelope.rs` - `ResponseEnvelope` + typed `ToolError` categories
- Created `crates/hooteproto/src/timing.rs` - `ToolTiming` enum with timing classification
- Updated `crates/hooteproto/src/lib.rs` - Exports new modules
- Added streaming design to plan (design only, not implemented)
- Fixed ToolError API migration (~10 files)
- Created `TypedDispatcher` in `crates/hootenanny/src/api/typed_dispatcher.rs`
- Created typed methods in `crates/hootenanny/src/api/service_typed.rs`
- Added `payload_to_request()` and `envelope_to_payload()` conversions
- Wired TypedDispatcher into ZMQ server (hybrid: tries typed first, falls back to JSON)
- All tests passing (53 hootenanny + 39 hooteproto)

### Typed Tools (Using New Path)
- ABC: `abc_parse`, `abc_validate`, `abc_transpose`
- SoundFont: `soundfont_inspect`, `soundfont_preset_inspect`
- Garden: `garden_status`, `garden_play/pause/stop/seek/set_tempo/emergency_pause`, `garden_get_regions`, `garden_create/delete/move_region`
- Jobs: `job_status`, `job_list`
- Config: `config_get`
- Admin: `ping`, `list_tools`

### Still on JSON Path
- Orpheus tools, Audio conversion, Graph tools, CAS tools

### Next Steps
1. Implement job creation for FireAndForget commands
2. Update chaosgarden protocol to include job_id in requests/responses
3. Add job TTL for short-lived jobs
4. Migrate remaining tools to typed path

### Files to Clean Up
- `crates/hootenanny/src/api/typed_dispatch.rs` - Can be deleted, was intermediate approach
- Old `Payload` enum in `lib.rs` - Keep for now, deprecate after full migration
