# rmcp Session Management Extension Proposal

## Problem Statement

We're building HalfRemembered MCP, an ensemble performance space where multiple LLM agents (Claude Code, Gemini CLI, etc.) collaborate on music generation in real-time. We need **opt-in session management capabilities** that allow server implementations to control session lifecycle without forcing behavior changes on all rmcp users.

### What We Observed

1. **Client Reconnection Works**: Claude Code successfully reconnects after server restart
2. **Session ID is Stale**: The old session ID is no longer valid
3. **Requests Fail**: POST to `/message?sessionId=<old-id>` returns HTTP 404 NOT_FOUND
4. **Our Needs**: Server-side control over session lifecycle for multi-agent scenarios

### Current rmcp Behavior

In `rmcp-0.8.5/src/transport/sse_server.rs`:

```rust
tokio::spawn(async move {
    // Wait for connection closure
    to_client_tx_clone.closed().await;

    // Clean up session IMMEDIATELY
    let session_id = session.clone();
    let tx_store = app.txs.clone();
    let mut txs = tx_store.write().await;
    txs.remove(&session_id);  // ❌ Immediate removal
    tracing::debug!(%session_id, "Closed session and cleaned up resources");
});
```

**Problem**: When the SSE stream closes (network blip, server restart, laptop sleep), the session is immediately removed. Reconnecting clients have no way to resume.

## Use Case: Multi-Agent Musical Ensemble

**Scenario**: 3-5 LLM agents collaborating on music generation
- Claude Code agents: generating melodies, harmonies
- Gemini agents: analyzing patterns, suggesting variations
- Shared state: job queue (model inference), artifact store (MIDI files), conversation tree

**Requirements**:
- Agents should survive brief disconnections (network hiccups, server restarts)
- Long-running jobs (30-60s music generation) shouldn't be lost
- Reconnection should be transparent to the user
- Multiple concurrent sessions (one per agent) must be stable

**Current Pain Points**:
1. Server restart → all agents lose session → must manually reconnect
2. Network blip → 404 errors → user thinks server is broken
3. No session state persistence → can't resume mid-workflow
4. Client implementations must handle reconnection themselves (complex)

## Proposed Solution: Opt-In Session Management Hooks

Add **optional callbacks** that let server implementations manage session lifecycle, without changing default behavior or requiring client changes.

### Option 1: Session Lifecycle Hooks (RECOMMENDED)

Expose hooks for session disconnect events, allowing servers to control when cleanup happens.

```rust
// Hook receives session_id and a cleanup function it can call when ready
pub type SessionCleanupFn = Box<dyn FnOnce() + Send>;

pub struct SseServerConfig {
    pub bind: SocketAddr,
    pub sse_path: String,
    pub post_path: String,
    pub ct: CancellationToken,
    pub sse_keep_alive: Option<Duration>,

    // NEW: Optional session disconnect callback
    // Receives: (session_id, cleanup_fn)
    // Hook decides when to call cleanup_fn
    pub on_session_disconnect: Option<
        Box<dyn Fn(SessionId, SessionCleanupFn) + Send + Sync>
    >,
}

// In sse_handler cleanup:
tokio::spawn(async move {
    to_client_tx_clone.closed().await;

    let session_id = session.clone();

    // Prepare cleanup function that removes session
    let cleanup: SessionCleanupFn = Box::new(move || {
        let mut txs = tx_store.write().await;
        if txs.remove(&session_id).is_some() {
            tracing::debug!(%session_id, "Cleaned up session");
        }
    });

    if let Some(callback) = &config.on_session_disconnect {
        // Hook controls when cleanup happens
        callback(session_id, cleanup);
    } else {
        // Default: immediate cleanup
        cleanup();
    }
});
```

**Example Usage (Server Implementation)**:
```rust
// Server can implement custom TTL if desired
let sse_config = SseServerConfig {
    bind: addr.parse()?,
    sse_path: "/sse".to_string(),
    post_path: "/message".to_string(),
    ct: shutdown_token.clone(),
    sse_keep_alive: Some(Duration::from_secs(30)),

    // Opt-in to delayed cleanup (TTL pattern)
    on_session_disconnect: Some(Box::new(|session_id, cleanup| {
        tokio::spawn(async move {
            tracing::info!(%session_id, "Session disconnected, starting 5min TTL");

            // Wait 5 minutes before cleanup
            tokio::time::sleep(Duration::from_secs(300)).await;

            // Now cleanup
            cleanup();
            tracing::info!(%session_id, "Session TTL expired, cleaned up");
        });
    })),
};
```

**Alternative patterns the hook enables**:

```rust
// Immediate cleanup with logging
on_session_disconnect: Some(Box::new(|session_id, cleanup| {
    tracing::info!(%session_id, "Session disconnected");
    cleanup(); // Immediate
})),

// Metrics collection
on_session_disconnect: Some(Box::new(|session_id, cleanup| {
    metrics::increment_counter("sessions_disconnected");
    cleanup();
})),

// Conditional TTL (based on session state)
on_session_disconnect: Some(Box::new(|session_id, cleanup| {
    let has_pending_jobs = check_pending_jobs(&session_id);
    if has_pending_jobs {
        // Keep alive 5 minutes for jobs to complete
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(300)).await;
            cleanup();
        });
    } else {
        // No pending work, immediate cleanup
        cleanup();
    }
})),
```

**Benefits**:
- ✅ **Opt-in**: Default behavior unchanged (backward compatible)
- ✅ **Server-controlled**: Each deployment decides policy
- ✅ **No client changes**: Works with existing clients
- ✅ **Flexible**: Can implement TTL, logging, metrics, conditional cleanup
- ✅ **No security concerns**: Server owns the policy
- ✅ **Clean abstraction**: Hook receives cleanup function, controls when it's called
- ✅ **Composable**: Can do logging + metrics + TTL in one hook

**Tradeoffs**:
- ⚠️ Server implementations must manage cleanup timing (but only if they opt-in)
- ⚠️ Slight API surface expansion
- ⚠️ Hook must call cleanup() or session leaks (documented requirement)

### Option 2: Expose Session Store API

Allow server implementations to replace the default session storage.

```rust
pub trait SessionStore: Send + Sync {
    async fn insert(&self, session_id: SessionId, tx: Sender<Message>);
    async fn get(&self, session_id: &SessionId) -> Option<Sender<Message>>;
    async fn remove(&self, session_id: &SessionId) -> Option<Sender<Message>>;
}

pub struct SseServerConfig {
    pub bind: SocketAddr,
    pub sse_path: String,
    pub post_path: String,
    pub ct: CancellationToken,
    pub sse_keep_alive: Option<Duration>,

    // NEW: Optional custom session store
    pub session_store: Option<Arc<dyn SessionStore>>,
}
```

**Benefits**:
- ✅ Maximum flexibility
- ✅ Server can implement TTL, persistence, replication, etc.
- ✅ Clean abstraction

**Tradeoffs**:
- ⚠️ More invasive API change
- ⚠️ Async trait complexity
- ⚠️ May be overkill for simple use cases

## Recommendation: Option 1 (Lifecycle Hooks)

**Why**:
- ✅ **Minimal API surface**: Single optional callback
- ✅ **Backward compatible**: Default behavior unchanged
- ✅ **Zero security risk**: No automatic session reuse, server controls policy
- ✅ **Works today**: No client changes needed
- ✅ **Use-case agnostic**: Can implement TTL, metrics, logging, etc.

**Implementation Checklist**:
- [ ] Add `SessionCleanupFn` type alias
- [ ] Add `on_session_disconnect: Option<Box<dyn Fn(SessionId, SessionCleanupFn) + Send + Sync>>` to SseServerConfig
- [ ] Wrap cleanup logic in closure passed to hook
- [ ] Update sse_handler to call hook or default cleanup
- [ ] Document behavior in rmcp README (emphasize hook MUST call cleanup())
- [ ] Add examples: TTL, logging, conditional cleanup
- [ ] Add tests: hook invocation, default behavior, cleanup verification

## Upstream Path

**Questions for rmcp maintainers**:

1. **Interest in session management extensibility?** Would you accept a PR adding optional lifecycle hooks (on_session_disconnect)?

2. **API preferences?**
   - Callback approach vs trait-based vs other?
   - Naming conventions?
   - Documentation style?

3. **Scope**: Should this be SSE-specific or extend to other transports?

4. **Use cases**: Are there other scenarios where server-controlled session management would be valuable?

**Key Points for Discussion**:
- ✅ Opt-in only (default behavior unchanged)
- ✅ No client-side changes required
- ✅ Server owns security/policy decisions
- ✅ Enables but doesn't mandate TTL/persistence patterns

## Testing Strategy

If we implement this (either locally or for upstream):

1. **Unit tests**:
   - Session cleanup after TTL
   - Reconnection within TTL
   - Concurrent sessions

2. **Integration tests**:
   - Server restart → client reconnect → success
   - Network blip → auto-recover
   - TTL expiry → 404 on stale POST

3. **Load tests**:
   - 10+ concurrent sessions with random disconnects
   - Memory usage with various TTL values
   - Session churn rate monitoring

## References

- **rmcp repository**: https://github.com/modelcontextprotocol/rust-sdk
- **Our implementation**: `/home/atobey/src/halfremembered-mcp/crates/hootenanny`
- **SSE spec**: https://html.spec.whatwg.org/multipage/server-sent-events.html
- **Related issue**: SSE reconnection is a common pattern in event-driven systems

## Project Context

**HalfRemembered MCP** is an experimental multi-agent music collaboration system using:
- Multiple LLM agents (Claude Code, Gemini) as "ensemble members"
- Local music models (Orpheus) for generation
- Shared state (jobs, artifacts, conversations)
- OpenTelemetry for observability (all sessions → otlp-mcp)

The session resilience problem is blocking our multi-agent jam session experiments. We need stable SSE connections for agents to collaborate effectively.

---

**Authors**:
- 🤖 Claude (claude@anthropic.com)
- 👤 Amy Tobey (atobey)

**Date**: 2025-11-27

**Status**: Proposal - awaiting rmcp maintainer feedback
