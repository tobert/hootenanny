# Plan: Luanette - Lua Scripting MCP Server

## Implementation Status

### Phase Checklist
- [x] Phase 1: Minimal MCP Server Scaffold (~2-3 hours) ✅
- [x] Phase 2: Upstream Client Manager (~4-5 hours) ✅
- [x] Phase 3: Lua Runtime with OpenTelemetry (~5-6 hours) ✅
- [ ] Phase 4: Tool Bridge with Traceparent Propagation (~6-7 hours)
- [ ] Phase 5: Music Standard Library (~4-5 hours)
- [ ] Phase 6: Job System (~3-4 hours)
- [ ] Phase 7: Discovery & Describe (~2-3 hours)
- [ ] Phase 8: Error Handling (~2-3 hours)
- [ ] Phase 9: Integration Testing & Examples (~3-4 hours)

### Session Sign-offs
<!--
Add notes here when signing off from a session.
Format: YYYY-MM-DD - Phase N progress: Brief summary of what was completed
-->

2024-12-06 - Phase 1 complete:
- Created crates/luanette with Cargo.toml, main.rs, handler.rs, runtime.rs, schema.rs
- Implemented sandboxed Lua runtime with mlua 0.10 (lua54 feature)
- Exposed lua_eval and lua_describe tools via baton::Handler
- Server runs on port 8081, responds to MCP initialize
- 11 unit tests passing (runtime + handler)
- Tested with hrcli: `lua_eval --code 'return 2 + 2'` returns `{ result: 4, duration_ms: 0 }`
- Sandbox blocks: os.execute, io.*, debug.*, dofile, loadfile
- Allows: math.*, string.*, table.*, os.clock/date/time/getenv, log.* (tracing)

2024-12-06 - Detour: baton::client module (MCP client consolidation):
- Created baton::client module with McpClient (Streamable HTTP) and SseClient (SSE transport)
- Added "client" feature flag to baton Cargo.toml
- Both transports support: initialize, list_tools, call_tool, complete_argument
- Traceparent propagation for distributed tracing
- Migrated luanette to use baton::client::McpClient
- Migrated llm-mcp-bridge to use baton::client::McpClient (lazy init to avoid circular deps)
- Migrated hrcli to use baton::client::SseClient (thin wrapper with CLI-specific ToolInfo)
- Reduced mcp_client.rs from 589 lines to 120 lines in hrcli
- TODO: Revisit AgentManager lazy MCP client initialization design

2024-12-06 - Phase 2 complete (Upstream Client Manager):
- Added --hootenanny-url CLI argument to luanette
- Created ClientManager with namespace mapping (clients/manager.rs)
- ClientManager supports: add_upstream, call_tool, all_tools, refresh_tools
- Wired ClientManager into LuanetteHandler

2025-12-06 - Phase 2 continued (mcp.* Lua Globals):
- Created tool_bridge.rs with McpBridgeContext for async-to-sync bridging
- McpBridgeContext uses tokio::runtime::Handle::block_on() for MCP calls from Lua
- register_mcp_globals() creates mcp.* namespace tables with tool functions
- Updated LuaRuntime with with_mcp_bridge() constructor
- JSON <-> Lua table conversion with proper array/object handling
- 15 tests passing (runtime + handler + manager + tool_bridge)
- Tested live with hootenanny:
  - mcp.hootenanny.job_list({}) → returns job list
  - mcp.hootenanny.agent_chat_backends({}) → returns backend config
  - mcp.hootenanny.cas_store({content_base64="...", mime_type="text/plain"}) → stores content, returns hash
- Phase 2 complete!

2025-12-06 - Phase 3 complete (Lua Runtime with OpenTelemetry):
- Added OpenTelemetry dependencies (opentelemetry 0.28, tracing-opentelemetry 0.29)
- Created telemetry.rs with OTLP exporter for traces, logs, and metrics
- Added --otlp-endpoint CLI argument (default: 127.0.0.1:35991)
- Created otel_bridge.rs with `otel.*` Lua namespace:
  - otel.trace_id() → current trace ID (nil if outside span)
  - otel.span_id() → current span ID (nil if outside span)
  - otel.traceparent() → W3C traceparent header
  - otel.event(name, attrs?) → add event to current span
  - otel.set_attribute(key, value) → set span attribute
  - otel.record_metric(name, value, attrs?) → record metric
- Registered otel globals in sandboxed Lua VM
- 20 tests passing (runtime + handler + manager + tool_bridge + otel_bridge)
- Verified telemetry export to otlp-mcp:
  - Traces show span hierarchy (mcp.dispatch → mcp.tool.call → call_tool)
  - Logs capture script events, attributes, and metrics
- Note: trace_id/span_id return nil from spawn_blocking context (expected, Phase 4 will add span context propagation)

---

## Overview
Create a standalone MCP server called `luanette` that acts as a **programmable control plane** and **glue layer** for the MCP ecosystem. It allows AI agents to compose, transform, and automate tools from multiple upstream MCP servers (initially Hootenanny) using Lua scripts. Scripts are CAS-addressed artifacts, enabling the creation of high-level "Meta-Tools" without recompilation.

## Architecture

### The "Spider" Model
`luanette` acts as a central hub that connects to multiple upstream MCP servers.
- **Upstreams**: Configurable connections to other MCP servers (Hootenanny, Filesystem, Git, etc.).
- **Namespacing**: Upstream tools are exposed in Lua under `mcp.<namespace>.<tool>` (e.g., `mcp.hootenanny.orpheus_generate`).
- **Glue & Facades**: Lua scripts can wrap, combine, or mock upstream tools. `luanette` exports these scripts as new MCP tools, allowing for rapid API iteration and logic encapsulation.

### New Crate: `crates/luanette`
Standalone MCP server following the `baton` Handler pattern.

**Key Design Decisions:**
- **Async model**: `mlua` + `tokio` (blocking pool + hooks for safety).
- **Job-based**: All script execution returns job IDs.
- **CAS integration**: Scripts stored as `text/x-lua` artifacts.
- **Sandbox**: Lua 5.4 environment + Tokio timeouts + Restricted globals.
- **Discovery**: Dynamic tool binding at startup.

### Component Structure

```
crates/luanette/
├── Cargo.toml
├── src/
│   ├── main.rs                 # Server startup, axum routing
│   ├── handler.rs              # MCP Handler implementation
│   ├── service.rs              # Business logic orchestration
│   ├── runtime/
│   │   ├── mod.rs              # LuaRuntime core
│   │   ├── stdlib.rs           # Standard library exposure
│   │   ├── tools.rs            # Dynamic MCP tool registration
│   │   └── sandbox.rs          # Sandboxing config
│   ├── types.rs                # Domain types (ScriptId, ExecutionResult)
│   └── client.rs               # Hootenanny MCP client
```

## Detailed Implementation Design

### Crate Structure
```
crates/luanette/
├── Cargo.toml
├── src/
│   ├── main.rs                    # Server startup, CLI args, OTLP init
│   ├── handler.rs                 # MCP Handler implementation (~250 lines)
│   ├── runtime.rs                 # Lua VM lifecycle + sandbox (~350 lines)
│   ├── stdlib.rs                  # Lua standard library (~200 lines)
│   ├── otel_bridge.rs             # OpenTelemetry Lua namespace (~150 lines)
│   ├── tool_bridge.rs             # Hootenanny MCP client + Lua bindings (~250 lines)
│   ├── job_system.rs              # Background job execution (~120 lines)
│   ├── schema.rs                  # Request/response types with schemars
│   └── error.rs                   # Lua error formatting for AI (~80 lines)
└── tests/
    ├── integration.rs             # End-to-end script execution tests
    └── fixtures/
        ├── hello.lua              # Basic script
        ├── tool_call.lua          # Call orpheus_generate
        ├── otel_example.lua       # OpenTelemetry features
        └── error_example.lua      # Stack trace validation
```

### Key Components

#### 1. Lua Runtime (`runtime.rs`)
- Initialize Lua VM with `lua54` feature
- Sandbox configuration:
  - Execution limit: 30s timeout (tokio::time::timeout)
  - Memory tracking (optional)
  - Disable: `dofile`, `loadfile`, `require`, `os.execute`, `os.remove`, `os.rename`
  - Allow: `io.*` for file reading/writing (simplifies early development)
- Async-transparent execution via mlua coroutines
- Main function signature: `function main(params) ... end`
- **OpenTelemetry Integration:**
  - Every script execution creates a span: `luanette.script.execute`
  - Span attributes: `script.hash`, `script.name`, `script.creator`
  - Script parameters recorded as `script.params.*` attributes
  - Current span context stored in Lua VM scope for `otel.*` access
  - Errors captured as span events with stack traces

#### 2. Tool Bridge (`tool_bridge.rs`)
Dynamic tool discovery pattern:
```rust
pub struct ToolRegistry {
    tools: HashMap<String, ToolInfo>,
}

impl ToolRegistry {
    pub async fn discover(client: &McpToolClient) -> Result<Self> {
        // 1. Call hootenanny's tools/list
        // 2. Build map of tool_name → schema
        // 3. Create async Lua functions for each tool
    }
}
```

Type conversion (critical):
- Lua tables ↔ JSON objects/arrays
- Handle mixed keys (array + object in same table)
- Preserve nulls (Lua nil → JSON null)

**Traceparent Propagation:**
- Every MCP tool call automatically includes W3C `traceparent` header
- Extract from current span context in Lua VM
- Format: `00-{trace_id}-{span_id}-{flags}`
- Propagated via HTTP headers to upstream MCP servers
- Scripts can access via `otel.traceparent()` for manual propagation

#### 3. Standard Library (`stdlib.rs`)
Exposed Lua globals:
- `log.info(msg)`, `log.error(msg)`, `log.warn(msg)`, `log.debug(msg)` (includes trace context)
- `math.*` (full library)
- `string.*` (full library)
- `table.*` (ipairs, pairs, insert, concat)
- `clock.now()` - Returns virtual timestamp (u64)
- `clock.advance(ms)` - Advances virtual time for testing

#### 3a. OpenTelemetry Bridge (`otel_bridge.rs`)
Exposed `otel.*` namespace for observability:
- `otel.trace_id()` - Returns current trace ID as hex string (32 chars)
- `otel.span_id()` - Returns current span ID as hex string (16 chars)
- `otel.traceparent()` - Returns W3C traceparent header value
- `otel.event(name, attributes?)` - Add event to current span
  - Example: `otel.event("midi_processed", {notes = 128, bars = 8})`
- `otel.set_attribute(key, value)` - Set attribute on current span
  - Example: `otel.set_attribute("processing.style", "jazz")`
- `otel.record_metric(name, value, attributes?)` - Record a metric
  - Example: `otel.record_metric("notes.generated", 256, {instrument = "piano"})`

**Integration with log.*:**
- All `log.*` calls automatically include trace context
- Emitted as OpenTelemetry log records with trace_id/span_id
- Severity levels mapped: debug=DEBUG, info=INFO, warn=WARN, error=ERROR

#### 4. Job System (`job_system.rs`)
Copy hootenanny's pattern verbatim:
- JobStore: `HashMap<String, JobInfo>` wrapped in `Arc<Mutex<>>`
- States: Pending → Running → Complete/Failed/Cancelled
- Background execution: `tokio::spawn()` with JoinHandle tracking
- Cancellation support via `handle.abort()`

#### 5. Error Formatting (`error.rs`)
AI-optimized error messages:
```
Lua Runtime Error in script cas://abc123def456

Error: attempt to call a nil value (field 'orpheus_generate')
Stack trace:
  [Lua] script.lua:15: in function 'main'

Troubleshooting:
- Tool name typo? Available tools: orpheus_generate, cas_store, ...
- Check parameter types match tool schema
- Use lua_describe to introspect script
```

### Tools Exposed by Luanette

**Note:** Luanette does *not* automatically re-export upstream tools. It exposes its own engine tools, and users create "Proxy Scripts" to expose specific upstream functionality or workflows.

1. **`lua_execute`** - Main execution tool
   - Input: `{ script_hash, params, creator?, tags? }`
   - Output: `{ job_id }`
   - Fetches script from hootenanny CAS
   - Spawns background job

2. **`lua_eval`** - Quick one-off execution (Use with caution)
   - Input: `{ code: string }`
   - Output: `{ result }`
   - Useful for debugging or quick "glue" without saving artifacts

3. **`lua_describe`** - Introspection
   - Input: `{ script_hash }`
   - Output: Schema from `describe()` function

4. **`script_search`** - Discovery
   - Input: `{ tag?, vibe? }`
   - Filters for `tag = "type:lua"`

6. **Upstream Management**
   - `mcp_connect`: Dynamically connect to a new upstream MCP
     - Input: `{ namespace: "git", url: "http://localhost:8090" }`
   - `mcp_disconnect`: Remove an upstream connection
   - `mcp_list_upstreams`: List connected namespaces

## Implementation Plan

### Phase 1: Minimal MCP Server Scaffold (~2-3 hours)
**Goal**: Server responds to MCP initialize

**Tasks:**
1. **Create crate structure**
   - `cargo new --lib crates/luanette`
   - Add to workspace `Cargo.toml`
   - Add dependencies:
     - `mlua = { version = "0.10", features = ["lua54", "async", "send", "serialize"] }`
     - `baton = { path = "../baton" }`
     - `axum`, `tokio`, `serde`, `schemars`, `anyhow`, `tracing`

2. **Implement minimal Handler**
   - Define `LuanetteHandler` struct
   - Implement `baton::Handler` trait
   - Initial tools: `lua_execute`, `script_describe`
   - Return placeholder responses (no Lua yet)

3. **Server startup in main.rs**
   - Copy pattern from `hootenanny/src/main.rs`
   - Initialize baton McpState
   - Create axum router with `baton::dual_router()`
   - Run server on port 8081 (avoid hootenanny's 8080)

**Critical Files**:
- `crates/luanette/Cargo.toml` (new)
- `crates/luanette/src/main.rs` (new)
- `crates/luanette/src/handler.rs` (new)
- `crates/luanette/src/schema.rs` (new)
- `Cargo.toml` (workspace root - add member)

**Validation**: `curl http://localhost:8081/mcp` returns MCP protocol response

### Phase 2: Upstream Client Manager (~4-5 hours)
**Goal**: Connect to upstream MCP servers (Hootenanny) and manage tool namespaces

**Tasks:**
1. **Define `ClientManager` struct**
   - Map of `namespace -> McpToolClient`
   - Support for multiple connections (prepared for multi-MCP future)
   - Config loading (hardcoded `hootenanny` for MVP, extensible later via `luanette.toml`)

2. **Implement `McpToolClient`**
   - Reference: `llm-mcp-bridge/src/mcp_client.rs`
   - Implement: `initialize()`, `list_tools()`, `call_tool()`
   - **Note on Communication**: This client uses HTTP (JSON requests/responses) and HTTP polling for job status, assuming upstream MCPs provide this standard interface.

3. **Tool Discovery & Namespacing**
   - Connect to `http://localhost:8080/mcp` as `hootenanny` namespace
   - Fetch `tools/list`
   - Store tool definitions mapped by `namespace.tool_name`
   - Log discovery: "Registered mcp.hootenanny.orpheus_generate", etc.

4. **Dynamic Management**
   - Implement `add_client(namespace, url)` and `remove_client(namespace)` on Manager
   - Ensure thread safety (RwLock) for runtime updates
   - (Optional) Persist config to `luanette.toml` on change

**Critical Files**:
- `crates/luanette/src/clients/mod.rs` (new)
- `crates/luanette/src/clients/manager.rs` (new)
- `crates/luanette/src/clients/mcp_client.rs` (new)
- `crates/luanette/src/main.rs` (update)

**Validation**: Startup logs show "Connected to [hootenanny]. Discovered 30 tools."

### Phase 3: Lua Runtime with OpenTelemetry (~5-6 hours)
**Goal**: Execute basic Lua scripts with automatic tracing and observability

**Tasks:**
1. **Initialize OpenTelemetry in main.rs**
   - Set up OTLP exporter (read endpoint from env `OTEL_EXPORTER_OTLP_ENDPOINT`)
   - Configure tracer provider with service name `luanette`
   - Configure logger provider for structured logs
   - Configure meter provider for metrics
   - Install global providers

2. **Create LuaRuntime with span context**
   - Initialize Lua VM with Lua 5.4
   - Configure sandbox (restricted environment)
   - Implement `execute_script(source: &str, params: Value, span_context: SpanContext) -> Result<Value>`
   - Store span context in Lua registry for `otel.*` access

3. **Script execution pattern with tracing**
   - Create span: `luanette.script.execute`
   - Add span attributes:
     - `script.hash` - CAS hash of script
     - `script.name` - From describe() if available
     - `script.creator` - Creator metadata
     - `script.params.*` - Flattened parameter keys
   - Wrap in `tokio::time::timeout(Duration::from_secs(30), ...)`
   - Use `tokio::task::spawn_blocking` for Lua execution
   - Load source with `lua.load(source).exec()`
   - Extract `main` function from globals
   - Convert params to Lua table (`lua.to_value()`)
   - Call: `main.call(lua_params)`
   - Convert result back to JSON
   - On error: record span event with stack trace

4. **Implement otel_bridge.rs**
   - Create `otel.*` Lua namespace
   - `otel.trace_id()` - Extract from span context in registry
   - `otel.span_id()` - Extract from span context in registry
   - `otel.traceparent()` - Format W3C traceparent header
   - `otel.event(name, attrs?)` - Add span event
   - `otel.set_attribute(key, value)` - Set span attribute
   - `otel.record_metric(name, value, attrs?)` - Record metric

5. **Integrate logging with trace context**
   - Modify `log.*` functions in stdlib.rs
   - Emit OpenTelemetry log records instead of plain tracing
   - Include trace_id and span_id from context
   - Map severity levels appropriately

6. **Error handling with observability**
   - Wrap execution in Result
   - Capture Lua stack traces
   - Record as span events on error
   - Format errors for AI agents (see `error.rs` design)

**Critical Files**:
- `crates/luanette/src/main.rs` (update - OTLP init)
- `crates/luanette/src/runtime.rs` (new - with span context)
- `crates/luanette/src/otel_bridge.rs` (new - otel.* namespace)
- `crates/luanette/src/stdlib.rs` (new - log.* with trace context)
- `crates/luanette/src/error.rs` (new)

**Validation**:
- Unit test executes hello.lua and returns "Hello, Hootenanny!"
- Traces visible in OTLP collector with script.hash attribute
- `otel.trace_id()` returns valid 32-char hex string

### Phase 4: Tool Bridge with Traceparent Propagation (~6-7 hours)
**Goal**: Lua scripts can call upstream tools with automatic distributed tracing

**Tasks:**
1. **Add W3C Trace Context to Baton (benefits entire ecosystem)**
   - Add dependencies to `crates/baton/Cargo.toml`:
     - `opentelemetry = "0.28"`
     - `opentelemetry-http = "0.28"`
   - Update `streamable_handler()` in `baton/src/transport/streamable.rs`:
     - Extract `traceparent` header from incoming requests
     - Use `opentelemetry::global::get_text_map_propagator()` to extract trace context
     - Set as parent context when creating dispatch span
   - Update `sse_endpoint()` similarly if needed
   - This enables ALL baton-based servers (hootenanny, luanette) to receive trace context

2. **Implement `mcp` Global Table**
   - Create the root `mcp` table in Lua
   - For each registered namespace (e.g., `hootenanny`), create a sub-table
   - Populate sub-tables with wrapper functions

3. **Dynamic Tool Binding with Span Creation**
   - Iterate over discovered tools from `ClientManager`
   - Generate Lua functions that:
     1. Create child span for the tool call: `luanette.mcp.{namespace}.{tool}`
     2. Add span attributes: `mcp.namespace`, `mcp.tool`, `mcp.params.*`
     3. Extract traceparent from current span context
     4. Serialize args to JSON
     5. Use blocking call to async bridge (via channels)
     6. Deserialize result back to Lua
     7. Record result/error in span
   - Bind to `mcp.hootenanny.tool_name`

4. **Traceparent Injection in HTTP Client**
   - Copy/adapt `inject_trace_context()` helper from hootenanny's `local_models.rs:86-105`
   - Apply to `McpToolClient` in `mcp_client.rs`
   - Format W3C traceparent: `00-{trace_id}-{span_id}-{flags}`
   - Flags: "01" if sampled, "00" otherwise
   - Include `traceparent` header in all MCP tool calls
   - Use `tracing_opentelemetry::OpenTelemetrySpanExt` to get current context

5. **Async Bridge Pattern**
   - Lua runs in blocking thread pool (`spawn_blocking`)
   - MCP calls use `oneshot` channels to communicate with async runtime
   - Pass span context through channels for proper nesting
   - Pattern: Lua fn sends request → async task handles → response back to Lua

6. **Type Conversion (Critical)**
   - Lua tables ↔ JSON objects/arrays
   - Handle mixed keys and nil/null correctly

7. **Integration Test with E2E Tracing**
   - Script calls `mcp.hootenanny.cas_inspect({ hash = "..." })`
   - Verify correct routing and response
   - Verify traceparent header sent to upstream
   - Verify baton extracts incoming traceparent
   - Verify full trace chain: client → luanette → hootenanny
   - Check spans nest correctly: script.execute → mcp.hootenanny.cas_inspect

**Critical Files**:
- `crates/baton/Cargo.toml` (update - add opentelemetry deps)
- `crates/baton/src/transport/streamable.rs` (update - extract traceparent)
- `crates/luanette/src/tool_bridge.rs` (new - with span creation and injection)
- `crates/luanette/src/clients/mcp_client.rs` (update - traceparent injection)
- `crates/luanette/src/runtime.rs` (update)

**Validation**:
- Integration test passes: Lua -> `mcp.hootenanny.echo` -> Rust -> Lua
- Traceparent header visible in HTTP requests to hootenanny
- Hootenanny receives and parses traceparent correctly
- Full distributed trace visible in OTLP collector
- Trace shows complete chain: client → luanette.script.execute → luanette.mcp.hootenanny.cas_inspect → hootenanny span

### Phase 5: Music Standard Library (~4-5 hours)
**Goal**: Provide high-level MIDI manipulation capabilities to Lua

**Tasks:**
1. **Add midly dependency**
   - `midly = "0.5"` for MIDI parsing/writing

2. **Define MIDI Lua Object Model**
   - Design Lua table structure for Tracks, Events, Notes
   - Example: `{ type = "note_on", channel = 0, note = 60, velocity = 100, time = 480 }`

3. **Implement `midi` module (Rust -> Lua)**
   - `midi.read(filepath)` -> parses MIDI file to Lua table
   - `midi.write(filepath, lua_table)` -> writes Lua table to MIDI file
   - Pure file I/O, no CAS integration

4. **Helper Functions**
   - `midi.merge(tracks)` - Combine multiple track tables
   - `midi.transpose(events, semitones)` - Modify note values
   - `midi.quantize(events, grid)` - Snap to grid (in ticks)

5. **Temp Directory Management**
   - Provide `temp.path(filename)` -> returns managed temp file path
   - Scripts write to temp, then call `mcp.hootenanny.cas_upload_file`
   - Temp cleanup on script completion

**Critical Files**:
- `crates/luanette/src/stdlib/midi.rs` (new)
- `crates/luanette/src/stdlib/temp.rs` (new - temp directory management)
- `crates/luanette/Cargo.toml` (update - add midly)

**Workflow Pattern**:
```lua
-- Input: cas_inspect -> local_path -> midi.read
local info = mcp.hootenanny.cas_inspect { hash = params.input_hash }
local track = midi.read(info.local_path)

-- Process
midi.transpose(track.events, 7)

-- Output: midi.write -> temp file -> cas_upload_file
local output_path = temp.path("output.mid")
midi.write(output_path, track)
local result = mcp.hootenanny.cas_upload_file {
    file_path = output_path,
    mime_type = "audio/midi"
}
```

### Phase 6: Job System (~3-4 hours)
**Goal**: Async script execution with polling

**Tasks:**
1. **Create JobStore**
   - Copy from `hootenanny/src/job_system.rs` verbatim
   - JobInfo with status, result, error, timestamps
   - Methods: `create_job`, `mark_running`, `mark_complete`, `mark_failed`

2. **Implement lua_execute tool**
    - Accept: `{ script: "cas:hash", params: {...} }`
    - Fetch script content from hootenanny CAS
    - Create job ID
    - Spawn tokio task for execution
    - Return job ID immediately

3. **Job management tools**
    - `job_status`: Get current status + result
    - `job_poll`: Wait for completion with timeout
    - `job_cancel`: Abort running script
    - `job_list`: List all jobs

**Files**: `job_system.rs`, updates to `handler.rs`

### Phase 7: Discovery & Describe (~2-3 hours)
**Goal**: Store scripts as artifacts, make them searchable

1. **script_store tool** (wrapper around hootenanny)
    - Accept: `{ content: "...", tags: [...] }`
    - Call hootenanny's `cas_store` with mime type `text/x-lua`
    - Return artifact ID

2. **lua_describe tool**
    - Accept: `{ script: "cas:hash" }`
    - Execute script in sandbox
    - Call `describe()` function if present
    - Return schema (name, description, params)

3. **script_search tool**
    - Query hootenanny's Trustfall adapter
    - Filter: `{ Artifact(tag: "type:lua") { id tags creator } }`
    - Support filters: tag, creator, vibe search
    - Return list of matching scripts

**Files**: updates to `handler.rs`, `service.rs`

### Phase 8: Error Handling (~2-3 hours)
**Goal**: Polish error reporting for AI consumption

1. **Enhanced Stack Traces**
   - Map internal Lua errors to clean stack traces
   - Hide internal runtime lines (stdlib wrappers)
   - Extract line numbers and function names

2. **Troubleshooting Hints**
   - Detect common failures (e.g., calling non-existent tool)
   - Suggest fixes in error message (e.g., "Did you mean 'orpheus_generate'?")
   - Validate parameter types against schema where possible

**Files**: `src/error.rs`, `src/runtime.rs`

### Phase 9: Integration Testing & Examples (~3-4 hours)
**Goal**: Validate the system works end-to-end

1. **Integration tests**
    - Test basic execution: "Hello World" script
    - Test MCP tool calling from Lua
    - Test error handling (syntax errors, runtime errors)
    - Test job cancellation
    - Test script storage + retrieval

2. **Example scripts**
    - Simple: normalize + resample audio
    - Moderate: process multiple stems
    - Complex: adaptive mastering chain (from d.doc)
    - Real-world: SoundCloud upload pipeline

3. **Documentation**
    - Update BOTS.md with luanette info
    - Add examples to docs/
    - MCP server instructions for Claude

**Files**: `tests/integration.rs`, `docs/luanette.md`

## Summary of Critical Files

### New Files (all in crates/luanette/)
1. **`Cargo.toml`** - Dependencies (mlua, baton, tokio, opentelemetry, etc.)
2. **`src/main.rs`** - Server startup, CLI, OTLP initialization
3. **`src/handler.rs`** - MCP Handler (tools list, call dispatch)
4. **`src/runtime.rs`** - Lua VM, sandbox, execute() with span context
5. **`src/stdlib.rs`** - Lua standard library (log with trace context, clock, math, string, table)
6. **`src/otel_bridge.rs`** - OpenTelemetry Lua namespace (otel.*)
7. **`src/tool_bridge.rs`** - Hootenanny client, tool discovery, Lua bindings with tracing
8. **`src/job_system.rs`** - JobStore (copy from hootenanny)
9. **`src/schema.rs`** - Request/response types with schemars
10. **`src/error.rs`** - AI-friendly error formatting
11. **`src/clients/mod.rs`** - Client module
12. **`src/clients/manager.rs`** - Upstream connection manager
13. **`src/clients/mcp_client.rs`** - Generic MCP client with traceparent propagation
14. **`tests/integration.rs`** - End-to-end tests including tracing
15. **`examples/hello.lua`** - Basic example script
16. **`examples/otel_demo.lua`** - OpenTelemetry features demo

### Modified Files
- **`Cargo.toml`** (workspace root) - Add `"crates/luanette"` member
- **`crates/baton/Cargo.toml`** - Add OpenTelemetry dependencies for trace context extraction (✅ DONE)
- **`crates/baton/src/transport/streamable.rs`** - Add W3C traceparent header extraction (✅ DONE)
- **`crates/baton/src/transport/message.rs`** - Add traceparent extraction for SSE transport (✅ DONE)
- **`crates/baton/src/protocol/mod.rs`** - Accept parent context parameter (✅ DONE)
- **`docs/BOTS.md`** - Add luanette documentation section

## Reference Files to Study
- `/home/atobey/src/halfremembered-mcp/crates/hootenanny/src/main.rs` - Server startup
- `/home/atobey/src/halfremembered-mcp/crates/hootenanny/src/api/handler.rs` - Handler pattern
- `/home/atobey/src/halfremembered-mcp/crates/hootenanny/src/job_system.rs` - Job system
- `/home/atobey/src/halfremembered-mcp/crates/llm-mcp-bridge/src/mcp_client.rs` - MCP client
- `/home/atobey/src/halfremembered-mcp/crates/hootenanny/src/api/schema.rs` - Schema examples
- `/home/atobey/src/halfremembered-mcp/crates/hootenanny/src/mcp_tools/local_models.rs:86-105` - Traceparent injection pattern
- `/home/atobey/src/halfremembered-mcp/crates/hootenanny/src/telemetry.rs` - OpenTelemetry setup
- `/home/atobey/src/halfremembered-mcp/crates/baton/src/protocol/mod.rs` - Span creation and parent context patterns
- `/home/atobey/src/halfremembered-mcp/crates/baton/src/transport/streamable.rs` - HTTP transport traceparent extraction
- `/home/atobey/src/halfremembered-mcp/crates/baton/src/transport/message.rs` - SSE transport traceparent extraction

## Implementation Patterns

### Traceparent Extraction (Baton)
Add to `streamable_handler()` in `baton/src/transport/streamable.rs`:

```rust
use opentelemetry::global;
use opentelemetry_http::HeaderExtractor;

// Extract trace context from incoming HTTP headers
let parent_context = global::get_text_map_propagator(|propagator| {
    propagator.extract(&HeaderExtractor(&headers))
});

// Use parent_context when creating spans in dispatch
// This makes the dispatch span a child of the incoming trace
```

### Traceparent Injection (Luanette MCP Client)
Add to `McpToolClient` in `luanette/src/clients/mcp_client.rs`:

```rust
use opentelemetry::trace::TraceContextExt;
use tracing_opentelemetry::OpenTelemetrySpanExt;

fn inject_trace_context(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    let span = tracing::Span::current();
    let context = span.context();
    let ctx_span = context.span();
    let span_context = ctx_span.span_context();

    if span_context.is_valid() {
        let trace_id = span_context.trace_id();
        let span_id = span_context.span_id();
        let flags = if span_context.is_sampled() { "01" } else { "00" };

        let traceparent = format!("00-{}-{}-{}", trace_id, span_id, flags);
        builder.header("traceparent", traceparent)
    } else {
        builder
    }
}
```

## Resolved Design Decisions (from user feedback)

1. **Parallel Execution**: Start simple with sequential execution only. Iterate when use cases emerge.
2. **Filesystem Access**: Whitelisted directories only (e.g., /tmp, ~/.hootenanny). Defer to post-MVP.
3. **GStreamer**: Defer until use cases emerge.
4. **Virtual Clock**: Simulated time for testing (controllable, deterministic).
5. **Script Versioning**: Immutable CAS artifacts with parent linkage (standard artifact pattern).
6. **W3C Trace Context in Baton**: Adding traceparent extraction to baton benefits the entire MCP ecosystem:
   - Enables distributed tracing across all baton-based servers
   - Hootenanny immediately gains ability to receive trace context from clients
   - Future MCP servers built on baton get this for free
   - Clean separation: baton handles extraction, services handle injection

## Dependencies

### Baton (✅ COMPLETED - traceparent extraction added)
```toml
[dependencies]
# ... existing baton deps ...

# OpenTelemetry (for W3C Trace Context extraction)
opentelemetry = "0.28"
opentelemetry-http = "0.28"
tracing-opentelemetry = "0.29"  # Note: 0.29 for compatibility with hootenanny
```

**Implementation completed (commits: 928a8d6, a74aef0):**
- ✅ Added W3C traceparent extraction in `streamable_handler()` (HTTP transport)
- ✅ Added W3C traceparent extraction in `message_handler()` (SSE transport)
- ✅ Updated `dispatch()` signature to accept `opentelemetry::Context` parameter
- ✅ Used `span.set_parent()` to link incoming traces
- ✅ Both transports now propagate distributed traces correctly
- ✅ Unit tests added and passing (4 tests total):
  - `test_traceparent_extraction` - Validates W3C header parsing (streamable)
  - `test_traceparent_extraction_without_header` - Graceful degradation (streamable)
  - `test_message_handler_traceparent_extraction` - W3C parsing (SSE)
  - `test_message_handler_traceparent_extraction_without_header` - Graceful degradation (SSE)

### Luanette
```toml
[dependencies]
# MCP Framework
baton = { path = "../baton" }

# Lua
mlua = { version = "0.10", features = ["lua54", "async", "send", "serialize"] }

# Async Runtime
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
futures = "0.3"

# Web Server
axum = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1.1.0"

# Error Handling
anyhow = "1"
thiserror = "1"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-opentelemetry = "0.28"
opentelemetry = "0.28"
opentelemetry_sdk = { version = "0.28", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.28", features = ["grpc-tonic", "trace", "logs", "metrics"] }
opentelemetry-semantic-conventions = "0.28"

# Utilities
uuid = { version = "1.11", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json", "stream"] }

# MIDI (for Phase 5)
midly = "0.5"
```

## Success Criteria (MVP)

1. ✅ Server starts on port 8081, responds to MCP initialize
2. ✅ OTLP exporter configured, traces/logs/metrics sent to collector
3. ✅ Discovers hootenanny tools at startup (logs 30+ tools)
4. ✅ Executes Lua scripts via `lua_execute` (async job pattern)
5. ✅ Every script execution creates `luanette.script.execute` span with attributes
6. ✅ Scripts can call hootenanny tools (orpheus_generate, cas_store, etc.)
7. ✅ Traceparent header automatically propagated to all MCP tool calls
8. ✅ `otel.*` namespace works: trace_id(), span_id(), traceparent(), event(), set_attribute(), record_metric()
9. ✅ `log.*` functions emit OpenTelemetry logs with trace context
10. ✅ Async transparency: scripts look synchronous, mlua handles yields
11. ✅ Job system: poll for results, cancel running scripts
12. ✅ Errors include Lua stack traces with line numbers and recorded as span events
13. ✅ Script introspection via `lua_describe`
14. ✅ Script discovery via `script_search` (delegates to graph_context)
15. ✅ Integration tests pass, examples run, traces visible in OTLP collector

## Deferred to Post-MVP

- Parallel execution helpers (`spawn()`, `wait_all()`)
- Filesystem access (whitelisted paths)
- GStreamer integration
- Advanced virtual clock features (MIDI sync)
- Performance optimization (runtime pooling)
- Script versioning UI
- Multi-tier deployment (stable/sandbox/playground)

## Known Risks & Mitigations

### 1. **Lua + Tokio Async Bridge Complexity**
**Risk**: mlua doesn't support true async Lua functions with tokio directly. The `async` feature is limited.

**Mitigation**:
- Run Lua in `spawn_blocking` thread pool
- Use `oneshot` channels for Lua → async communication
- Keep Lua VMs short-lived (create per-execution, don't pool yet)

**Reference**: Similar pattern used in `rust-s3` and other blocking-in-async codebases

### 2. **Temp Directory Cleanup**
**Risk**: Scripts that error out may leave temp files behind.

**Mitigation**:
- Use unique temp directory per script execution (UUID-based)
- Clean up temp dir in drop handler or job completion
- Document temp limits (e.g., max 100MB per script)

### 3. **Type Conversion Edge Cases**
**Risk**: Lua tables with mixed integer/string keys, or sparse arrays, may not map cleanly to JSON.

**Mitigation**:
- Document limitations clearly ("use arrays OR objects, not both")
- Add validation in type conversion layer
- Provide helper: `table.is_array(t)` for script debugging

### 4. **Job System Integration**
**Risk**: JobStore is copied from hootenanny but needs integration with Lua execution cleanup.

**Mitigation**:
- Store Lua VM handle for cancellation (drop VM on abort)
- Ensure proper error propagation from blocking threads
- Test cancellation thoroughly in Phase 6

## Estimated Effort

**Total: 28-34 hours** (5-6 days at 5-6 hours/day)
- Phase 1: 2-3 hours (MCP Server Scaffold)
- Phase 2: 4-5 hours (Upstream Client Manager)
- Phase 3: 5-6 hours (Lua Runtime + OpenTelemetry integration)
- Phase 4: 6-7 hours (Tool Bridge + Traceparent propagation)
- Phase 5: 4-5 hours (Music Standard Library)
- Phase 6: 3-4 hours (Job System)
- Phase 7: 2-3 hours (Discovery & Describe)
- Phase 8: 2-3 hours (Error Handling)
- Phase 9: 3-4 hours (Integration Testing & Examples)

**Note:** OpenTelemetry adds ~4 hours total (2 in Phase 3, 2 in Phase 4)

---

## Lua Scripting Language Design

### Script Structure

Every script must export a `main` function that receives parameters:

```lua
function main(params)
    -- params is a table with whatever was passed to lua_execute
    local input = params.input
    local style = params.style or "ambient"

    -- Call upstream MCP tools
    local result = mcp.hootenanny.orpheus_generate {
        model = "base",
        temperature = 1.0,
        tags = {"style:" .. style}
    }

    -- Return value is serialized back to caller
    return result
end
```

### Optional: describe() Function

For introspection and tool generation:

```lua
function describe()
    return {
        name = "generate_ambient",
        description = "Generate ambient MIDI with Orpheus",
        params = {
            style = { type = "string", required = false, default = "ambient" },
            bars = { type = "number", required = false, default = 8 }
        },
        returns = "Job ID for Orpheus generation"
    }
end
```

### MCP Tool Syntax

MCP tools map to Lua as namespaced function calls:

| MCP Tool Call | Lua Equivalent |
|---------------|----------------|
| `tool_name(param: value)` | `mcp.namespace.tool_name { param = value }` |
| `{ key: value }` | `{ key = value }` |
| `[a, b, c]` | `{ a, b, c }` |
| `null` | `nil` |

**Example:**
```lua
-- Call Hootenanny's cas_inspect tool
local info = mcp.hootenanny.cas_inspect {
    hash = "abc123def456"
}

-- Call Hootenanny's orpheus_generate tool
local job = mcp.hootenanny.orpheus_generate {
    model = "base",
    temperature = 1.0,
    max_tokens = 1024,
    tags = {"experiment", "ambient"}
}

-- Poll for job completion (if tool is async)
local result = mcp.hootenanny.job_poll {
    job_ids = {job.job_id},
    timeout_ms = 30000,
    mode = "any"
}
```

### Music Standard Library

The `midi` module provides high-level MIDI manipulation:

```lua
-- Fetch MIDI from CAS, then parse
local info = mcp.hootenanny.cas_inspect { hash = "abc123def456" }
local track = midi.read(info.local_path)

-- Track structure:
-- {
--   events = {
--     { type = "note_on", channel = 0, note = 60, velocity = 100, time = 0 },
--     { type = "note_off", channel = 0, note = 60, velocity = 0, time = 480 },
--     ...
--   }
-- }

-- Transpose up an octave
midi.transpose(track.events, 12)

-- Quantize to 16th notes (assuming 480 ticks per quarter)
midi.quantize(track.events, 120)

-- Merge multiple tracks
local combined = midi.merge({track1, track2, track3})

-- Write to temp, then upload to CAS
local output_path = temp.path("output.mid")
midi.write(output_path, combined)
local result = mcp.hootenanny.cas_upload_file {
    file_path = output_path,
    mime_type = "audio/midi"
}
```

### Standard Library

Available globals in all scripts:

**Logging:**
```lua
log.debug("Detailed trace information")
log.info("Normal informational message")
log.warn("Warning about potential issue")
log.error("Error occurred")
```

**Math/String/Table:**
```lua
math.random(1, 10)
string.format("Processing %s", filename)
table.insert(results, value)
table.concat(strings, ", ")

for i, item in ipairs(array) do
    print(i, item)
end

for key, value in pairs(object) do
    print(key, value)
end
```

**Virtual Clock (for testing):**
```lua
local start = clock.now()  -- Virtual timestamp (u64)
clock.advance(1000)        -- Advance 1000ms
local elapsed = clock.now() - start
```

**OpenTelemetry (observability):**
```lua
-- Get current trace/span IDs (useful for logging correlation)
local trace_id = otel.trace_id()  -- "a1b2c3d4e5f6789012345678901234ab"
local span_id = otel.span_id()    -- "a1b2c3d4e5f67890"

-- Get W3C traceparent for manual propagation (usually automatic)
local traceparent = otel.traceparent()  -- "00-a1b2c3d4...-01"

-- Add custom span events for debugging
otel.event("midi_processed", {
    note_count = 128,
    bars = 8,
    style = "jazz"
})

-- Set span attributes for filtering/analysis
otel.set_attribute("processing.style", "ambient")
otel.set_attribute("output.format", "midi")

-- Record custom metrics
otel.record_metric("notes.generated", 256, { instrument = "piano" })
otel.record_metric("processing.duration_ms", 1234)
```

### Example Scripts

#### Simple: Generate MIDI with specific style
```lua
function describe()
    return {
        name = "generate_styled_midi",
        description = "Generate MIDI with specified musical style",
        params = {
            style = { type = "string", required = true },
            bars = { type = "number", required = false, default = 8 }
        }
    }
end

function main(params)
    local job = mcp.hootenanny.orpheus_generate {
        model = "base",
        temperature = 1.0,
        tags = {"style:" .. params.style, "bars:" .. (params.bars or 8)}
    }

    return job
end
```

#### Moderate: Process and transpose MIDI
```lua
function main(params)
    -- Fetch and parse input MIDI
    local info = mcp.hootenanny.cas_inspect { hash = params.input_hash }
    local track = midi.read(info.local_path)

    -- Transpose up a fifth
    midi.transpose(track.events, 7)

    -- Quantize to 8th notes
    midi.quantize(track.events, 240)

    -- Write to temp, then upload
    local output_path = temp.path("output.mid")
    midi.write(output_path, track)

    local result = mcp.hootenanny.cas_upload_file {
        file_path = output_path,
        mime_type = "audio/midi"
    }

    return { hash = result.hash }
end
```

#### Complex: Multi-variation generation
```lua
function main(params)
    local variations = {}

    -- Generate 3 variations with different temperatures
    for i = 1, 3 do
        local temp = 0.7 + (i * 0.2)  -- 0.9, 1.1, 1.3

        local job = mcp.hootenanny.orpheus_generate {
            model = "base",
            temperature = temp,
            variation_set_id = params.variation_set_id,
            variation_index = i,
            tags = {"temp:" .. string.format("%.1f", temp)}
        }

        table.insert(variations, job)
    end

    return { variations = variations }
end
```

#### Observability: MIDI Processing with Tracing
```lua
function main(params)
    -- Log the trace context at start for correlation
    log.info("Processing MIDI - trace_id: " .. otel.trace_id())

    otel.set_attribute("input.hash", params.input_hash)
    otel.set_attribute("processing.transpose", params.semitones or 0)

    -- Fetch and parse input MIDI
    otel.event("fetching_midi", { hash = params.input_hash })
    local info = mcp.hootenanny.cas_inspect { hash = params.input_hash }
    local track = midi.read(info.local_path)

    local note_count = #track.events
    otel.record_metric("input.notes", note_count)

    -- Transpose if requested
    if params.semitones and params.semitones ~= 0 then
        otel.event("transposing", { semitones = params.semitones })
        midi.transpose(track.events, params.semitones)
    end

    -- Write output
    otel.event("writing_output")
    local output_path = temp.path("output.mid")
    midi.write(output_path, track)

    -- Upload to CAS (traceparent automatically propagated!)
    local result = mcp.hootenanny.cas_upload_file {
        file_path = output_path,
        mime_type = "audio/midi"
    }

    otel.set_attribute("output.hash", result.hash)
    log.info("Processing complete - output hash: " .. result.hash)

    return {
        hash = result.hash,
        note_count = note_count,
        trace_id = otel.trace_id()  -- Include in response for debugging
    }
end
```

### Error Handling

Use `pcall` for safe execution:

```lua
function main(params)
    local ok, result = pcall(function()
        return mcp.hootenanny.orpheus_generate {
            model = params.model
        }
    end)

    if not ok then
        log.error("Generation failed: " .. tostring(result))
        return { success = false, error = tostring(result) }
    end

    return { success = true, job_id = result.job_id }
end
```

Use `assert` for validation:

```lua
function main(params)
    assert(params.input, "input parameter is required")
    assert(params.style, "style parameter is required")

    -- Proceed with validated params
    local result = mcp.hootenanny.orpheus_generate {
        model = "base",
        tags = {"style:" .. params.style}
    }

    return result
end
```

### Async Transparency

Tool calls that spawn jobs are automatically awaited by mlua:

```lua
-- This LOOKS synchronous but yields internally
local job = mcp.hootenanny.orpheus_generate { model = "base" }

-- Control returns here after the MCP call completes
log.info("Job created: " .. job.job_id)

-- For truly async operations, poll separately
local result = mcp.hootenanny.job_poll {
    job_ids = {job.job_id},
    timeout_ms = 60000
}
```

### Restrictions

**Disabled for security:**
- `dofile`, `loadfile`, `require` - No dynamic code loading
- `os.execute`, `os.remove`, `os.rename` - No system commands or destructive ops
- `debug.*` - No introspection hooks

**Enabled:**
- `io.*` - Full file I/O (read, write, open, close) for development simplicity
- `math.*` - Full library (safe)
- `string.*` - Full library (safe)
- `table.*` - Full library (safe)
- `os.getenv`, `os.clock`, `os.time` - Read-only system info
- `log.*` - Custom logging with automatic trace context
- `otel.*` - OpenTelemetry observability (trace_id, span_id, traceparent, event, set_attribute, record_metric)
- `clock.*` - Virtual time
- `midi.*` - MIDI manipulation
- `temp.*` - Temp directory helpers
- `mcp.*` - Namespaced upstream tools with automatic traceparent propagation

**Note**: Can tighten `io.*` restrictions post-MVP if needed

**Execution limits:**
- Timeout: 30 seconds per script
- No instruction counting (rely on timeout)
- No memory limits (rely on timeout + Rust safety)
