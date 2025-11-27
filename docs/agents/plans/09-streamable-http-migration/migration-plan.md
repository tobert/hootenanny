# Migration Plan: SSE → Streamable HTTP

**Date**: 2025-11-27
**Status**: Planning
**Breaking Change**: Yes - complete migration, no backwards compatibility

## Executive Summary

Migrate from `rmcp::transport::sse_server::SseServer` to `rmcp::transport::streamable_http_server::StreamableHttpService`. This solves the session reconnection bug permanently and provides better session management.

## Why Migrate?

### Current Problem (SSE)
- ❌ Sessions die immediately on disconnect
- ❌ No session resumption support
- ❌ Client can't provide session ID
- ❌ HTTP 410 errors after server restart
- ❌ Requires client-side reconnection logic

### Solution (Streamable HTTP)
- ✅ Persistent sessions via `SessionManager`
- ✅ Client-provided session IDs (from headers)
- ✅ `resume(session_id, last_event_id)` support
- ✅ Stateful mode keeps sessions alive
- ✅ Built into rmcp - no forking needed!

## Architecture Comparison

### Current (SSE)
```rust
// main.rs
let sse_config = SseServerConfig {
    bind: addr.parse()?,
    sse_path: "/sse".to_string(),
    post_path: "/message".to_string(),
    ct: shutdown_token.clone(),
    sse_keep_alive: Some(Duration::from_secs(30)),
};

let (sse_server, sse_router) = SseServer::new(sse_config);

let app_router = sse_router.merge(web_router);

let bind_addr = sse_server.config.bind;
let ct = sse_server.with_service(move || {
    EventDualityServer::new_with_state(...)
});
```

**Endpoints**:
- `GET /sse` → SSE stream (server generates session ID)
- `POST /message?sessionId=<id>` → Send request

**Session Lifecycle**:
1. Client connects to `/sse`
2. Server generates UUID, sends in first event
3. Client uses ID for POST requests
4. Disconnect → session removed immediately
5. Reconnect → new UUID, old ID = 410

### Target (Streamable HTTP)
```rust
// main.rs
use rmcp::transport::streamable_http_server::{
    StreamableHttpService,
    StreamableHttpServerConfig,
    session::local::LocalSessionManager,
};

let session_manager = Arc::new(LocalSessionManager::new());

let service = StreamableHttpService::new(
    move || {
        Ok(EventDualityServer::new_with_state(
            shared_state.clone(),
            local_models.clone(),
            artifact_store.clone(),
            job_store.clone(),
            audio_graph_adapter.clone(),
            audio_graph_db.clone(),
        ))
    },
    session_manager.clone(),
    StreamableHttpServerConfig {
        sse_keep_alive: Some(Duration::from_secs(30)),
        stateful_mode: true,  // ← Persistent sessions
    },
);

let app_router = Router::new()
    .fallback_service(service)
    .merge(web_router);

let listener = tokio::net::TcpListener::bind(addr).await?;
axum::serve(listener, app_router)
    .with_graceful_shutdown(async move {
        shutdown_token.cancelled().await;
    })
    .await?;
```

**Endpoints** (stateful mode):
- `POST /` → Single-shot request OR create session + stream
- `GET /` + `X-Session-ID` header → Resume/stream responses
- `DELETE /` + `X-Session-ID` header → Close session

**Session Lifecycle**:
1. Client POSTs initial request
2. Server creates session, returns ID in response header
3. Client can GET with same session ID to stream
4. Disconnect → session kept alive by SessionManager
5. Reconnect with same ID → resumes!

## Migration Steps

### Phase 1: Research & Validation
- [x] Confirm rmcp has streamable HTTP transport
- [x] Verify MCP TypeScript SDK support
- [ ] Read LocalSessionManager implementation
- [ ] Understand session resume mechanics
- [ ] Check if Claude Code supports streamable HTTP
- [ ] Identify required Cargo features

### Phase 2: Code Changes

#### 2.1 Update Dependencies
```toml
# Cargo.toml
[dependencies]
rmcp = {
    git = "https://github.com/modelcontextprotocol/rust-sdk",
    features = [
        "transport-streamable-http-server",  # ← Add this
        # Remove: "transport-sse-server"
    ]
}
```

#### 2.2 Modify main.rs

**Remove**:
```rust
use rmcp::transport::sse_server::{SseServer, SseServerConfig};

let sse_config = SseServerConfig { ... };
let (sse_server, sse_router) = SseServer::new(sse_config);
let ct = sse_server.with_service(...);
```

**Add**:
```rust
use rmcp::transport::streamable_http_server::{
    StreamableHttpService,
    StreamableHttpServerConfig,
    session::local::LocalSessionManager,
};

let session_manager = Arc::new(LocalSessionManager::new());
let service = StreamableHttpService::new(...);
// Direct axum serve (no separate sse_server struct)
```

#### 2.3 Update Server Initialization

**Remove** the `sse_server.with_service()` pattern.

**Add** direct service instantiation in StreamableHttpService::new().

#### 2.4 Graceful Shutdown

**Current**:
```rust
let ct = sse_server.with_service(...);
// ct is used for graceful shutdown
```

**New**:
```rust
// Use shutdown_token directly with axum serve
axum::serve(listener, app_router)
    .with_graceful_shutdown(async move {
        shutdown_token.cancelled().await;
    })
    .await?;
```

#### 2.5 Remove SSE-specific Code

- Remove SSE endpoint documentation
- Update telemetry (no more "SSE connection" logs)
- Update README with new endpoint URLs

### Phase 3: Client Configuration

Update `.claude/mcp.json`:

**Current**:
```json
{
  "mcpServers": {
    "hrmcp": {
      "command": "hootenanny",
      "args": ["--port", "8080"],
      "transport": "sse",
      "url": "http://127.0.0.1:8080/sse"
    }
  }
}
```

**New**:
```json
{
  "mcpServers": {
    "hrmcp": {
      "command": "hootenanny",
      "args": ["--port", "8080"],
      "transport": "streamable-http",
      "url": "http://127.0.0.1:8080/"
    }
  }
}
```

Note: Check exact transport name in MCP TypeScript SDK.

### Phase 4: Testing

#### Test Cases
1. **Initial Connection**
   - [ ] Client connects and receives session ID
   - [ ] Tools execute successfully
   - [ ] Responses stream correctly

2. **Server Restart**
   - [ ] Client detects disconnect
   - [ ] Client reconnects with same session ID
   - [ ] Session resumes (or gracefully creates new one)
   - [ ] No HTTP 410 errors

3. **Multi-Client**
   - [ ] Multiple Claude Code sessions connect simultaneously
   - [ ] Sessions isolated correctly
   - [ ] No session ID collisions

4. **Long-Running Jobs**
   - [ ] Start music generation job
   - [ ] Restart server mid-job
   - [ ] Job completes (stored in JobStore)
   - [ ] Client can retrieve result after reconnect

5. **Session Cleanup**
   - [ ] Inactive sessions cleaned up after timeout
   - [ ] No memory leaks with many connects/disconnects

## Breaking Changes

### Server Side
- ❌ **Endpoints changed**: `/sse` → `/`, `/message` → `/`
- ❌ **Session model**: No more auto-generated IDs on connect
- ✅ **State preserved**: Jobs, artifacts, conversations still work

### Client Side
- ❌ **Transport type**: Must use `streamable-http` not `sse`
- ❌ **URL**: Change from `/sse` to `/`
- ✅ **Session handling**: Better reconnection support

### Configuration
- ❌ **mcp.json**: Must update transport and URL
- ❌ **Cargo features**: Switch feature flags

## Benefits

### Immediate
1. **No more 410 errors** after server restart
2. **Session resumption** works correctly
3. **Client-controlled sessions** (can keep same ID)

### Long-term
1. **Better multi-agent support** (persistent sessions)
2. **Custom SessionManager** (can add Redis, etc.)
3. **Standard MCP transport** (not SSE-specific)

## Risks & Mitigation

### Risk: Claude Code doesn't support streamable HTTP
**Mitigation**: Verified MCP TS SDK has examples. Check Claude Code source.
**Fallback**: Stay on SSE, fork rmcp for session TTL

### Risk: SessionManager doesn't persist across server restart
**Mitigation**: LocalSessionManager is in-memory. For persistence, implement custom SessionManager with sled.
**Acceptable**: In-memory is fine for dev, can enhance later

### Risk: Performance regression
**Mitigation**: Both use HTTP + SSE underneath, should be equivalent.
**Testing**: Benchmark before/after with 10+ concurrent clients

### Risk: Breaking existing workflows
**Mitigation**: This is a breaking change. Document clearly, update all configs.
**Communication**: Update README, add migration guide

## Timeline

### Immediate (Today)
- [x] Research streamable HTTP API
- [ ] Verify Claude Code support
- [ ] Test minimal example

### Short-term (This Week)
- [ ] Implement migration
- [ ] Test with single client
- [ ] Test server restart scenario

### Medium-term (Next Week)
- [ ] Multi-client testing
- [ ] Session persistence investigation
- [ ] Performance benchmarking
- [ ] Documentation updates

## Success Criteria

- [ ] Server restarts without breaking client connections
- [ ] No HTTP 410 errors on reconnection
- [ ] Multi-client jam sessions work reliably
- [ ] All existing features (jobs, artifacts, etc.) work unchanged
- [ ] Clean shutdown and session cleanup

## References

- **rmcp streamable HTTP**: `~/.cargo/registry/.../rmcp-0.8.5/src/transport/streamable_http_server/`
- **SessionManager trait**: `streamable_http_server/session.rs`
- **LocalSessionManager**: `streamable_http_server/session/local.rs`
- **MCP TS SDK examples**: `~/src/claude/mcp-typescript-sdk/src/examples/client/streamableHttpWithSseFallbackClient.ts`

## Open Questions

1. **Does Claude Code support streamable HTTP?**
   - Check: `~/src/claude/claude-code/` codebase
   - Look for: transport configuration, streamable HTTP usage

2. **What's the exact transport name in mcp.json?**
   - Options: `streamable-http`, `http`, `streamableHttp`
   - Check: MCP TS SDK client examples

3. **Does LocalSessionManager support TTL?**
   - Read: `session/local.rs` implementation
   - If not: Can we extend it?

4. **How are sessions cleaned up?**
   - Automatic timeout?
   - Manual cleanup needed?
   - Configuration options?

---

**Authors**:
- 🤖 Claude (primary)
- 💎 Gemini (via other Claude session suggestion)

**Status**: Ready for implementation pending Claude Code compatibility verification
