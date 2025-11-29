//! Session Management
//!
//! Handles MCP session lifecycle including creation, resumption, and cleanup.

mod store;

pub use store::{spawn_cleanup_task, InMemorySessionStore, Session, SessionStats, SessionStore};

use axum::response::sse::Event;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::types::protocol::Implementation;

/// SSE event sender type alias.
pub type SseSender = mpsc::Sender<Result<Event, axum::Error>>;

/// A reference to a session (from DashMap).
pub type SessionRef<'a> = dashmap::mapref::one::Ref<'a, String, Session>;

/// A mutable reference to a session (from DashMap).
pub type SessionRefMut<'a> = dashmap::mapref::one::RefMut<'a, String, Session>;

impl Session {
    /// Create a new session with the given ID.
    pub fn new(id: String) -> Self {
        let now = Instant::now();
        Self {
            id,
            created_at: now,
            last_seen: now,
            client_info: None,
            initialized: false,
            tx: None,
        }
    }

    /// Check if the session has an active SSE connection.
    pub fn is_connected(&self) -> bool {
        self.tx.as_ref().map(|tx| !tx.is_closed()).unwrap_or(false)
    }

    /// Get the age of the session since last activity.
    pub fn idle_duration(&self) -> std::time::Duration {
        self.last_seen.elapsed()
    }

    /// Update the last_seen timestamp.
    pub fn touch(&mut self) {
        self.last_seen = Instant::now();
    }

    /// Mark the session as initialized with client info.
    pub fn set_initialized(&mut self, client_info: Implementation) {
        self.initialized = true;
        self.client_info = Some(client_info);
        self.touch();
    }

    /// Register an SSE connection.
    pub fn register_sse(&mut self, tx: SseSender) {
        self.tx = Some(tx);
        self.touch();
    }

    /// Send an SSE event to the client.
    pub async fn send_event(&self, event: Event) -> Result<(), SendError> {
        match &self.tx {
            Some(tx) => {
                tx.send(Ok(event)).await.map_err(|_| SendError::ChannelClosed)
            }
            None => Err(SendError::NotConnected),
        }
    }
}

/// Error when sending an SSE event.
#[derive(Debug, Clone, Copy)]
pub enum SendError {
    /// No SSE connection registered.
    NotConnected,
    /// SSE channel is closed.
    ChannelClosed,
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::NotConnected => write!(f, "session has no SSE connection"),
            SendError::ChannelClosed => write!(f, "SSE channel is closed"),
        }
    }
}

impl std::error::Error for SendError {}
