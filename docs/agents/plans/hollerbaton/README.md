# Plan: Migrate Holler to Use Baton

## Status: Complete ✅

Migration completed. Holler now uses baton for MCP protocol handling.

## Goal

Replace holler's custom axum-based MCP implementation with baton, the generic MCP library.

## Current State

Holler has its own MCP implementation:
- `crates/holler/src/mcp.rs` - Custom MCP message handling
- `crates/holler/src/sse.rs` - Custom SSE transport
- Uses axum directly for HTTP routing

Baton provides:
- `transport/sse.rs`, `transport/streamable.rs` - HTTP transports
- `session/store.rs` - Session management
- `types/tool.rs`, `types/resource.rs`, `types/prompt.rs` - MCP types
- `protocol/mod.rs` - Protocol handling

## Migration Steps

### 1. Add baton dependency
```toml
# crates/holler/Cargo.toml
baton = { path = "../baton" }
```

### 2. Replace holler's MCP types with baton's
- Use `baton::types::tool::Tool` instead of custom tool types
- Use `baton::types::resource::Resource` for resources
- Use `baton::types::prompt::Prompt` for prompts

### 3. Use baton's transport layer
- Replace custom SSE handlers with baton's `transport::sse`
- Replace custom message handlers with baton's `protocol` module

### 4. Keep ZMQ forwarding logic
Holler's unique value is bridging MCP to ZMQ. This logic stays:
- Forward tool calls to hootenanny/luanette via ZMQ
- Forward responses back to MCP clients

### 5. Delete redundant code
- `crates/holler/src/sse.rs` - Replaced by baton
- Parts of `crates/holler/src/mcp.rs` - Simplified to just forwarding

## Files to Modify

| Action | Path |
|--------|------|
| EDIT | `crates/holler/Cargo.toml` |
| EDIT | `crates/holler/src/mcp.rs` |
| DELETE | `crates/holler/src/sse.rs` |
| EDIT | `crates/holler/src/main.rs` |

## Benefits

1. **Less code to maintain** - baton handles MCP protocol details
2. **Consistency** - same MCP implementation as other services
3. **Features** - get baton's session management, progress notifications, etc.

## Risks

1. **Breaking changes** - MCP API might behave slightly differently
2. **Testing** - need to verify all tool calls still work through the bridge

## Prerequisites

- baton crate is stable and feature-complete
- Integration tests for holler's ZMQ forwarding

---

## Implementation Notes (2024-12)

### What Was Done

1. **Added baton dependency** to `crates/holler/Cargo.toml`

2. **Created `src/handler.rs`** - New `ZmqHandler` struct implementing `baton::Handler`:
   - Dynamically fetches tools from ZMQ backends (luanette, hootenanny, chaosgarden)
   - Routes tool calls via `BackendPool::route_tool()` based on name prefix
   - Converts MCP arguments to `hooteproto::Payload` variants
   - Returns `baton::CallToolResult` with success/error content

3. **Updated `src/serve.rs`**:
   - Uses `baton::McpState<ZmqHandler>` and `baton::dual_router()`
   - Nested MCP routes under `/mcp` (Streamable: POST `/mcp`, SSE: GET `/mcp/sse` + POST `/mcp/message`)
   - Health endpoint remains at `/health` with its own state
   - Spawns baton's session cleanup task with cancellation token

4. **Deleted redundant files**:
   - `src/mcp.rs` - Custom JSON-RPC handling (replaced by baton)
   - `src/sse.rs` - Custom SSE transport (replaced by baton)

5. **Updated `src/main.rs`** - Removed `mcp` and `sse` modules, added `handler`

### API Changes

Old endpoints:
- `POST /mcp` - JSON-RPC
- `GET /sse` - Server-sent events

New endpoints (baton dual_router):
- `POST /mcp` - Streamable HTTP transport (recommended)
- `DELETE /mcp` - Session termination
- `GET /mcp/sse` - SSE connection (legacy)
- `POST /mcp/message` - SSE message endpoint (legacy)

### Remaining Work

1. **ZMQ SUB → baton notifications**: Currently ZMQ SUB broadcasts from backends are received but not forwarded to MCP clients. This needs to be wired to baton's `ResourceNotifier` or custom notification system.

2. **Traceparent propagation**: The `call_tool_with_context` doesn't currently extract traceparent from baton's context. Add when baton exposes trace context.

3. **Integration tests**: Add HTTP-level tests that actually start holler and test the MCP endpoints end-to-end.
