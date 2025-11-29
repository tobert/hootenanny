# Smooth Reconnects Plan

**Status**: ✅ Implemented

**Goal**: Make MCP reconnects seamless for Claude Code when hootenanny restarts (cargo watch, crashes, deploys).

**Current Behavior**: Sessions are transparently recreated when the server restarts. The baton SessionStore's `get_or_create()` always succeeds - either returning an existing session or creating a new one with the same ID.

## Problem Analysis

### What Happens Now
1. Server restarts → all in-memory sessions lost
2. Claude Code's next request fails (session not found)
3. Claude Code shows error, requires manual `/mcp` to reconnect

### What Should Happen
1. Server restarts → sessions restored from disk OR new session created transparently
2. Claude Code's next request works (session resumed or re-initialized)
3. No user intervention needed

## Approach Options

### Option A: Persistent Sessions (Server-Side)
Store sessions in sled so they survive restarts.

**Pros**: Sessions truly persist, client doesn't need changes
**Cons**: Complexity, stale session cleanup, state sync issues

### Option B: Stateless Sessions (Simpler)
Sessions are ephemeral. Client re-initializes on any error.

**Pros**: Simple, no persistence needed, always clean state
**Cons**: Requires Claude Code to handle gracefully (it should already)

### Option C: Health Check + Auto-Reconnect
Add health endpoint. Claude Code polls and reconnects proactively.

**Pros**: Proactive, smoother UX
**Cons**: Polling overhead, still needs reconnect logic

## Recommended: Option B + Health Check

1. **Accept sessions are ephemeral** - Restart = new session, that's fine
2. **Improve error responses** - Return clear "session expired, please re-initialize"
3. **Add health endpoint** - `/health` for monitoring
4. **Ensure re-init is fast** - Sub-100ms initialize response

## Implementation Tasks

### Task 1: Better Session Expiry Handling
When a request comes with unknown session ID:
- Return proper JSON-RPC error with clear message
- Include hint to re-initialize
- HTTP 404 with `Mcp-Session-Id` header for new session

### Task 2: Health Endpoint
```rust
GET /health -> 200 OK {"status": "healthy", "uptime_secs": N}
```

### Task 3: Startup Notification (Optional)
Log/trace when server starts so observability shows restart events.

### Task 4: Document Reconnect Behavior
Update CLAUDE.md or MCP config with expected reconnect behavior.

## Questions to Investigate

1. Does Claude Code auto-reconnect on certain errors?
2. What error response triggers best reconnect behavior?
3. Is there an MCP spec for session resumption?

## Success Criteria

- [x] Server restart + immediate tool call = works (sessions transparently recreated)
- [x] No "Failed to reconnect" on normal restarts (baton handles gracefully)
- [x] Health endpoint available: `GET /health`

## Implementation

### Health Endpoint
Added `GET /health` returning:
```json
{
  "status": "healthy",
  "uptime_secs": N,
  "version": "X.Y.Z",
  "sessions": { "total": N, "connected": N },
  "jobs": { "pending": N, "running": N }
}
```

### Session Handling
The baton crate's `InMemorySessionStore::get_or_create()` handles reconnects by:
1. If session ID exists: return it
2. If session ID doesn't exist (post-restart): create new session with same ID
3. Either way, client can continue without explicit re-initialization

---

Authors:
- Claude: Initial plan, implementation

🤖 Claude <claude@anthropic.com>
