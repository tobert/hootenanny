//! Client connection tracking for bidirectional heartbeats
//!
//! Tracks connected clients (e.g., holler) and monitors their health.
//! Implements the server side of the Paranoid Pirate pattern.

use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Information about a connected client
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// ZMQ identity (routing address)
    pub identity: Bytes,
    /// Service name from Ready command
    pub service: String,
    /// When the client connected
    pub connected_at: Instant,
    /// Last time we received any message from this client
    pub last_seen: Instant,
    /// Number of consecutive heartbeat failures
    pub failures: u32,
}

impl ClientInfo {
    pub fn new(identity: Bytes, service: String) -> Self {
        let now = Instant::now();
        Self {
            identity,
            service,
            connected_at: now,
            last_seen: now,
            failures: 0,
        }
    }
}

/// Tracks connected clients and their health
#[derive(Debug)]
pub struct ClientTracker {
    /// Connected clients by identity
    clients: RwLock<HashMap<Bytes, ClientInfo>>,
    /// How long before a client is considered stale
    stale_threshold: Duration,
    /// Maximum failures before removing a client
    max_failures: u32,
}

impl Default for ClientTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientTracker {
    /// Create a new client tracker with default settings
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            stale_threshold: Duration::from_secs(30),
            max_failures: 3,
        }
    }

    /// Create a client tracker with custom settings
    pub fn with_config(stale_threshold: Duration, max_failures: u32) -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
            stale_threshold,
            max_failures,
        }
    }

    /// Register a new client or update existing one
    pub async fn register(&self, identity: Bytes, service: String) {
        let mut clients = self.clients.write().await;
        if let Some(existing) = clients.get_mut(&identity) {
            // Update existing client
            existing.last_seen = Instant::now();
            existing.failures = 0;
            info!(
                "Client re-registered: {} (service: {})",
                hex_identity(&identity),
                service
            );
        } else {
            // New client
            info!(
                "Client registered: {} (service: {})",
                hex_identity(&identity),
                service
            );
            clients.insert(identity.clone(), ClientInfo::new(identity, service));
        }
    }

    /// Record that we received a message from a client
    pub async fn record_activity(&self, identity: &Bytes) {
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(identity) {
            client.last_seen = Instant::now();
            client.failures = 0;
        }
    }

    /// Record a heartbeat failure for a client
    pub async fn record_failure(&self, identity: &Bytes) -> Option<u32> {
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(identity) {
            client.failures += 1;
            Some(client.failures)
        } else {
            None
        }
    }

    /// Remove a client
    pub async fn remove(&self, identity: &Bytes) {
        let mut clients = self.clients.write().await;
        if clients.remove(identity).is_some() {
            info!("Client removed: {}", hex_identity(identity));
        }
    }

    /// Get all client identities for heartbeating
    pub async fn get_client_identities(&self) -> Vec<Bytes> {
        self.clients.read().await.keys().cloned().collect()
    }

    /// Get connected client count
    pub async fn count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Clean up stale clients that haven't been seen recently
    pub async fn cleanup_stale(&self) -> Vec<String> {
        let now = Instant::now();
        let mut clients = self.clients.write().await;
        let stale: Vec<_> = clients
            .iter()
            .filter(|(_, info)| now.duration_since(info.last_seen) > self.stale_threshold)
            .map(|(id, info)| (id.clone(), info.service.clone()))
            .collect();

        let mut removed = Vec::new();
        for (identity, service) in stale {
            warn!(
                "Removing stale client: {} (service: {})",
                hex_identity(&identity),
                service
            );
            clients.remove(&identity);
            removed.push(service);
        }

        removed
    }

    /// Check if a client has exceeded max failures
    pub async fn should_remove(&self, identity: &Bytes) -> bool {
        let clients = self.clients.read().await;
        clients
            .get(identity)
            .is_some_and(|c| c.failures >= self.max_failures)
    }

    /// Get summary for health endpoint
    pub async fn summary(&self) -> serde_json::Value {
        let clients = self.clients.read().await;
        let now = Instant::now();

        let client_list: Vec<_> = clients
            .values()
            .map(|c| {
                serde_json::json!({
                    "identity": hex_identity(&c.identity),
                    "service": c.service,
                    "connected_secs": now.duration_since(c.connected_at).as_secs(),
                    "last_seen_secs": now.duration_since(c.last_seen).as_secs(),
                    "failures": c.failures,
                })
            })
            .collect();

        serde_json::json!({
            "count": clients.len(),
            "clients": client_list,
        })
    }
}

/// Format identity bytes as hex for logging
fn hex_identity(identity: &Bytes) -> String {
    if identity.len() <= 8 {
        hex::encode(identity)
    } else {
        format!("{}...", hex::encode(&identity[..8]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_registration() {
        let tracker = ClientTracker::new();

        let id1 = Bytes::from_static(b"client1");
        let id2 = Bytes::from_static(b"client2");

        tracker.register(id1.clone(), "holler-1".to_string()).await;
        tracker.register(id2.clone(), "holler-2".to_string()).await;

        assert_eq!(tracker.count().await, 2);

        let identities = tracker.get_client_identities().await;
        assert!(identities.contains(&id1));
        assert!(identities.contains(&id2));
    }

    #[tokio::test]
    async fn test_client_removal() {
        let tracker = ClientTracker::new();

        let id = Bytes::from_static(b"client1");
        tracker.register(id.clone(), "holler".to_string()).await;
        assert_eq!(tracker.count().await, 1);

        tracker.remove(&id).await;
        assert_eq!(tracker.count().await, 0);
    }

    #[tokio::test]
    async fn test_failure_tracking() {
        let tracker = ClientTracker::with_config(Duration::from_secs(30), 3);

        let id = Bytes::from_static(b"client1");
        tracker.register(id.clone(), "holler".to_string()).await;

        // Record failures
        assert_eq!(tracker.record_failure(&id).await, Some(1));
        assert!(!tracker.should_remove(&id).await);

        assert_eq!(tracker.record_failure(&id).await, Some(2));
        assert!(!tracker.should_remove(&id).await);

        assert_eq!(tracker.record_failure(&id).await, Some(3));
        assert!(tracker.should_remove(&id).await);
    }

    #[tokio::test]
    async fn test_activity_resets_failures() {
        let tracker = ClientTracker::with_config(Duration::from_secs(30), 3);

        let id = Bytes::from_static(b"client1");
        tracker.register(id.clone(), "holler".to_string()).await;

        // Record some failures
        tracker.record_failure(&id).await;
        tracker.record_failure(&id).await;

        // Activity should reset failures
        tracker.record_activity(&id).await;

        // Next failure should be back to 1
        assert_eq!(tracker.record_failure(&id).await, Some(1));
    }

    #[tokio::test]
    async fn test_stale_cleanup() {
        // Create tracker with very short stale threshold
        let tracker = ClientTracker::with_config(Duration::from_millis(10), 3);

        let id = Bytes::from_static(b"client1");
        tracker.register(id.clone(), "holler".to_string()).await;

        // Wait for stale threshold
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Cleanup should remove the client
        let removed = tracker.cleanup_stale().await;
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], "holler");
        assert_eq!(tracker.count().await, 0);
    }

    #[tokio::test]
    async fn test_re_registration() {
        let tracker = ClientTracker::new();

        let id = Bytes::from_static(b"client1");

        // Initial registration
        tracker.register(id.clone(), "holler".to_string()).await;

        // Record some failures
        tracker.record_failure(&id).await;
        tracker.record_failure(&id).await;

        // Re-register should reset failures
        tracker.register(id.clone(), "holler".to_string()).await;

        // Should not be removed after one failure
        tracker.record_failure(&id).await;
        assert!(!tracker.should_remove(&id).await);
    }
}

/// Spawn a background task that sends heartbeats to all tracked clients
/// and cleans up stale ones.
#[allow(dead_code)]
pub fn spawn_server_heartbeat_task(
    tracker: Arc<ClientTracker>,
    send_heartbeat: impl Fn(Bytes) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>
        + Send
        + Sync
        + 'static,
    interval: Duration,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        info!(
            "🫀 Server heartbeat task started (interval: {:?})",
            interval
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Get all client identities
                    let identities = tracker.get_client_identities().await;

                    for identity in identities {
                        let success = send_heartbeat(identity.clone()).await;

                        if success {
                            tracker.record_activity(&identity).await;
                            debug!("Heartbeat OK to client {}", hex_identity(&identity));
                        } else if let Some(failures) = tracker.record_failure(&identity).await {
                            warn!(
                                "Heartbeat failed to client {} ({} failures)",
                                hex_identity(&identity),
                                failures
                            );

                            if tracker.should_remove(&identity).await {
                                tracker.remove(&identity).await;
                            }
                        }
                    }

                    // Clean up any stale clients
                    let removed = tracker.cleanup_stale().await;
                    if !removed.is_empty() {
                        debug!("Cleaned up {} stale clients", removed.len());
                    }
                }
                _ = shutdown.recv() => {
                    info!("Server heartbeat task shutting down");
                    break;
                }
            }
        }
    })
}
