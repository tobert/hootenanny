# Task 2: State & Session Management

**Objective**: Define the data structures that will hold the application state and user sessions, independent of the network transport.

## Dependencies
*   Add `dashmap = "6.0"` to `crates/hootenanny/Cargo.toml`.

## Steps

1.  Create `crates/hootenanny/src/web/state.rs`.
2.  Define `Session` struct:
    ```rust
    pub struct Session {
        pub id: String,
        pub agent_id: Option<String>,
        pub created_at: std::time::SystemTime,
        pub last_seen: std::time::SystemTime,
        // Channel to send SSE events to the active connection.
        // If None, session is "Zombie" (valid but disconnected).
        pub tx: Option<tokio::sync::mpsc::Sender<Result<axum::response::sse::Event, axum::Error>>>, 
    }
    ```
3.  Define `AppState` struct:
    ```rust
    pub struct AppState {
        // Concurrent map for sessions
        pub sessions: Arc<DashMap<String, Session>>,
        
        // The core application logic
        pub server: Arc<EventDualityServer>,
        
        // Persistence layer
        pub journal: Arc<Journal>,
    }
    ```
4.  Implement `AppState` methods:
    *   `new(server, journal) -> Self`
    *   `get_session(id) -> Option<SessionRef>`
    *   `register_connection(session_id, sender) -> Result<()>`
        *   If session exists, update `tx` and `last_seen`.
        *   If session missing (server restart?), try to rehydrate from Journal (future) or create fresh.
        *   **Critical**: Ensure no deadlocks. Do not hold DashMap locks while acquiring Server locks.

## Success Criteria
*   Structs defined and compiling.
*   `dashmap` added to Cargo.toml.
*   Clear separation between "Session Data" and "Active Connection".