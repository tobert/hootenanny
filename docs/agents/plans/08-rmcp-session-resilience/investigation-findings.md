# Investigation Findings: rmcp Session Mechanics

## Question
Can clients reconnect to rmcp SSE server with the same session ID?

## Answer
**NO** - The current implementation always generates new session IDs.

## Evidence

### Server Side (`sse_server.rs:85-90`)
```rust
async fn sse_handler(
    State(app): State<App>,
    nested_path: Option<Extension<NestedPath>>,
    parts: Parts,
) -> Result<Sse<impl Stream<Item = Result<Event, io::Error>>>, Response<String>> {
    let session = session_id();  // ❌ ALWAYS generates new UUID
    tracing::info!(%session, ?parts, "sse connection");
    // ...
}
```

The server **ignores** any session ID the client might send. It unconditionally calls `session_id()` which returns `Uuid::new_v4()`.

### Client Side (`streamable_http_client.rs:27-37`)
```rust
async fn get_stream(
    &self,
    uri: Arc<str>,
    session_id: Arc<str>,  // Client CAN send a session ID
    last_event_id: Option<String>,
    auth_token: Option<String>,
) -> ... {
    let mut request_builder = self
        .get(uri.as_ref())
        .header(HEADER_SESSION_ID, session_id.as_ref());  // Sent as header
    // ...
}
```

Clients **do** send session IDs in headers, but the SSE server doesn't read them.

### Client Receives New ID
```rust
let session_id = response.headers().get(HEADER_SESSION_ID);  // Line 130
let session_id = session_id
    .and_then(|v| v.to_str().ok())
    .map(|s| s.to_string());
```

After connecting, the client reads the **new** session ID from response headers.

## Implications for Our Proposal

### ❌ Original TTL Idea is Flawed

Our proposal assumed:
1. Client disconnects with session ID `abc-123`
2. Server delays cleanup for 5 minutes
3. Client reconnects with `abc-123`
4. Session resumes

**Reality**:
1. Client disconnects with session ID `abc-123`
2. Server delays cleanup for 5 minutes
3. Client reconnects → server assigns `xyz-789` (new UUID)
4. Client uses new ID, old session leaks until TTL expires

### ✅ But Wait - There's Hope!

Our **actual use case** doesn't need session ID resumption:

**What we ACTUALLY observed:**
- Server restarts
- Client reconnects automatically
- Gets **new** session ID
- All jobs/artifacts/conversations persist (they're in shared Arc<> stores)
- Only in-flight request/response pairs are lost

**The real problem:**
- HTTP 410 Gone error when client POSTs with stale session ID
- Client needs to detect this and reconnect to get new ID

## The Real Solution

We don't need session TTL. We need:

### Option A: Graceful 410 Handling
When POST receives unknown session ID:
1. Return 410 Gone with header `X-Reconnect-To: /sse`
2. Client detects 410 → reconnects → gets new ID → retries

### Option B: Accept Any Session ID (Stateless)
Since all real state (jobs, artifacts) is in shared stores that aren't tied to session IDs:
1. Accept POST with **any** session ID
2. Create response channel on-demand
3. Only reject if can't route response (no active SSE stream for that ID)

### Option C: Support Client-Provided Session IDs
Modify SSE handler to:
1. Check for `X-Session-ID` header in request
2. If present and valid, reuse it
3. Otherwise generate new UUID

This would enable true session resumption.

## Gemini's Race Condition

Gemini was right about the race condition, but it's based on a false assumption (that sessions can be resumed). Since they can't, the race doesn't exist in practice.

**However**, if we implement Option C, the race DOES become real and we'd need generation counters.

## Recommendation

**Don't pursue session TTL.** Instead:

1. **Immediate**: Document the reconnection pattern for clients
2. **Better**: Improve 410 error responses (Option A)
3. **Best**: Support client-provided session IDs (Option C) + generation counters

For our multi-agent ensemble:
- Jobs persist across reconnects (Arc<JobStore>)
- Artifacts persist (Arc<ArtifactStore>)
- Conversations persist (Arc<Mutex<ConversationState>>)
- Only need clients to handle 410 gracefully

## Next Steps

1. Update proposal to focus on **graceful reconnection** not TTL
2. Propose 410 error improvements
3. Optionally propose client-controlled session IDs as enhancement
4. Document current behavior clearly
