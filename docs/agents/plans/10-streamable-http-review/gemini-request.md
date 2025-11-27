# Gemini Review Request: Streamable HTTP MCP Implementation

## Context

We just got streamable HTTP MCP transport working with Claude Code! The migration from SSE to streamable HTTP revealed some interesting integration challenges. We'd like your review of what we implemented and ideas for improving session resilience on server restart.

## What We Did

### The Problem

After switching from `transport-sse-server` to `transport-streamable-http-server` in rmcp, Claude Code couldn't connect. Error:

```
HTTP 404: Invalid OAuth error response: SyntaxError: Unexpected end of JSON input. Raw body:
```

### Root Cause

Claude Code's HTTP MCP transport performs **OAuth discovery** before connecting (per RFC 9728). It hits:
- `/.well-known/oauth-authorization-server`
- `/.well-known/oauth-protected-resource`

Our streamable HTTP service was mounted at `/mcp` and catching ALL sub-paths including `.well-known/*`. The rmcp streamable HTTP handler returns `406 Not Acceptable` for GET requests without `Accept: text/event-stream` header, which Claude Code couldn't parse as valid OAuth response.

### The Fix

Added explicit routes for OAuth discovery that return proper JSON 404 responses BEFORE the streamable HTTP service catches them:

```rust
// Handler for OAuth discovery - return 404 with JSON to indicate no OAuth required
async fn no_oauth() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        r#"{"error": "not_found", "error_description": "This MCP server does not require authentication"}"#
    )
}

// OAuth discovery endpoints must be handled BEFORE the catch-all MCP service
let app_router = axum::Router::new()
    .route("/mcp/.well-known/oauth-authorization-server", axum::routing::get(no_oauth))
    .route("/mcp/.well-known/oauth-protected-resource", axum::routing::get(no_oauth))
    .route("/.well-known/oauth-authorization-server", axum::routing::get(no_oauth))
    .route("/.well-known/oauth-protected-resource", axum::routing::get(no_oauth))
    .nest_service("/mcp", service)
    .merge(web_router);
```

### Current Server Setup

```rust
// Create session manager for persistent sessions
let session_manager = Arc::new(LocalSessionManager::default());

// Create StreamableHttpService
let service = StreamableHttpService::new(
    move || {
        Ok(EventDualityServer::new_with_state(
            shared_state.clone(),
            local_models_svc.clone(),
            artifact_store_svc.clone(),
            job_store_svc.clone(),
            audio_graph_adapter_svc.clone(),
            audio_graph_db_svc.clone(),
        ))
    },
    session_manager.clone(),
    StreamableHttpServerConfig {
        sse_keep_alive: Some(Duration::from_secs(30)),
        stateful_mode: true,  // Keep sessions alive
    },
);
```

## Outstanding Issues

### Session Loss on Server Restart

When the server restarts (e.g., cargo-watch triggering rebuild), all MCP sessions are lost. Claude Code gets:

```
HTTP 404: Invalid OAuth error response...
```

Until the user manually runs `/mcp` to reconnect.

**Current behavior:**
1. Server restarts → `LocalSessionManager` is recreated → all sessions gone
2. Client tries to use old `Mcp-Session-Id` header → 401 Unauthorized (session not found)
3. But the error surfaces as "OAuth error" because of how Claude Code handles non-JSON 401 responses

### Questions for You

1. **OAuth 404 Response Format**: Is our JSON 404 response correct per RFC 9728? Should we include additional fields?

2. **Session Persistence**: The `LocalSessionManager` from rmcp stores sessions in memory. For development with cargo-watch, sessions die on every rebuild. Options:
   - Implement a custom `SessionManager` that persists to disk/sled
   - Add session TTL so stale sessions are cleaned up gracefully
   - Something else?

3. **401 vs 404**: When a client sends an invalid `Mcp-Session-Id`, rmcp returns 401. Should we wrap this to return a more parseable error for clients that expect OAuth flows?

4. **Reconnection Flow**: Per the streamable HTTP spec, clients can reconnect with `Last-Event-ID` header. Should we be implementing event replay on reconnection?

## Existing Work

We have a session TTL proposal for the SSE transport at:
`docs/agents/plans/08-rmcp-session-resilience/session-ttl-proposal.md`

Key idea: Add `on_session_disconnect` hook to rmcp that lets servers control session cleanup timing. This was written for SSE but the same pattern could apply to streamable HTTP's `LocalSessionManager`.

## What We Want From You

1. **Code Review**: Does our OAuth discovery fix look correct? Any edge cases we're missing?

2. **Session Resilience Ideas**: How should we handle server restarts gracefully? Options:
   - Custom `SessionManager` implementation
   - Client-side reconnection improvements
   - Something in the protocol layer?

3. **Testing Strategy**: What should we test to ensure the streamable HTTP transport is robust?

4. **rmcp Upstream**: Should we propose changes to rmcp for better session management, or handle it all in our server code?

## Files to Review

- `crates/hootenanny/src/main.rs` - Server setup with OAuth routes
- `docs/agents/plans/08-rmcp-session-resilience/session-ttl-proposal.md` - Previous session work
- rmcp source: `~/.cargo/git/checkouts/rust-sdk-773cd6d57c4837f3/03040f8/crates/rmcp/src/transport/streamable_http_server/`

## Victory Celebration

We generated a fanfare to celebrate getting this working! The pipeline:
- Orpheus (music transformer) → MIDI tokens
- CAS (BLAKE3 content-addressable storage) → persistent artifacts
- RustySynth + FF4 soundfont → WAV audio
- All orchestrated via MCP tool calls over streamable HTTP

The fanfare is 109 seconds of boisterous, brassy celebration. 🎺

---

**Authors**:
- 🤖 Claude (claude@anthropic.com)
- 👤 Amy Tobey (atobey)

**Date**: 2025-11-27
