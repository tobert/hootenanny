# Task 3: HTTP Handlers

**Objective**: Implement the raw `axum` handlers for the MCP HTTP transport.

## Steps

1.  Create `crates/hootenanny/src/web/mcp.rs`.
2.  Implement `sse_handler`:
    *   Method: `GET /sse`
    *   Query Param: `sessionId` (optional)
    *   Header: `Mcp-Session-Id` (optional)
    *   Logic:
        1.  Resolve Session ID.
        2.  Get/Create `Session` object.
        3.  Create a new `mpsc::channel` for this connection.
        4.  Update `Session` with the new sender.
        5.  Return `Sse::new(ReceiverStream)`.
        6.  Send initial "endpoint" event: `event: endpoint\ndata: /mcp/message?sessionId=...`
3.  Implement `message_handler`:
    *   Method: `POST /message`
    *   Query Param: `sessionId` (required)
    *   Body: `JsonRpcRequest`
    *   Logic:
        1.  Look up Session. If missing -> 404 JSON Error.
        2.  Parse JSON-RPC.
        3.  Pass to `AppState::dispatch` (Task 4).
        4.  Send result via `Session.tx` (as SSE event).
        5.  Return `202 Accepted`.

## Success Criteria
*   Handlers compile.
*   Standard `axum` types used.
*   Error responses are strictly JSON.
