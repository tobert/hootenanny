# SSE MCP Protocol - Implementation Notes

**Date**: 2025-11-16
**Session**: 5
**Author**: Claude

## Problem Discovered

The `hrcli` CLI client was built with incorrect assumptions about SSE MCP protocol.

### Wrong Assumption ❌
- POST request to `/message?sessionId=<id>` with JSON-RPC
- Response comes back in HTTP response body
- Simple request/response pattern

### Actual Protocol ✅
- POST request to `/message?sessionId=<id>` → Server returns **`202 Accepted`** with **empty body**
- **Actual JSON-RPC response arrives via the SSE stream**
- Bidirectional communication pattern

## How SSE MCP Works

```
┌─────────┐                           ┌─────────┐
│ Client  │                           │ Server  │
└────┬────┘                           └────┬────┘
     │                                     │
     │ GET /sse                            │
     ├────────────────────────────────────>│
     │                                     │
     │ event: endpoint                     │
     │ data: /message?sessionId=<uuid>     │
     │<────────────────────────────────────┤
     │                                     │
     │ [Keep SSE connection OPEN]          │
     │<═══════════════════════════════════>│
     │                                     │
     │ POST /message?sessionId=<uuid>      │
     │ {"jsonrpc":"2.0","id":1,...}        │
     ├────────────────────────────────────>│
     │                                     │
     │ HTTP 202 Accepted (empty body)      │
     │<────────────────────────────────────┤
     │                                     │
     │ event: message                      │
     │ data: {"jsonrpc":"2.0","id":1,...}  │
     │<════════════════════════════════════┤ (via SSE stream!)
     │                                     │
```

## Test Evidence

```bash
$ cargo run -p hrcli -- list-tools

DEBUG: POST to http://127.0.0.1:8080/message?sessionId=05c2ee00-22d2-43e4-a79f-26896855fa9f
DEBUG: Request: {
  "id": 0,
  "jsonrpc": "2.0",
  "method": "initialize",
  "params": { ... }
}
DEBUG: Response status: 202 Accepted  ← Server accepted request
DEBUG: Response body:                 ← But body is EMPTY!
```

The response arrives on the SSE stream, not in the HTTP body.

## Required Architecture

### Current (Broken)
```rust
async fn send_request(&self, session_id: &str, request: Value) -> Result<Value> {
    let response = self.client.post(url).body(request).send().await?;
    let body = response.text().await?;  // ❌ Body is empty!
    serde_json::from_str(&body)         // ❌ Fails to parse
}
```

### Needed (Correct)
```rust
struct McpClient {
    session_id: String,
    http_client: reqwest::Client,
    sse_task: JoinHandle<()>,           // ✅ Background SSE listener
    response_rx: mpsc::Receiver<Value>, // ✅ Responses from SSE
    request_tx: mpsc::Sender<Value>,    // ✅ Send requests via channel
}

async fn send_request(&self, request: Value) -> Result<Value> {
    // 1. POST request (gets 202 Accepted)
    self.http_client.post(url).body(request).send().await?;

    // 2. Wait for response to arrive via SSE stream
    let response = self.response_rx.recv().await?; // ✅ From SSE!

    Ok(response)
}

// Background task
async fn sse_listener_task(stream: EventStream, response_tx: mpsc::Sender<Value>) {
    while let Some(event) = stream.next().await {
        if event.event == "message" {
            let response: Value = serde_json::from_str(&event.data)?;
            response_tx.send(response).await?; // ✅ Send to main thread
        }
    }
}
```

## Implementation Steps

1. **Refactor `McpClient::establish_session()`**
   - Keep SSE connection open instead of closing it
   - Spawn background task to listen for events
   - Set up mpsc channels for coordination

2. **Parse SSE events properly**
   - Look for `event: message` lines
   - Extract `data:` JSON payload
   - Parse as JSON-RPC response

3. **Match responses to requests**
   - Use JSON-RPC `id` field to correlate
   - Could use HashMap<id, oneshot::Sender> for multiple concurrent requests

4. **Handle edge cases**
   - SSE connection drops → reconnect
   - Request timeout → return error
   - Server errors → propagate

## References

- Server code: `crates/hootenanny/src/main.rs` - SSE server setup with rmcp
- Current client: `crates/hrcli/src/mcp_client.rs` - needs refactor
- rmcp library: Uses SSE for bidirectional JSON-RPC over HTTP

## Next Session TODO

1. Read rmcp source to understand SSE event format
2. Implement background SSE listener task
3. Use channels to coordinate POST requests with SSE responses
4. Test with `list-tools` first
5. Then test all 4 musical tools

---

**Status**: Foundation complete, needs SSE bidirectional implementation
**Complexity**: Medium (async coordination with channels)
**Estimated time**: 2-3 hours for complete implementation
