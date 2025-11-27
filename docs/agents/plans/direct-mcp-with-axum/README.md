# Direct MCP Implementation Plan

**Goal**: Replace the `rmcp` library's transport layer (server implementation) with a direct `axum` implementation in `hootenanny`.

**Why**: 
1.  **Session Resilience**: We need full control over session lifecycles to handle server restarts (dev loops) and network blips without disconnecting agents like Gemini and Claude. `rmcp`'s architecture makes this difficult as it binds sessions to ephemeral in-memory tasks.
2.  **Protocol Control**: We need to ensure "polite" JSON error responses for all HTTP status codes (401, 404) to prevent client crashes.
3.  **Simplicity**: Moving from complex generic traits to explicit HTTP handlers simplifies debugging and observability.

**Strategy**: "Keep the Types, Dump the Engine"
*   **Keep**: `rmcp::model` (JSON-RPC types, Tool definitions).
*   **Replace**: `StreamableHttpService` and `LocalSessionManager`.
*   **Build**: Custom `axum` handlers for `/sse` and `/message` that interact with a persisted Session Store.

## Task Index

*   [Task 1: Test Infrastructure](task_01_test_infrastructure.md) - setup the integration test harness
*   [Task 2: State & Session Management](task_02_state_and_session.md) - define the data structures
*   [Task 3a: Refactor Server](task_03a_refactor_server.md) - split `server.rs` into modules
*   [Task 3b: HTTP Handlers](task_03_http_handlers.md) - implement SSE and POST endpoints
*   [Task 4: Tool Dispatch](task_04_tool_dispatch.md) - wire JSON-RPC methods to application logic
*   [Task 5: Server Wiring](task_05_main_wiring.md) - update `main.rs`
*   [Task 6: Integration Testing](task_06_integration_test.md) - verify resilience