use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;
use axum::response::sse::Event;
use uuid::Uuid;
use tracing::{self, instrument}; // Add tracing import

use crate::persistence::journal::Journal;
use crate::api::service::EventDualityServer;

/// Represents an active or recently active client session.
pub struct Session {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    #[allow(dead_code)]
    pub created_at: SystemTime,
    pub last_seen: SystemTime,
    // Channel to send SSE events to the active connection.
    // If None, the session is "Zombie" (valid but currently disconnected).
    pub tx: Option<mpsc::Sender<Result<Event, axum::Error>>>,
}

impl Session {
    pub fn new(id: String) -> Self {
        let now = SystemTime::now();
        Session {
            id,
            agent_id: None,
            created_at: now,
            last_seen: now,
            tx: None,
        }
    }
}

/// Shared application state, including active sessions and core services.
pub struct AppState {
    /// Concurrent map for sessions
    pub sessions: Arc<DashMap<String, Session>>,

    /// The core application logic server
    pub server: Arc<EventDualityServer>,

    /// Persistence layer (for journal, conversation state, etc.)
    #[allow(dead_code)]
    pub journal: Arc<Journal>,
}

impl AppState {
    pub fn new(server: Arc<EventDualityServer>, journal: Arc<Journal>) -> Self {
        AppState {
            sessions: Arc::new(DashMap::new()),
            server,
            journal,
        }
    }

    /// Retrieves an existing session or creates a new one if specified by ID.
    /// If an ID is provided and no session exists for it, a new one is created.
    /// If no ID is provided, a new session is always created.
    #[instrument(skip(self), fields(session_id_hint = ?session_id_hint))]
    pub fn get_or_create_session(&self, session_id_hint: Option<String>) -> String {
        match session_id_hint {
            Some(hint_id) => {
                match self.sessions.entry(hint_id.clone()) {
                    Entry::Occupied(mut entry) => {
                        let session = entry.get_mut();
                        session.last_seen = SystemTime::now();
                        tracing::info!(session_id = %hint_id, "Reactivating existing session");
                        hint_id
                    },
                    Entry::Vacant(entry) => {
                        let session = Session::new(hint_id.clone());
                        tracing::info!(session_id = %hint_id, "Created new session with hinted ID (may be resurrection)");
                        entry.insert(session);
                        hint_id
                    },
                }
            },
            None => {
                let new_id = Uuid::new_v4().to_string();
                let session = Session::new(new_id.clone());
                tracing::info!(session_id = %new_id, "Created brand new session");
                self.sessions.insert(new_id.clone(), session);
                new_id
            }
        }
    }

    /// Registers a new SSE connection for a given session ID.
    /// This updates the `tx` field of an existing session.
    #[instrument(skip(self, sender), fields(session_id = %session_id))]
    pub fn register_connection(&self, session_id: &str, sender: mpsc::Sender<Result<Event, axum::Error>>) {
        if let Some(mut session_entry) = self.sessions.get_mut(session_id) {
            session_entry.tx = Some(sender);
            session_entry.last_seen = SystemTime::now();
            tracing::info!("Registered new SSE connection for session");
        } else {
            // This indicates a logical error in the flow, as get_or_create_session should
            // have been called prior to this to ensure the session exists.
            tracing::error!("Attempted to register connection for non-existent session ID. This should not happen.");
            // We could create it here as a fallback, but it's better to fail early and understand why.
        }
    }
}