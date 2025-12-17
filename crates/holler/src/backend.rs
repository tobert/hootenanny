//! Backend connection pool for ZMQ DEALER sockets
//!
//! Manages connections to Luanette, Hootenanny, and Chaosgarden backends.
//! All backends use HOOT01 frame protocol with Cap'n Proto payloads.
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
use hooteproto::{Command, HootFrame, Payload, PROTOCOL_VERSION};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::heartbeat::{BackendState, HeartbeatResult, HealthTracker};

/// Configuration for a backend connection
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub name: String,
    pub endpoint: String,
    pub timeout_ms: u64,
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
            "Connected to {} at {} (HOOT01 + Cap'n Proto)",
            config.name, config.endpoint
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

    /// Create a backend in disconnected state for lazy connection.
    ///
    /// The backend starts in Dead state and will connect on first reconnect() call.
    /// Use this when you want to start the server immediately without blocking on
    /// backend availability.
    pub fn new_disconnected(config: BackendConfig) -> Self {
        let reconnect_config = ReconnectConfig::default();
        let health = Arc::new(HealthTracker::new());
        health.set_state(BackendState::Dead);

        info!(
            "Created disconnected backend {} for {} (HOOT01)",
            config.name, config.endpoint
        );

        Self {
            config,
            socket: RwLock::new(None),
            health,
            reconnect_delay: RwLock::new(reconnect_config.initial_delay),
            reconnect_config,
        }
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
                let ready = HootFrame {
                    command: Command::Ready,
                    content_type: hooteproto::ContentType::Empty,
                    request_id: Uuid::new_v4(),
                    service: self.config.name.clone(),
                    traceparent: None,
                    body: Bytes::new(),
                };
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
        use hooteproto::{payload_to_capnp_envelope, capnp_envelope_to_payload, ContentType};

        // Generate request ID
        let request_id = Uuid::new_v4();

        // Convert payload to Cap'n Proto envelope
        let message = payload_to_capnp_envelope(request_id, &payload)
            .context("Failed to convert payload to capnp")?;

        // Serialize to bytes
        let body_bytes = capnp::serialize::write_message_to_words(&message);

        // Create HootFrame
        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::CapnProto,
            request_id,
            service: "hootenanny".to_string(),
            traceparent,
            body: Bytes::from(body_bytes),
        };

        // Serialize HootFrame to ZMQ message
        let frames = frame.to_frames();
        let zmq_msg = frames_to_zmq_message(&frames);

        debug!("Sending capnp request to {} ({} bytes)", self.config.name, frame.body.len());

        let mut socket_guard = self.socket.write().await;
        let socket = socket_guard
            .as_mut()
            .context("Socket disconnected - backend is dead")?;

        // Send
        let timeout = Duration::from_millis(self.config.timeout_ms);

        tokio::time::timeout(timeout, socket.send(zmq_msg))
            .await
            .context("Send timeout")?
            .context("Failed to send")?;

        // Receive
        let response = tokio::time::timeout(timeout, socket.recv())
            .await
            .context("Receive timeout")?
            .context("Failed to receive")?;

        // Parse HootFrame from response
        let response_frames: Vec<Bytes> = response
            .into_vec()
            .into_iter()
            .map(|bytes| Bytes::from(bytes))
            .collect();

        let response_frame = HootFrame::from_frames(&response_frames)
            .context("Failed to parse response HootFrame")?;

        // Parse Cap'n Proto response
        let reader = response_frame.read_capnp()
            .context("Failed to read capnp from response")?;

        let envelope_reader = reader.get_root::<hooteproto::envelope_capnp::envelope::Reader>()
            .context("Failed to get envelope root")?;

        let response_payload = capnp_envelope_to_payload(envelope_reader)
            .context("Failed to convert capnp to payload")?;

        Ok(response_payload)
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
        })
        .await?;
        self.hootenanny = Some(Arc::new(backend));
        Ok(())
    }

    /// Set up Hootenanny backend for lazy connection (non-blocking startup).
    ///
    /// Creates a disconnected backend that will connect via the heartbeat/reconnect loop.
    /// The server can start immediately without waiting for the backend to be available.
    pub fn setup_hootenanny_lazy(&mut self, endpoint: &str, timeout_ms: u64) {
        let backend = Backend::new_disconnected(BackendConfig {
            name: "hootenanny".to_string(),
            endpoint: endpoint.to_string(),
            timeout_ms,
        });
        self.hootenanny = Some(Arc::new(backend));
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