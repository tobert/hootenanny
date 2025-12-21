//! Backend connection to hootenanny using shared HootClient
//!
//! Uses hooteproto's HootClient for lazy connection, reconnection, and request correlation.

use anyhow::Result;
use hooteproto::{ClientConfig, HootClient, Payload};
use std::sync::Arc;

/// Pool of backend connections
///
/// Simplified to only connect to hootenanny, which proxies to vibeweaver and chaosgarden.
pub struct BackendPool {
    pub hootenanny: Option<Arc<HootClient>>,
}

impl BackendPool {
    /// Create a new empty pool
    pub fn new() -> Self {
        Self { hootenanny: None }
    }

    /// Set up Hootenanny backend.
    ///
    /// ZMQ's connect() is non-blocking, so the peer doesn't need to exist.
    /// The server can start immediately without waiting for the backend.
    pub async fn setup_hootenanny(&mut self, endpoint: &str, timeout_ms: u64) {
        let config = ClientConfig::new("hootenanny", endpoint).with_timeout(timeout_ms);
        let client = HootClient::new(config).await;
        self.hootenanny = Some(client);
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
