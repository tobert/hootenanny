//! Session Store
//!
//! Trait and in-memory implementation for session storage.
//!
//! Implements OpenTelemetry spans for full session lifecycle observability:
//! - `mcp.session.create` - Session creation (new or resumed)
//! - `mcp.session.touch` - Activity tracking
//! - `mcp.session.sse_register` - SSE channel registration
//! - `mcp.session.initialized` - Client initialization complete
//! - `mcp.session.expire` - Session expiration (cleanup)
//! - `mcp.session.terminate` - Explicit session termination

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::SseSender;
use crate::types::logging::LogLevel;
use crate::types::protocol::Implementation;
use std::collections::HashSet;

/// An MCP session.
#[derive(Debug)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,

    /// When the session was created.
    pub created_at: Instant,

    /// Last activity timestamp.
    pub last_seen: Instant,

    /// Client implementation info (set after initialize).
    pub client_info: Option<Implementation>,

    /// Client capabilities (set after initialize).
    pub client_capabilities: Option<crate::types::protocol::ClientCapabilities>,

    /// Whether the session has completed initialization.
    pub initialized: bool,

    /// SSE channel sender (None if disconnected).
    pub tx: Option<SseSender>,

    /// Client's requested log level.
    pub log_level: LogLevel,

    /// Resources this session is subscribed to.
    pub subscriptions: HashSet<String>,
}

/// Statistics about active sessions.
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    /// Total number of sessions.
    pub total: usize,
    /// Sessions with active SSE connections.
    pub connected: usize,
    /// Sessions without active connections (zombies).
    pub disconnected: usize,
}

/// Session store trait for pluggable storage backends.
pub trait SessionStore: Send + Sync {
    /// Get an existing session or create a new one.
    /// If `id_hint` is provided and exists, returns that session.
    /// If `id_hint` is provided but doesn't exist, creates with that ID.
    /// If `id_hint` is None, generates a new UUID.
    fn get_or_create(&self, id_hint: Option<&str>) -> String;

    /// Get a session by ID (read-only).
    fn get(&self, id: &str) -> Option<super::SessionRef<'_>>;

    /// Get a session by ID (mutable).
    fn get_mut(&self, id: &str) -> Option<super::SessionRefMut<'_>>;

    /// Update the last_seen timestamp.
    fn touch(&self, id: &str);

    /// Mark a session as initialized.
    fn set_initialized(&self, id: &str, client_info: Implementation);

    /// Set client capabilities for a session.
    fn set_capabilities(&self, id: &str, capabilities: crate::types::protocol::ClientCapabilities);

    /// Register an SSE connection for a session.
    fn register_sse(&self, id: &str, tx: SseSender);

    /// Remove sessions older than the given TTL.
    /// Returns the number of sessions removed.
    fn cleanup(&self, max_idle: Duration) -> usize;

    /// Remove a specific session by ID.
    fn remove(&self, id: &str);

    /// Get session statistics.
    fn stats(&self) -> SessionStats;
}

/// In-memory session store using DashMap.
#[derive(Debug)]
pub struct InMemorySessionStore {
    sessions: DashMap<String, Session>,
}

impl InMemorySessionStore {
    /// Create a new in-memory session store.
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    /// Create a new store wrapped in Arc for sharing.
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore for InMemorySessionStore {
    fn get_or_create(&self, id_hint: Option<&str>) -> String {
        match id_hint {
            Some(hint) => {
                // Try to get existing or create with hinted ID
                let id = hint.to_string();
                let is_new = !self.sessions.contains_key(&id);

                self.sessions
                    .entry(id.clone())
                    .or_insert_with(|| Session::new(hint.to_string()));

                // Log session creation/resumption with span
                let _span = tracing::info_span!(
                    "mcp.session.create",
                    mcp.session_id = %id,
                    mcp.session.is_new = %is_new,
                )
                .entered();

                if is_new {
                    tracing::info!("Created new session");
                } else {
                    tracing::debug!("Resumed existing session");
                }

                id
            }
            None => {
                // Generate new UUID
                let id = Uuid::new_v4().to_string();
                let session = Session::new(id.clone());

                let _span = tracing::info_span!(
                    "mcp.session.create",
                    mcp.session_id = %id,
                    mcp.session.is_new = true,
                )
                .entered();

                tracing::info!("Created new session with generated ID");
                self.sessions.insert(id.clone(), session);
                id
            }
        }
    }

    fn get(&self, id: &str) -> Option<super::SessionRef<'_>> {
        self.sessions.get(id)
    }

    fn get_mut(&self, id: &str) -> Option<super::SessionRefMut<'_>> {
        self.sessions.get_mut(id)
    }

    fn touch(&self, id: &str) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.touch();
        }
    }

    fn set_initialized(&self, id: &str, client_info: Implementation) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            tracing::info!(
                session_id = %id,
                client_name = %client_info.name,
                client_version = %client_info.version,
                "Session initialized"
            );
            session.set_initialized(client_info);
        }
    }

    fn set_capabilities(&self, id: &str, capabilities: crate::types::protocol::ClientCapabilities) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            let supports_sampling = capabilities.sampling.is_some();
            tracing::debug!(
                session_id = %id,
                supports_sampling = supports_sampling,
                "Client capabilities registered"
            );
            session.set_capabilities(capabilities);
        }
    }

    fn register_sse(&self, id: &str, tx: SseSender) {
        if let Some(mut session) = self.sessions.get_mut(id) {
            session.register_sse(tx);
            tracing::info!(session_id = %id, "Registered SSE connection");
        }
    }

    fn cleanup(&self, max_idle: Duration) -> usize {
        let mut to_remove = Vec::new();

        for entry in self.sessions.iter() {
            let session = entry.value();
            let is_connected = session.is_connected();
            let idle = session.idle_duration();

            // Connected sessions get longer TTL, disconnected get shorter
            let effective_ttl = if is_connected {
                max_idle
            } else {
                max_idle / 6 // ~5 minutes if max_idle is 30 minutes
            };

            if idle > effective_ttl {
                to_remove.push(entry.key().clone());
            }
        }

        let removed = to_remove.len();
        for id in to_remove {
            if self.sessions.remove(&id).is_some() {
                tracing::info!(session_id = %id, "Removed stale session");
            }
        }

        if removed > 0 {
            tracing::info!(
                removed = removed,
                remaining = self.sessions.len(),
                "Session cleanup completed"
            );
        }

        removed
    }

    fn remove(&self, id: &str) {
        if self.sessions.remove(id).is_some() {
            tracing::info!(session_id = %id, "Session removed");
        }
    }

    fn stats(&self) -> SessionStats {
        let mut connected = 0;
        let mut disconnected = 0;

        for entry in self.sessions.iter() {
            if entry.value().is_connected() {
                connected += 1;
            } else {
                disconnected += 1;
            }
        }

        SessionStats {
            total: self.sessions.len(),
            connected,
            disconnected,
        }
    }
}

/// Spawn a background task that periodically cleans up stale sessions.
pub fn spawn_cleanup_task(
    store: Arc<dyn SessionStore>,
    interval: Duration,
    max_idle: Duration,
    cancel: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("Session cleanup task shutting down");
                    break;
                }
                _ = ticker.tick() => {
                    store.cleanup(max_idle);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let store = InMemorySessionStore::new();
        let id = store.get_or_create(None);
        assert!(!id.is_empty());

        let stats = store.stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.disconnected, 1);
    }

    #[test]
    fn test_session_with_hint() {
        let store = InMemorySessionStore::new();
        let id = store.get_or_create(Some("my-session-id"));
        assert_eq!(id, "my-session-id");

        // Getting with same hint should return same session
        let id2 = store.get_or_create(Some("my-session-id"));
        assert_eq!(id2, "my-session-id");

        let stats = store.stats();
        assert_eq!(stats.total, 1);
    }

    #[test]
    fn test_session_touch() {
        let store = InMemorySessionStore::new();
        let id = store.get_or_create(None);

        // Small sleep to ensure time passes
        std::thread::sleep(std::time::Duration::from_millis(10));

        let before = store.get(&id).unwrap().idle_duration();
        store.touch(&id);
        let after = store.get(&id).unwrap().idle_duration();

        assert!(after < before);
    }

    #[test]
    fn test_session_initialize() {
        let store = InMemorySessionStore::new();
        let id = store.get_or_create(None);

        assert!(!store.get(&id).unwrap().initialized);

        store.set_initialized(&id, Implementation::new("test", "1.0"));

        let session = store.get(&id).unwrap();
        assert!(session.initialized);
        assert_eq!(session.client_info.as_ref().unwrap().name, "test");
    }

    #[test]
    fn test_cleanup_removes_old_sessions() {
        let store = InMemorySessionStore::new();
        let _id = store.get_or_create(None);

        // Immediate cleanup with 0 TTL should remove the session
        let removed = store.cleanup(Duration::ZERO);
        assert_eq!(removed, 1);
        assert_eq!(store.stats().total, 0);
    }

    #[test]
    fn test_cleanup_keeps_recent_sessions() {
        let store = InMemorySessionStore::new();
        let _id = store.get_or_create(None);

        // Cleanup with long TTL should keep the session
        let removed = store.cleanup(Duration::from_secs(3600));
        assert_eq!(removed, 0);
        assert_eq!(store.stats().total, 1);
    }
}
