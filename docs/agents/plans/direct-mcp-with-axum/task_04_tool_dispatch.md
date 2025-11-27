# Task 4: Tool Dispatch

**Objective**: Wire the JSON-RPC requests to the actual methods in `EventDualityServer` using **Manual Dispatch** (Option A from Claude's review).

## Steps

1.  Create `crates/hootenanny/src/web/dispatch.rs` (or inside `mcp.rs`).
2.  Implement `handle_mcp_request(server: &EventDualityServer, request: JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError>`.
3.  Implement `handle_tool_call(server: &EventDualityServer, name: &str, args: Value) -> Result<CallToolResult, McpError>`.
    *   Use a large `match` statement for explicit control.
    ```rust
    match name {
        "play" => {
             let params: AddNodeRequest = serde_json::from_value(args)?;
             server.play(Parameters(params)).await
        },
        "add_node" => {
             let params: AddNodeRequest = serde_json::from_value(args)?;
             server.add_node(Parameters(params)).await
        },
        // ... map all tools explicitly
        _ => Err(McpError::method_not_found(name))
    }
    ```
4.  Implement `handle_initialize` manually (since we are dropping `ServerHandler` trait).
    *   Return `InitializeResult` with hardcoded server capabilities.

## Success Criteria
*   All tools (`play`, `add_node`, `orpheus_*`, `cas_*`) are reachable.
*   Compilation succeeds without `rmcp` macros doing the routing.