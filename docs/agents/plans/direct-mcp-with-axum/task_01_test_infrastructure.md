# Task 1: Test Infrastructure

**Objective**: Create a reusable `McpClient` test helper in `hootenanny`'s test suite.

We need a way to talk to our server using the exact protocol (SSE + POST) that agents use. We will adapt the code from `crates/hrcli/src/mcp_client.rs` into a test module.

## Steps

1.  Create directory `crates/hootenanny/tests/common/`.
2.  Create `crates/hootenanny/tests/common/mod.rs` (if needed).
3.  Create `crates/hootenanny/tests/common/mcp_client.rs`.
4.  Copy the logic from `hrcli::mcp_client`, removing CLI-specific dependencies if any.
    *   Ensure it exposes `connect(url)`, `call_tool`, `list_tools`.
    *   Ensure it allows specifying/reusing a `session_id` (critical for resilience testing).
5.  Add necessary dev-dependencies to `crates/hootenanny/Cargo.toml` (`reqwest`, `tokio`, `serde_json`, etc.).

## Success Criteria
*   `crates/hootenanny/tests/common/mcp_client.rs` compiles.
*   It can be imported in a dummy test.
