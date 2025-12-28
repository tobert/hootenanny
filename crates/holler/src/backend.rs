//! Backend connection to hootenanny using shared HootClient
//!
//! Uses hooteproto's HootClient for lazy connection, reconnection, and request correlation.

use anyhow::Result;
use hooteproto::{ClientConfig, ConnectionState, HootClient, Payload};
use std::sync::Arc;
use tracing::info;

/// Pool of backend connections
///
/// Simplified to only connect to hootenanny, which proxies to vibeweaver and chaosgarden.
pub struct BackendPool {
    pub hootenanny: Option<Arc<HootClient>>,
    /// Stored config for client recreation after Dead state
    hootenanny_config: Option<ClientConfig>,
}

impl BackendPool {
    /// Create a new empty pool
    pub fn new() -> Self {
        Self {
            hootenanny: None,
            hootenanny_config: None,
        }
    }

    /// Set up Hootenanny backend.
    ///
    /// ZMQ's connect() is non-blocking, so the peer doesn't need to exist.
    /// The server can start immediately without waiting for the backend.
    pub async fn setup_hootenanny(&mut self, endpoint: &str, timeout_ms: u64) {
        let config = ClientConfig::new("hootenanny", endpoint).with_timeout(timeout_ms);
        self.hootenanny_config = Some(config.clone()); // Store for recreation
        let client = HootClient::new(config).await;
        self.hootenanny = Some(client);
    }

    /// Check if hootenanny client is dead and needs recreation.
    ///
    /// This returns true when the client has exhausted retries and is marked Dead.
    /// The client should be recreated to get a fresh socket and clear pending state.
    pub fn needs_recreation(&self) -> bool {
        self.hootenanny
            .as_ref()
            .map(|c| c.health.get_state() == ConnectionState::Dead)
            .unwrap_or(false)
    }

    /// Recreate the hootenanny client after sustained failures.
    ///
    /// This should be called when `needs_recreation()` returns true.
    /// Creates a fresh client with the stored config.
    pub async fn recreate_hootenanny(&mut self) -> Result<()> {
        let config = self
            .hootenanny_config
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No config stored for recreation"))?;

        // Take the old client (if any) to ensure it's dropped
        if let Some(old_client) = self.hootenanny.take() {
            info!(
                "Dropping old hootenanny client (failures={})",
                old_client.health.get_failures()
            );
            // Arc will be dropped, triggering reactor shutdown
            drop(old_client);
        }

        // Create new client with fresh socket
        info!("Recreating hootenanny client for {}", config.endpoint);
        let client = HootClient::new(config).await;
        self.hootenanny = Some(client);

        Ok(())
    }

    /// Route all tool calls to hootenanny
    pub fn route_tool(&self, _tool_name: &str) -> Option<Arc<HootClient>> {
        self.hootenanny.clone()
    }

    /// Send a request to hootenanny
    pub async fn request(&self, payload: Payload) -> Result<Payload> {
        let client = self
            .hootenanny
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Hootenanny backend not configured"))?;
        client.request(payload).await
    }

    /// Send a request with traceparent to hootenanny
    pub async fn request_with_trace(
        &self,
        payload: Payload,
        traceparent: Option<String>,
    ) -> Result<Payload> {
        let client = self
            .hootenanny
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Hootenanny backend not configured"))?;
        client.request_with_trace(payload, traceparent).await
    }

    /// Get health status of hootenanny
    pub async fn health(&self) -> serde_json::Value {
        let mut backends = serde_json::Map::new();

        if let Some(ref client) = self.hootenanny {
            backends.insert("hootenanny".to_string(), client.health.health_summary().await);
        }

        serde_json::Value::Object(backends)
    }

    /// Check if hootenanny is alive
    pub fn all_alive(&self) -> bool {
        self.hootenanny
            .as_ref()
            .map(|c| c.health.is_alive())
            .unwrap_or(true)
    }

    /// Spawn health monitoring task for hootenanny with callback on connect
    pub fn spawn_health_task(
        &self,
        shutdown: tokio::sync::broadcast::Receiver<()>,
        on_connected: Option<Box<dyn Fn() + Send + Sync + 'static>>,
    ) {
        if let Some(ref client) = self.hootenanny {
            hooteproto::spawn_health_task(
                client.clone(),
                std::time::Duration::from_secs(5),
                client.config().max_failures,
                shutdown,
                on_connected,
            );
        }
    }
}

impl Default for BackendPool {
    fn default() -> Self {
        Self::new()
    }
}
