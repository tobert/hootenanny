# Plan: Remove MCP Server from Hootenanny

## Status: In Progress (Partial)

**Completed:**
- hooteproto Payload expanded with all tool variants
- holler tool_to_payload handles all tools
- ZMQ server returns "not_implemented" for unhandled tools

**Remaining:**
- Full ZMQ dispatch implementation
- Remove baton/MCP from hootenanny

This is follow-up work to the holler→baton migration. The goal is to make holler the single MCP entry point, with hootenanny as a pure ZMQ backend.

## Current State

Hootenanny has both:
1. **ZMQ server** (`src/zmq/hooteproto_server.rs`) - Handles basic CAS/artifact operations
2. **MCP server** (`src/api/handler.rs`) - Full tool suite via baton

The MCP server has ~40 tools with complex implementations including:
- Orpheus MIDI generation (async jobs)
- MusicGen/YuE audio generation
- ABC notation parsing/conversion
- BeatThis/CLAP analysis
- Trustfall graph queries
- Chaosgarden proxy
- Job system with polling

## Work Required

### 1. Expand ZMQ Server with All Services

The `HooteprotoServer` needs access to all the services currently used by `HootHandler`:

```rust
pub struct HooteprotoServer {
    bind_address: String,
    cas: Arc<FileStore>,
    artifacts: Arc<RwLock<artifact_store::FileStore>>,
    // NEW - add these:
    local_models: Arc<LocalModels>,
    job_store: Arc<JobStore>,
    graph_adapter: Arc<AudioGraphAdapter>,
    garden_manager: Option<Arc<GardenManager>>,
    gpu_monitor: Arc<GpuMonitor>,
    start_time: Instant,
}
```

### 2. Implement Tool Dispatch in ZMQ Server

Move tool implementations from `api/handler.rs` to a shared module that both ZMQ and MCP can use, or directly into the ZMQ dispatch:

```rust
async fn dispatch(&self, payload: Payload) -> Payload {
    match payload {
        // Existing
        Payload::CasStore { data, mime_type } => self.cas_store(data, mime_type).await,

        // NEW - add all these:
        Payload::OrpheusGenerate { temperature, top_p, cfg_coef, seed } => {
            self.orpheus_generate(temperature, top_p, cfg_coef, seed).await
        }
        Payload::ConvertMidiToWav { midi_hash, soundfont_hash, sample_rate } => {
            self.convert_midi_to_wav(midi_hash, soundfont_hash, sample_rate).await
        }
        // ... 30+ more tools
    }
}
```

### 3. Update main.rs

Remove MCP-related initialization:
- Remove `baton::McpState` creation
- Remove `baton::dual_router`
- Remove MCP session cleanup task
- Remove `CompositeHandler`, `HootHandler`, `AgentChatHandler`
- Keep `EventDualityServer` or refactor services into ZMQ server directly

### 4. Files to Delete

After migration:
- `src/api/handler.rs` - MCP handler (move logic to ZMQ)
- `src/api/composite.rs` - Composite handler wrapper
- `src/api/schema.rs` - Request/response types (may keep for ZMQ)
- `src/api/mod.rs`
- `tests/mcp_integration.rs` - MCP-specific tests

### 5. Files to Keep

- `src/api/service.rs` - `EventDualityServer` has shared state
- `src/api/responses.rs` - Response types used by tools
- `src/api/tools/*` - Individual tool implementations
- `src/mcp_tools/` - LocalModels, RustySynth (used by tools)

### 6. Dependencies to Remove

From `Cargo.toml`:
```toml
baton = { path = "../baton" }  # Remove
```

## Complexity Estimate

This is a **medium-large refactor**:
- ~2000 lines of tool dispatch code to restructure
- Async job handling needs to work over ZMQ
- Progress reporting currently uses baton's `ToolContext`
- Tests need rewriting for ZMQ instead of MCP

## Key Challenge: Type Mismatch

The main issue is that `hooteproto::Payload` types don't match `api::schema` request types:
- hooteproto uses `f64` for temperatures, schema uses `f32`
- Field names differ (e.g., `cfg_coef` vs none)
- Schema types have additional fields (variation_set_id, mime_type, etc.)

**Chosen approach: Align hooteproto to schema types**

The cleanest path forward is to update `hooteproto::Payload` variants to match the `api::schema` request types exactly. This means:

1. Update field types in hooteproto (f64 → f32 where needed)
2. Add missing fields to Payload variants (variation_set_id, mime_type, etc.)
3. Remove fields that don't exist in schema (cfg_coef)
4. Update holler's `tool_to_payload` to match

This ensures a single source of truth for request types and makes ZMQ dispatch trivial - just convert Payload to schema type and call the existing EventDualityServer methods.

## Alternative: Keep Both

Current state: hootenanny has both MCP and ZMQ servers.
- MCP: Full tool suite via baton
- ZMQ: Full tool suite via EventDualityServer (since 2025-12-12)

## Implementation Progress (2025-12-12)

### Completed

1. **Aligned hooteproto Payload types with api/schema request types**
   - Updated field types (f64 → f32 for temperatures)
   - Added artifact tracking fields (variation_set_id, parent_id, tags, creator)
   - Updated field names to match schema (midi_hash → input_hash, from_hash → section_a_hash)
   - Added `GraphHint` struct matching schema's hint structure
   - Made `mime_type` required where schema requires it

2. **Added EventDualityServer to HooteprotoServer**
   - Created `with_event_server()` constructor
   - Server can now operate in two modes:
     - Standalone: CAS + artifacts only (legacy)
     - Full: All tools via EventDualityServer

3. **Implemented dispatch via HootHandler**
   - `dispatch_via_server()` converts hooteproto Payload to JSON args
   - Calls existing `HootHandler.call_tool()` method
   - Converts baton `CallToolResult` back to hooteproto `Payload`

4. **Updated holler's tool_to_payload**
   - All Orpheus tools now pass new fields (model, max_tokens, etc.)
   - Graph tools use new schema field names
   - Added `extract_string_array` helper for tags

5. **Updated main.rs**
   - Creates EventDualityServer before ZMQ server
   - Passes EventDualityServer to HooteprotoServer::with_event_server()

### What This Enables

With these changes, holler can route ALL tools through hootenanny's ZMQ server:
- `holler → hooteproto → hootenanny ZMQ → EventDualityServer → tool handlers`

This paves the way for full MCP removal from hootenanny when ready.

### Completed (2025-12-12 - Full MCP Removal)

MCP/baton removed from hootenanny main.rs and public API:

1. ✅ Removed `api/handler.rs` (baton Handler impl)
2. ✅ Removed `api/composite.rs`
3. ✅ Removed MCP routes from main.rs
4. ✅ Removed llm-mcp-bridge dependency (was MCP-based)
5. ✅ Created `api/dispatch.rs` for ZMQ tool dispatch
6. ✅ Cleaned up unused imports

**Note**: baton is still a dependency because `EventDualityServer` methods return `baton::CallToolResult`. The `dispatch.rs` module bridges this by converting to/from JSON. A future refactor could replace these return types with custom types to fully remove baton.

### Architecture After Removal

```
holler (MCP gateway)
    ├── baton MCP server (for Claude/clients)
    └── hooteproto ZMQ client
           ↓
hootenanny (ZMQ backend)
    ├── ZMQ ROUTER (hooteproto)
    ├── dispatch.rs → EventDualityServer
    └── HTTP (artifacts, health only)
```

Hootenanny is now a pure ZMQ backend service. MCP clients connect through holler.
