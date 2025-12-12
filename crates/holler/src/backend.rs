//! Backend connection pool for ZMQ DEALER sockets
//!
//! Manages connections to Luanette, Hootenanny, and Chaosgarden backends.

use anyhow::{Context, Result};
use hooteproto::{Envelope, Payload};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

/// Configuration for a backend connection
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub name: String,
    pub endpoint: String,
    pub timeout_ms: u64,
}

/// A single backend connection
pub struct Backend {
    pub config: BackendConfig,
    socket: RwLock<DealerSocket>,
}

impl Backend {
    /// Connect to a backend
    pub async fn connect(config: BackendConfig) -> Result<Self> {
        let mut socket = DealerSocket::new();
        socket
            .connect(&config.endpoint)
            .await
            .with_context(|| format!("Failed to connect to {} at {}", config.name, config.endpoint))?;

        info!("Connected to {} at {}", config.name, config.endpoint);

        Ok(Self {
            config,
            socket: RwLock::new(socket),
        })
    }

    /// Send a request and wait for response
    pub async fn request(&self, payload: Payload) -> Result<Payload> {
        self.request_with_trace(payload, None).await
    }

    /// Send a request with traceparent and wait for response
    pub async fn request_with_trace(
        &self,
        payload: Payload,
        traceparent: Option<String>,
    ) -> Result<Payload> {
        let mut envelope = Envelope::new(payload);
        if let Some(tp) = traceparent {
            envelope = envelope.with_traceparent(tp);
        }
        let json = serde_json::to_string(&envelope)?;

        debug!("Sending to {}: {}", self.config.name, json);

        let mut socket = self.socket.write().await;

        // Send
        let msg = ZmqMessage::from(json.as_bytes().to_vec());
        let timeout = Duration::from_millis(self.config.timeout_ms);

        tokio::time::timeout(timeout, socket.send(msg))
            .await
            .context("Send timeout")?
            .context("Failed to send")?;

        // Receive
        let response = tokio::time::timeout(timeout, socket.recv())
            .await
            .context("Receive timeout")?
            .context("Failed to receive")?;

        let response_bytes = response.get(0).context("Empty response")?;
        let response_str = std::str::from_utf8(response_bytes)?;

        debug!("Received from {}: {}", self.config.name, response_str);

        let response_envelope: Envelope = serde_json::from_str(response_str)
            .with_context(|| format!("Failed to parse response: {}", response_str))?;

        Ok(response_envelope.payload)
    }

    /// Check if backend is healthy with a ping
    #[allow(dead_code)]
    pub async fn health_check(&self) -> bool {
        match self.request(Payload::Ping).await {
            Ok(Payload::Pong { .. }) => true,
            Ok(_) => {
                warn!("{} returned unexpected response to ping", self.config.name);
                false
            }
            Err(e) => {
                warn!("{} health check failed: {}", self.config.name, e);
                false
            }
        }
    }
}

/// Pool of backend connections
pub struct BackendPool {
    pub luanette: Option<Arc<Backend>>,
    pub hootenanny: Option<Arc<Backend>>,
    pub chaosgarden: Option<Arc<Backend>>,
}

impl BackendPool {
    /// Create a new empty pool
    pub fn new() -> Self {
        Self {
            luanette: None,
            hootenanny: None,
            chaosgarden: None,
        }
    }

    /// Connect to Luanette
    pub async fn connect_luanette(&mut self, endpoint: &str, timeout_ms: u64) -> Result<()> {
        let backend = Backend::connect(BackendConfig {
            name: "luanette".to_string(),
            endpoint: endpoint.to_string(),
            timeout_ms,
        })
        .await?;
        self.luanette = Some(Arc::new(backend));
        Ok(())
    }

    /// Connect to Hootenanny
    pub async fn connect_hootenanny(&mut self, endpoint: &str, timeout_ms: u64) -> Result<()> {
        let backend = Backend::connect(BackendConfig {
            name: "hootenanny".to_string(),
            endpoint: endpoint.to_string(),
            timeout_ms,
        })
        .await?;
        self.hootenanny = Some(Arc::new(backend));
        Ok(())
    }

    /// Connect to Chaosgarden
    pub async fn connect_chaosgarden(&mut self, endpoint: &str, timeout_ms: u64) -> Result<()> {
        let backend = Backend::connect(BackendConfig {
            name: "chaosgarden".to_string(),
            endpoint: endpoint.to_string(),
            timeout_ms,
        })
        .await?;
        self.chaosgarden = Some(Arc::new(backend));
        Ok(())
    }

    /// Route a tool call to the appropriate backend based on prefix
    pub fn route_tool(&self, tool_name: &str) -> Option<Arc<Backend>> {
        // Route by prefix - Luanette handles Lua scripts and job orchestration
        if tool_name.starts_with("lua_")
            || tool_name.starts_with("script_")
        {
            return self.luanette.clone();
        }

        // Hootenanny handles everything else: CAS, artifacts, graph, orpheus, musicgen,
        // soundfont, ABC, analysis, generation, garden proxy, jobs, etc.
        if tool_name.starts_with("cas_")
            || tool_name.starts_with("artifact_")
            || tool_name.starts_with("graph_")
            || tool_name.starts_with("add_annotation")
            || tool_name.starts_with("orpheus_")
            || tool_name.starts_with("musicgen_")
            || tool_name.starts_with("yue_")
            || tool_name.starts_with("convert_")
            || tool_name.starts_with("soundfont_")
            || tool_name.starts_with("abc_")
            || tool_name.starts_with("beatthis_")
            || tool_name.starts_with("clap_")
            || tool_name.starts_with("garden_")
            || tool_name.starts_with("job_")
            || tool_name.starts_with("sample_llm")
        {
            return self.hootenanny.clone();
        }

        // Chaosgarden handles transport and timeline
        if tool_name.starts_with("transport_") || tool_name.starts_with("timeline_") {
            return self.chaosgarden.clone();
        }

        None
    }

    /// Get health status of all backends
    #[allow(dead_code)]
    pub async fn health(&self) -> serde_json::Value {
        let luanette_ok = if let Some(ref b) = self.luanette {
            b.health_check().await
        } else {
            false
        };

        let hootenanny_ok = if let Some(ref b) = self.hootenanny {
            b.health_check().await
        } else {
            false
        };

        let chaosgarden_ok = if let Some(ref b) = self.chaosgarden {
            b.health_check().await
        } else {
            false
        };

        serde_json::json!({
            "luanette": {
                "connected": self.luanette.is_some(),
                "healthy": luanette_ok,
            },
            "hootenanny": {
                "connected": self.hootenanny.is_some(),
                "healthy": hootenanny_ok,
            },
            "chaosgarden": {
                "connected": self.chaosgarden.is_some(),
                "healthy": chaosgarden_ok,
            },
        })
    }
}

impl Default for BackendPool {
    fn default() -> Self {
        Self::new()
    }
}
