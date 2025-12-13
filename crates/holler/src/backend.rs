//! Backend connection pool for ZMQ DEALER sockets
//!
//! Manages connections to Luanette, Hootenanny, and Chaosgarden backends.
//! Supports both standard Hootenanny protocol (MsgPack Envelope) and Chaosgarden protocol (MsgPack Message).
//!
//! Health tracking via the HealthTracker struct enables the Paranoid Pirate pattern
//! for detecting backend failures and triggering reconnection.
//!
//! Socket reconnection follows ZMQ best practices from RFC 7 (MDP) and zguide:
//! - On disconnect, the socket is closed and a new one created
//! - Exponential backoff between reconnection attempts (1s → 32s max)
//! - Ready command sent after reconnection to re-register with broker

use anyhow::{Context, Result};
use bytes::Bytes;
use hooteproto::{garden, Command, Envelope, HootFrame, Payload, ReadyPayload, PROTOCOL_VERSION};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::heartbeat::{BackendState, HeartbeatResult, HealthTracker};

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

/// Reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Initial delay before first reconnection attempt
    pub initial_delay: Duration,
    /// Maximum delay between reconnection attempts
    pub max_delay: Duration,
    /// Connection timeout
    pub connect_timeout: Duration,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(32),
            connect_timeout: Duration::from_secs(5),
        }
    }
}

/// A single backend connection
///
/// Socket is wrapped in Option to allow closing and reopening on disconnect.
/// Per ZMQ best practices, we close and recreate the socket rather than
/// relying on ZMQ's automatic reconnection, which doesn't re-register workers.
pub struct Backend {
    pub config: BackendConfig,
    /// Socket is Option to allow close/reopen. None means disconnected.
    socket: RwLock<Option<DealerSocket>>,
    /// Health tracking for heartbeat monitoring
    pub health: Arc<HealthTracker>,
    /// Current reconnection delay (doubles each attempt up to max)
    reconnect_delay: RwLock<Duration>,
    /// Reconnection configuration
    reconnect_config: ReconnectConfig,
}

impl Backend {
    /// Connect to a backend
    pub async fn connect(config: BackendConfig) -> Result<Self> {
        let reconnect_config = ReconnectConfig::default();
        let socket = Self::create_and_connect(&config, reconnect_config.connect_timeout).await?;

        info!(
            "Connected to {} at {} ({:?})",
            config.name, config.endpoint, config.protocol
        );

        let health = Arc::new(HealthTracker::new());
        health.set_state(BackendState::Ready);

        Ok(Self {
            config,
            socket: RwLock::new(Some(socket)),
            health,
            reconnect_delay: RwLock::new(reconnect_config.initial_delay),
            reconnect_config,
        })
    }

    /// Create a new socket and connect to the endpoint
    async fn create_and_connect(
        config: &BackendConfig,
        timeout: Duration,
    ) -> Result<DealerSocket> {
        debug!("Creating DEALER socket for {}", config.name);
        let mut socket = DealerSocket::new();
        debug!(
            "DEALER socket created, connecting to {}",
            config.endpoint
        );

        tokio::time::timeout(timeout, socket.connect(&config.endpoint))
            .await
            .with_context(|| {
                format!(
                    "Timeout connecting to {} at {}",
                    config.name, config.endpoint
                )
            })?
            .with_context(|| {
                format!(
                    "Failed to connect to {} at {}",
                    config.name, config.endpoint
                )
            })?;

        Ok(socket)
    }

    /// Close the socket and attempt to reconnect with exponential backoff.
    ///
    /// Per ZMQ RFC 7 and zguide Chapter 4:
    /// - Socket must be closed and reopened (not just reconnected)
    /// - Ready command must be sent to re-register with broker
    /// - Exponential backoff prevents thundering herd
    ///
    /// Returns Ok(true) if reconnection succeeded, Ok(false) if still trying.
    pub async fn reconnect(&self) -> Result<bool> {
        // Close existing socket
        {
            let mut socket_guard = self.socket.write().await;
            if socket_guard.is_some() {
                info!("{}: Closing socket for reconnection", self.config.name);
                *socket_guard = None; // Drop closes the socket
            }
        }

        // Get current delay and update for next attempt
        let delay = {
            let mut delay_guard = self.reconnect_delay.write().await;
            let current = *delay_guard;
            *delay_guard = (*delay_guard * 2).min(self.reconnect_config.max_delay);
            current
        };

        info!(
            "{}: Waiting {:?} before reconnection attempt",
            self.config.name, delay
        );
        tokio::time::sleep(delay).await;

        // Attempt to create new socket and connect
        match Self::create_and_connect(&self.config, self.reconnect_config.connect_timeout).await {
            Ok(mut new_socket) => {
                // Send Ready command to re-register with broker
                let ready = HootFrame::ready(
                    &self.config.name,
                    &ReadyPayload {
                        protocol: "HOOT01".to_string(),
                        tools: vec![], // Client doesn't advertise tools
                        accepts_binary: true,
                    },
                )?;
                let msg = frames_to_zmq_message(&ready.to_frames());

                if let Err(e) = new_socket.send(msg).await {
                    warn!("{}: Failed to send Ready after reconnect: {}", self.config.name, e);
                    return Ok(false);
                }

                // Store new socket
                *self.socket.write().await = Some(new_socket);

                // Reset backoff delay on success
                *self.reconnect_delay.write().await = self.reconnect_config.initial_delay;

                // Update state
                self.health.set_state(BackendState::Connecting);
                self.health.reset_failures();

                info!("{}: Reconnected and sent Ready", self.config.name);
                Ok(true)
            }
            Err(e) => {
                warn!(
                    "{}: Reconnection failed: {} (next attempt in {:?})",
                    self.config.name,
                    e,
                    self.reconnect_delay.read().await
                );
                Ok(false)
            }
        }
    }

    /// Check if socket is currently connected
    pub async fn is_connected(&self) -> bool {
        self.socket.read().await.is_some()
    }

    /// Get current backend state
    pub fn state(&self) -> BackendState {
        self.health.get_state()
    }

    /// Check if backend is alive (Ready or Busy)
    pub fn is_alive(&self) -> bool {
        self.health.is_alive()
    }

    /// Send a HOOT01 heartbeat and wait for response
    ///
    /// This uses the new frame protocol for efficient heartbeating without
    /// MsgPack serialization overhead.
    ///
    /// Returns Disconnected if socket is not connected (caller should reconnect).
    pub async fn send_heartbeat(&self, timeout: Duration) -> HeartbeatResult {
        let frame = HootFrame::heartbeat(&self.config.name);
        let frames = frame.to_frames();
        let msg = frames_to_zmq_message(&frames);

        let mut socket_guard = self.socket.write().await;
        let socket = match socket_guard.as_mut() {
            Some(s) => s,
            None => return HeartbeatResult::Disconnected,
        };

        // Send heartbeat
        match tokio::time::timeout(timeout, socket.send(msg)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return HeartbeatResult::Error(format!("Send failed: {}", e)),
            Err(_) => return HeartbeatResult::Timeout,
        }

        // Wait for response
        let response = match tokio::time::timeout(timeout, socket.recv()).await {
            Ok(Ok(msg)) => msg,
            Ok(Err(e)) => return HeartbeatResult::Error(format!("Recv failed: {}", e)),
            Err(_) => return HeartbeatResult::Timeout,
        };

        // Parse response - check for HOOT01 heartbeat reply
        let response_frames: Vec<Bytes> = response
            .iter()
            .map(|f| Bytes::copy_from_slice(f))
            .collect();

        // Check if it's a HOOT01 frame
        if response_frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            match HootFrame::from_frames(&response_frames) {
                Ok(resp_frame) if resp_frame.command == Command::Heartbeat => {
                    HeartbeatResult::Success
                }
                Ok(resp_frame) => {
                    // Got a different command - still alive, but unexpected
                    debug!(
                        "Heartbeat got {:?} instead of Heartbeat, treating as success",
                        resp_frame.command
                    );
                    HeartbeatResult::Success
                }
                Err(e) => HeartbeatResult::Error(format!("Parse error: {}", e)),
            }
        } else {
            // Legacy response - still indicates liveness
            debug!("Got legacy response to heartbeat, treating as success");
            HeartbeatResult::Success
        }
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

        let mut socket_guard = self.socket.write().await;
        let socket = socket_guard
            .as_mut()
            .context("Socket disconnected - backend is dead")?;

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
        let msg_type = "shell_request";
        let message = garden::Message::new(session, msg_type, request);

        // 3. Serialize to MsgPack
        let bytes = rmp_serde::to_vec(&message)?;

        debug!("Sending to {} ({} bytes)", self.config.name, bytes.len());

        let mut socket_guard = self.socket.write().await;
        let socket = socket_guard
            .as_mut()
            .context("Socket disconnected - backend is dead")?;

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
///
/// Simplified to only connect to hootenanny, which proxies to luanette and chaosgarden.
pub struct BackendPool {
    pub hootenanny: Option<Arc<Backend>>,
}

impl BackendPool {
    /// Create a new empty pool
    pub fn new() -> Self {
        Self {
            hootenanny: None,
        }
    }

    /// Connect to Hootenanny (the unified backend)
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

    /// Route all tool calls to hootenanny
    ///
    /// Hootenanny handles everything directly or proxies to:
    /// - luanette: lua_*, script_*, job_*
    /// - chaosgarden: garden_*, transport_*, timeline_*
    pub fn route_tool(&self, _tool_name: &str) -> Option<Arc<Backend>> {
        self.hootenanny.clone()
    }

    /// Get health status of hootenanny
    pub async fn health(&self) -> serde_json::Value {
        let mut backends = serde_json::Map::new();

        if let Some(ref b) = self.hootenanny {
            backends.insert("hootenanny".to_string(), b.health.health_summary().await);
        }

        serde_json::Value::Object(backends)
    }

    /// Check if hootenanny is alive
    pub fn all_alive(&self) -> bool {
        self.hootenanny.as_ref().is_none_or(|b| b.is_alive())
    }
}

impl Default for BackendPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a Vec<Bytes> to a ZmqMessage for sending
fn frames_to_zmq_message(frames: &[Bytes]) -> ZmqMessage {
    if frames.is_empty() {
        return ZmqMessage::from(Vec::<u8>::new());
    }

    let mut msg = ZmqMessage::from(frames[0].to_vec());
    for frame in frames.iter().skip(1) {
        msg.push_back(frame.to_vec().into());
    }
    msg
}