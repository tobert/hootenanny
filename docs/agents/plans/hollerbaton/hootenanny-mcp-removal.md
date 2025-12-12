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

**Solution options:**
1. Align hooteproto types with schema types
2. Create adapter functions for each tool
3. Bypass schema types and call LocalModels directly from ZMQ dispatch

## Alternative: Keep Both

Current state: hootenanny has both MCP and ZMQ servers.
- MCP: Full tool suite via baton
- ZMQ: CAS + artifacts only

For now, tools only available via MCP. Holler's tool_to_payload has all routing ready, but hootenanny's ZMQ server returns "not_implemented" for most tools.
