# Claude Review Response: Direct MCP Implementation

**Reviewer**: 🤖 Claude
**Date**: 2025-11-28

---

## Overall Assessment

The plan is sound. "Keep the Types, Dump the Engine" is exactly the right strategy—`rmcp::model` gives us the JSON-RPC scaffolding and schema generation while we take control of the transport layer. This mirrors how mature systems often evolve: use libraries for serialization/types, own the I/O.

---

## Responses to Specific Questions

### 1. Tool Dispatch: Macro vs. Manual

**Recommendation: Manual dispatch, with a twist.**

The `#[tool_router]` macro generates a `ToolRouter<Self>` that's stored as a field in `EventDualityServer`. Looking at the current implementation (`server.rs:71`, `server.rs:587`), the macro:
1. Collects all `#[tool]` methods into a `HashMap<String, Box<dyn ToolHandler>>`
2. Generates `Self::tool_router()` to build this map
3. Relies on the `ServerHandler` trait impl to route `tools/call` → `tools/list`

**The insight**: The macro *does* generate dispatch logic, but it's coupled to `rmcp::ServerHandler`. You can't easily reuse it outside that trait.

**Two paths forward:**

**Option A: Manual match (as proposed)**
```rust
// task_04: explicit dispatch
match params.name.as_str() {
    "play" => server.play(Parameters(params.arguments)).await,
    "add_node" => server.add_node(Parameters(params.arguments)).await,
    // ... ~25 tools
    _ => Err(McpError::method_not_found(...))
}
```

**Pros**: Full control, easy to debug, no macro magic.
**Cons**: Must manually keep in sync with `#[tool]` annotations.

**Option B: Extract and reuse the generated router**

The `ToolRouter<Self>` is already built. You could expose it:
```rust
impl EventDualityServer {
    pub async fn dispatch_tool(&self, name: &str, args: Value) -> Result<CallToolResult, McpError> {
        self.tool_router.call(name, self, args).await
    }
}
```

This requires inspecting `rmcp::handler::server::router::tool::ToolRouter` to see if `call()` is public or can be made so. Looking at the architecture, it likely has an internal `Route` that wraps the closures.

**My recommendation**: Start with **Option A** (manual match). It's verbose but transparent. If maintaining sync becomes painful after 30+ tools, revisit Option B. The manual approach also lets you inject per-tool middleware (rate limiting, per-tool tracing spans) trivially.

---

### 2. Concurrency: DashMap + Arc<Mutex<ConversationState>>

**No deadlock risk with current design.**

The locking hierarchy is:
```
DashMap<SessionId, Session>  ←  outer, concurrent reads
    └── Session.server: EventDualityServer (Clone, no locks here)
            └── Arc<Mutex<ConversationState>>  ←  inner, per-mutation
```

**Why this is safe:**
- `DashMap` uses sharded locks internally—lookups don't block each other across shards.
- You only hold the `DashMap` read guard long enough to clone/Arc the Session.
- `ConversationState` mutex is acquired *after* releasing the session lookup.

**Potential hazard to watch:**
```rust
// DON'T do this
let session = sessions.get(&id);  // holds DashMap read guard
let state = session.server.state.lock();  // holds Mutex
sessions.insert(other_id, ...);  // needs write guard → DEADLOCK
```

**Fix**: Always extract what you need, drop the guard, then operate:
```rust
let server = sessions.get(&id).map(|s| s.server.clone());
drop(sessions);  // explicit, though scope would handle it
if let Some(server) = server {
    let state = server.state.lock();  // safe, no DashMap guard held
}
```

Given the HTTP request/response model, this is natural—each request is short-lived and processes a single session.

---

### 3. Testing: Tokio Task vs. Separate Process (Sled Locks)

**Tokio tasks are sufficient for your goal, but with a caveat.**

**The sled concern is real but addressable:**
- Sled uses file locks (`flock`) for single-writer guarantees.
- If you `drop()` the `Journal` / `ConversationStore` before restarting, the lock releases.
- In a Tokio task, you control this explicitly.

**Recommended test structure:**
```rust
#[tokio::test]
async fn test_session_survives_restart() {
    let state_dir = tempdir().unwrap();

    // Phase 1: Start server, create session
    let (shutdown_tx, server_handle) = start_server(state_dir.path()).await;
    let session_id = client.connect().await;
    client.call_tool("play", ...).await;

    // Graceful shutdown - ensures sled flushes
    shutdown_tx.send(()).unwrap();
    server_handle.await.unwrap();  // wait for clean exit

    // Phase 2: Restart on same state_dir
    let (shutdown_tx, server_handle) = start_server(state_dir.path()).await;

    // Same session_id should work
    let result = client.call_tool_with_session(session_id, "get_tree_status").await;
    assert!(result.is_ok());  // session was "rehydrated"

    shutdown_tx.send(()).unwrap();
    server_handle.await.unwrap();
}
```

**Separate process testing** is valuable for catching different bugs:
- Verifies the binary's signal handling (SIGTERM → clean shutdown).
- Tests the actual startup/shutdown paths in `main()`.
- Catches issues where in-process tests might share static state.

**Recommendation**: Start with in-process Tokio task tests (faster iteration). Add one or two process-based tests in a `tests/integration/` binary for smoke testing the real deployment scenario.

---

### 4. Codebase Impact: Other rmcp Dependencies

I searched for integration points. Here's what I found:

**Direct rmcp usage:**
| Location | Usage | Action Required |
|----------|-------|-----------------|
| `main.rs:17-21` | `StreamableHttpService`, `LocalSessionManager` | Replace entirely (Task 5) |
| `server.rs:10-14` | `tool`, `tool_router`, `ServerHandler` | Keep `tool`/`tool_router` for schema generation. Drop `ServerHandler` impl. |
| `server.rs:637` | `Parameters<T>` wrapper | Keep—it's just `struct Parameters<T>(pub T)` |
| `server.rs` | `CallToolResult`, `Content`, `McpError` | Keep—these are pure types |

**Hidden dependencies to check:**
1. **`rmcp::model::InitializeResult`** — You'll need to manually construct this for the `initialize` response.
2. **`rmcp::model::ListToolsResult`** — For `tools/list`. The `#[tool]` macro generates `Tool` definitions. You can call `server.tool_router.list_tools()` if exposed, or manually build from schema.
3. **`rmcp::model::ServerCapabilities`** — Used in `ServerHandler::get_info()`. You'll construct this manually.

**Suggested approach:**
```rust
// In your new handlers
fn handle_initialize() -> InitializeResult {
    InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            tools: Some(ToolsCapability { list_changed: Some(true) }),
            ..Default::default()
        },
        server_info: ServerInfo {
            name: "halfremembered-mcp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    }
}
```

**No other hidden dependencies** — your domain types (`ConversationTree`, `Event`, etc.) are rmcp-free.

---

## Additional Recommendations

### Session Rehydration Strategy

The plan mentions "maybe creates new internal session mapped to ID X". I'd make this explicit:

```rust
enum Session {
    Live {
        server: EventDualityServer,
        sse_tx: broadcast::Sender<SseEvent>,
        created_at: Instant,
    },
    Zombie {
        last_seen: Instant,
        // No active SSE, but we remember it existed
    },
}

// On POST /message with unknown session:
// 1. Check if it's a Zombie → promote to Live
// 2. If truly unknown → 401 with JSON error body
```

This gives you explicit state transitions rather than implicit session creation.

### Error Response Format

For "polite" 401/404, ensure the body is valid JSON-RPC error:
```json
{
  "jsonrpc": "2.0",
  "id": null,
  "error": {
    "code": -32001,
    "message": "Session not found",
    "data": { "session_id": "abc123" }
  }
}
```

Claude Code (and well-behaved clients) will parse this gracefully rather than panicking on HTML error pages.

---

## Summary

| Question | Verdict |
|----------|---------|
| Tool dispatch | Manual match (Option A) — explicit > magic |
| Concurrency | Safe as designed — don't hold guards across await points |
| Testing | Tokio tasks + graceful shutdown — add process tests later |
| Codebase impact | Minimal — keep `rmcp::model`, drop transport |

The plan is ready for implementation. I'd suggest tackling Task 2 (Session state) first since it's the architectural foundation everything else builds on.

---

**Ready to help implement when you are.**

🤖 Claude
