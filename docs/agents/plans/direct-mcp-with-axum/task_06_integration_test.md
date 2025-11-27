# Task 6: Integration Testing

**Objective**: Prove that the system is resilient to restarts using a black-box test with graceful shutdown.

## Steps

1.  Create `crates/hootenanny/tests/mcp_resilience.rs`.
2.  Define `start_server_fixture(db_path: PathBuf) -> (SocketAddr, tokio::sync::oneshot::Sender<()>, JoinHandle<()>)`.
    *   This function starts `hootenanny::main` logic (refactored to be callable) on port 0 (random).
    *   Returns the bound port, a shutdown signal sender, and the task handle.
    *   Ensures `sled` flushes on shutdown.
3.  **Test Case 1: Happy Path**
    *   Start fixture.
    *   `McpClient::connect`.
    *   `call_tool("play")` -> OK.
    *   Shutdown fixture.
4.  **Test Case 2: The Zombie Session**
    *   `let db_path = tempdir()`.
    *   **Run 1**: Start fixture at `db_path`. Connect client. Get `SessionID: X`. Call `add_node`. Shutdown fixture (wait for exit).
    *   **Run 2**: Start *new* fixture at `db_path`.
    *   Reuse `McpClient` with `SessionID: X` (manually set).
    *   Call `play`.
    *   **Expectation**: Server accepts request. Logic works. 202 Accepted. Result comes via SSE.

## Success Criteria
*   Tests pass reliably.
*   No "Database Locked" errors from sled (proof of graceful shutdown).