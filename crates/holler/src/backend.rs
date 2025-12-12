//! Backend connection pool for ZMQ DEALER sockets
//!
//! Manages connections to Luanette, Hootenanny, and Chaosgarden backends.
//! Supports both standard Hootenanny protocol (MsgPack Envelope) and Chaosgarden protocol (MsgPack Message).

use anyhow::{Context, Result};
use hooteproto::{garden, Envelope, Payload};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

/// Protocol used by the backend
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Protocol {
    /// Standard Hootenanny protocol (hooteproto::Envelope serialized as MsgPack)
    Hootenanny,
    /// Chaosgarden protocol (hooteproto::garden::Message serialized as MsgPack)
    Chaosgarden,
}

/// Configuration for a backend connection
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub name: String,
    pub endpoint: String,
    pub timeout_ms: u64,
    pub protocol: Protocol,
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

        info!("Connected to {} at {} ({:?})", config.name, config.endpoint, config.protocol);

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
        match self.config.protocol {
            Protocol::Hootenanny => self.request_hootenanny(payload, traceparent).await,
            Protocol::Chaosgarden => self.request_chaosgarden(payload, traceparent).await,
        }
    }

    async fn request_hootenanny(
        &self,
        payload: Payload,
        traceparent: Option<String>,
    ) -> Result<Payload> {
        let mut envelope = Envelope::new(payload);
        if let Some(tp) = traceparent {
            envelope = envelope.with_traceparent(tp);
        }
        
        // Serialize to MsgPack
        let bytes = rmp_serde::to_vec(&envelope)?;

        debug!("Sending to {} ({} bytes)", self.config.name, bytes.len());

        let mut socket = self.socket.write().await;

        // Send
        let msg = ZmqMessage::from(bytes);
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

        // Deserialize from MsgPack
        let response_envelope: Envelope = rmp_serde::from_slice(response_bytes)
            .with_context(|| "Failed to deserialize MsgPack response")?;

        Ok(response_envelope.payload)
    }

    async fn request_chaosgarden(
        &self,
        payload: Payload,
        _traceparent: Option<String>, // Chaosgarden protocol doesn't support traceparent in header yet
    ) -> Result<Payload> {
        // 1. Convert Payload to garden::ShellRequest
        let request = payload_to_garden_request(payload)?;
        
        // 2. Wrap in garden::Message
        let session = Uuid::new_v4();
        // Determine message type based on variant name (simplified)
        // In a real implementation, we might want precise mapping, but for now:
        let msg_type = "shell_request"; 
        let message = garden::Message::new(session, msg_type, request);

        // 3. Serialize to MsgPack
        let bytes = rmp_serde::to_vec(&message)?;

        debug!("Sending to {} ({} bytes)", self.config.name, bytes.len());

        let mut socket = self.socket.write().await;

        // Send
        let msg = ZmqMessage::from(bytes);
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

        // Chaosgarden replies are multipart: [identity, ..., content]
        // But since we are Dealer connected to Router, we might just get the content part?
        // Wait, Dealer/Router pattern:
        // Sender (Dealer) sends: [empty frame (added by ZMQ?), content] -> Router
        // Router receives: [SenderID, content]
        // Router sends: [SenderID, content]
        // Sender (Dealer) receives: [content]
        
        // However, chaosgarden server.rs sends: [identity, content]
        // So Dealer should receive: [content]
        
        let response_bytes = response.get(0).context("Empty response")?;
        
        // 4. Deserialize garden::Message<garden::ShellReply>
        let reply_msg: garden::Message<garden::ShellReply> = rmp_serde::from_slice(response_bytes)
            .with_context(|| "Failed to deserialize garden response")?;
            
        // 5. Convert garden::ShellReply back to Payload
        garden_reply_to_payload(reply_msg.content)
    }

    /// Check if backend is healthy with a ping
    #[allow(dead_code)]
    pub async fn health_check(&self) -> bool {
        // Chaosgarden doesn't support Payload::Ping directly via ShellRequest
        // But for now we only use Ping for Hootenanny
        if self.config.protocol == Protocol::Chaosgarden {
            // TODO: Implement health check for Chaosgarden (e.g. TransportState)
            return true;
        }

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

/// Convert hooteproto::Payload to garden::ShellRequest
fn payload_to_garden_request(payload: Payload) -> Result<garden::ShellRequest> {
    match payload {
        Payload::TransportPlay => Ok(garden::ShellRequest::Play),
        Payload::TransportStop => Ok(garden::ShellRequest::Stop),
        Payload::TransportSeek { position_beats } => Ok(garden::ShellRequest::Seek { 
            beat: garden::Beat(position_beats) 
        }),
        Payload::TransportStatus => Ok(garden::ShellRequest::GetTransportState),
        
        Payload::TimelineQuery { from_beats: _, to_beats: _ } => {
            // Mapping range to Option<(Beat, Beat)>
            // For now simplified
            Ok(garden::ShellRequest::GetRegions { range: None })
        },
        
        // TODO: Map other types as needed
        _ => anyhow::bail!("Unsupported payload for Chaosgarden: {:?}", payload),
    }
}

/// Convert garden::ShellReply to hooteproto::Payload
fn garden_reply_to_payload(reply: garden::ShellReply) -> Result<Payload> {
    match reply {
        garden::ShellReply::Ok { result } => Ok(Payload::Success { result }),
        garden::ShellReply::Error { error, traceback: _ } => Ok(Payload::Error { 
            code: "garden_error".to_string(), 
            message: error, 
            details: None 
        }),
        garden::ShellReply::TransportState { playing, position, tempo } => {
            Ok(Payload::Success {
                result: serde_json::json!({
                    "playing": playing,
                    "position": position.0,
                    "tempo": tempo
                })
            })
        },
        garden::ShellReply::Regions { regions } => {
            Ok(Payload::Success {
                result: serde_json::to_value(regions)?
            })
        },
        garden::ShellReply::PendingApprovals { approvals } => {
            Ok(Payload::Success {
                result: serde_json::to_value(approvals)?
            })
        },
        garden::ShellReply::RegionCreated { region_id } => {
             Ok(Payload::Success {
                result: serde_json::json!({"region_id": region_id})
            })
        },
        garden::ShellReply::NodeAdded { node_id } => {
             Ok(Payload::Success {
                result: serde_json::json!({"node_id": node_id})
            })
        },
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
            protocol: Protocol::Hootenanny,
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
            protocol: Protocol::Hootenanny,
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
            protocol: Protocol::Chaosgarden,
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