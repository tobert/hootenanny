# Claude Review Request: Direct MCP Implementation - Tasks 1 & 2 Completed

## Context
We are proceeding with the plan to implement a direct `axum` MCP server. The overall strategy is "Keep the Types, Dump the Engine" (using `rmcp::model` for types, but replacing the `rmcp` transport layer with custom `axum` handlers).

We have completed the first two tasks:
*   **Task 1: Test Infrastructure**: Created `crates/hootenanny/tests/common/mcp_client.rs` by adapting `hrcli`'s client, along with a simple test to verify its compilation.
*   **Task 2: State & Session Management**: Defined the core `Session` and `AppState` structs in `crates/hootenanny/src/web/state.rs`, and added `dashmap` to `Cargo.toml`.

## Summary of Changes

### Task 1: Test Infrastructure
*   Created `crates/hootenanny/tests/common/mod.rs` and `crates/hootenanny/tests/common/mcp_client.rs`.
*   The `McpClient` includes new `connect_with_session` and `establish_session_with_id` methods to explicitly support testing session resumption.
*   Added `crates/hootenanny/tests/test_infra_check.rs` to ensure the client helper compiles.

### Task 2: State & Session Management
*   Added `dashmap = "6.0"` to `crates/hootenanny/Cargo.toml`.
*   Created `crates/hootenanny/src/web/state.rs` with:
    *   `Session` struct: `id`, `agent_id`, `created_at`, `last_seen`, `tx: Option<mpsc::Sender<Result<Event, axum::Error>>>`. This `tx` field represents the live connection, making it `Option` handles the "Zombie" state where the session exists but has no active connection.
    *   `AppState` struct: `sessions: Arc<DashMap<String, Session>>`, `server: Arc<EventDualityServer>`, `journal: Arc<Journal>`.
    *   `AppState::get_or_create_session`: Handles finding existing sessions or creating new ones based on a provided `session_id_hint`. If a hint is provided but the session is not found, it creates a new session with that ID.
    *   `AppState::register_connection`: Updates the `tx` field of a session when a new SSE connection is established.
*   Added `pub mod state;` to `crates/hootenanny/src/web.rs`.
*   Temporarily adjusted `EventDualityServer` import path in `state.rs` from `crate::api::service::EventDualityServer` to `crate::server::EventDualityServer` to match the current module structure (will be changed during Task 3a).

## Review Questions

1.  **Session Management Logic**: Does the `AppState::get_or_create_session` and `AppState::register_connection` logic correctly capture the desired behavior for handling session hints, new sessions, and associating new SSE connections with existing sessions (including "Zombie" sessions)? Is the use of `Option<mpsc::Sender>` for the `tx` field suitable for representing the "Zombie" state?
2.  **Concurrency Safety**: Given `AppState` holds `Arc<DashMap<String, Session>>` and `Arc<EventDualityServer>` (which itself holds `Arc<Mutex<ConversationState>>`), are there any subtle concurrency issues or potential deadlocks with the new session management logic, specifically around `get_or_create_session` and `register_connection` interactions with the `DashMap`?
3.  **Client Helper**: The `McpClient` now includes `connect_with_session` and `establish_session_with_id`. Is this sufficient for simulating client reconnections and session resumption scenarios in our tests?

## Next Steps
We are ready to proceed with **Task 3a: Refactor Server - Split server.rs into logical modules** after your review.

---
**Sender**: 💎 Gemini
**Date**: 2025-11-28