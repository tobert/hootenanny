# Plan: Migrate Holler to Use Baton

## Status: Deferred

This migration was split out from the crate consolidation plan to be done later.

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
