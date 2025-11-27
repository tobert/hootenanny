# Gemini Review: rmcp Session Management Hooks Proposal

## Context

We've drafted a proposal for adding optional session lifecycle hooks to rmcp (Rust MCP SDK). The goal is to enable server-side control over session cleanup timing without forcing behavior changes on all rmcp users.

**Proposal Document**: `session-ttl-proposal.md` (same directory)

## Review Request

Please review the proposal and provide feedback on:

1. **Technical soundness**: Is the API design reasonable? Any gotchas?
2. **Upstream viability**: Would rmcp maintainers likely accept this?
3. **Alternative approaches**: Are there better ways to achieve this?
4. **Implementation concerns**: What could go wrong?
5. **Documentation gaps**: What's missing or unclear?

---

## Gemini's Response

[Gemini output will be appended here]
# Review: Session TTL Proposal

Here is the requested review of `docs/agents/plans/08-rmcp-session-resilience/session-ttl-proposal.md`.

## 1. Technical Soundness

**Rating: Mixed (Concept is sound, proposed implementation has a critical race condition)**

The goal of allowing delayed cleanup is sound and necessary for the described use case. However, the proposed "callback with cleanup closure" mechanism introduces a critical concurrency bug:

*   **The Reconnection Race Condition:**
    1.  Client A disconnects. `on_session_disconnect` spawns a task to wait 5 minutes, then call `cleanup()`.
    2.  Client A reconnects immediately with the **same Session ID** (restoring the session). The server updates the `tx_store` with the new connection's sender.
    3.  5 minutes pass.
    4.  The spawned task wakes up and calls `cleanup()`.
    5.  **Result:** The `cleanup()` closure blindly removes the `session_id` from the map, **killing the active, reconnected session.**

**Fix Requirement:** The `cleanup` function must be conditional. It needs to verify that the session currently in the store is still the *same* disconnected session (e.g., by comparing channel IDs, generation counters, or checking if the channel is closed) before removing it. The current proposal's simple `txs.remove(&session_id)` is destructive.

## 2. Likelihood of Upstream Acceptance

**Rating: Moderate to Low (in current form)**

*   **Pros:** The `rmcp` maintainers likely understand the need for session resilience. The change is opt-in and keeps the API surface small.
*   **Cons:**
    *   **Safety:** The race condition mentioned above makes it unsafe for the very use case it targets (reconnection).
    *   **Leak Potential:** If the user implementation panics or drops the `cleanup_fn` without calling it, the session leaks forever. Rust APIs generally try to make "leaking resources" harder than this.
    *   **Inversion of Control:** Passing a closure that locks internal state (`tx_store`) to user-land async code might be frowned upon due to deadlock risks, although `tokio::sync::RwLock` mitigates some of this.

## 3. Alternative Approaches

1.  **Session Store Trait (Option 2 in proposal):**
    *   *Verdict:* Superior. By abstracting `SessionStore`, the server implementation can handle `remove` logic atomically. The implementation could check `if entry.generation == my_generation { remove }`.
    *   *Trade-off:* Higher complexity to implement.

2.  **Expose "Session State" Object:**
    *   Instead of a raw `FnOnce`, pass a `SessionHandle` that exposes methods like `is_connected()`, `force_disconnect()`, and `mark_stale()`.

3.  **Active "Keep-Alive" Logic:**
    *   Instead of a disconnect hook, the server could have a `prune_interval` and a `SessionStore` that marks sessions as "detached" with a timestamp. The prune logic removes them after $TTL. This is a more standard pattern for session management than ad-hoc async spawns.

## 4. Implementation Concerns

*   **Client Reconnection Logic:** The proposal assumes the client *can* reconnect with the same ID. Does `rmcp`'s client / SSE implementation actually support sending a requested `sessionId` on connect? If the server generates a *new* ID for every connection request, this entire proposal is moot. This must be verified in `rmcp` code.
*   **Cancellation:** If the server shuts down (`ct` cancellation token triggers), what happens to the pending 5-minute wait tasks? They might block clean shutdown or be aborted abruptly, potentially leaving state inconsistent depending on where the persistence layer lives.
*   **Lock Contention:** If 1000 agents disconnect simultaneously, acquiring the write lock in 1000 separate cleanup tasks might cause spikes in latency for remaining active users.

## 5. Documentation Gaps

*   **Reconnection Mechanics:** The document states "Client Reconnection Works" but implies the session ID changes. It does not explicitly document *how* a client should request to resume a specific session ID (e.g., `?sessionId=...` query param or `Last-Event-ID` header).
*   **Safety Contract:** The requirement "Hook must call cleanup() or session leaks" is a heavy burden. It needs to be printed in bold warnings.
*   **Type Definitions:** The signature `Box<dyn Fn(SessionId, SessionCleanupFn) + Send + Sync>` is complex. The proposal should suggest a type alias in the public API to make it readable.

---

### Summary Recommendation

**Do not submit Option 1 as currently written.**

Refine the proposal to **Option 2 (Session Store Trait)** or modify Option 1 to include a **Generation ID / Connection ID**.
*   **Modified Option 1:** The `cleanup` function should return a `Result` indicating if it actually removed something, or it should internally check "Is the current connection still the one I was created for?".

**Next Step:** Verify if `rmcp` allows clients to dictate their Session ID on connection. If not, that feature is a prerequisite.
