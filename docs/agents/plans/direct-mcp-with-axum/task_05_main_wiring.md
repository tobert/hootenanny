# Task 5: Server Wiring

**Objective**: Update the application entry point to use the new architecture.

## Steps

1.  Edit `crates/hootenanny/src/main.rs`.
2.  Initialize `AppState`.
3.  Create Router:
    ```rust
    let app = Router::new()
        .route("/mcp/sse", get(web::mcp::sse_handler))
        .route("/mcp/message", post(web::mcp::message_handler))
        // ... existing web routes
        .with_state(app_state);
    ```
4.  Remove `rmcp` server setup code (`StreamableHttpService`).
5.  Ensure graceful shutdown flushes the `AppState` / `Journal`.

## Success Criteria
*   `cargo run` starts the server.
*   Server listens on port.
