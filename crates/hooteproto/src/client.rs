//! Shared ZMQ DEALER client following zguide's Lazy Pirate pattern.
//!
//! Key principles from zguide Chapter 4:
//! - ZMQ handles reconnection automatically - don't destroy sockets
//! - connect() is async/non-blocking - peer doesn't need to exist
//! - Retry failed requests (Lazy Pirate), don't assume connection is dead
//! - Heartbeat = application-level ping to verify peer is responding
//!
//! Usage:
//! ```ignore
//! let client = HootClient::new(config);  // Connects immediately (non-blocking)
//! let response = client.request(payload).await?;  // Retries on timeout
//! ```

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use rzmq::{Context, Msg, Socket, SocketType};
use rzmq::socket::options::{LINGER, RECONNECT_IVL, RECONNECT_IVL_MAX, ROUTING_ID};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope,
    Command, ContentType, HootFrame, Payload, PROTOCOL_VERSION,
};

/// Connection state - tracks if peer is responding, not ZMQ socket state
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Never received a response from peer
    Unknown = 0,
    /// Peer is responding to requests/heartbeats
    Connected = 1,
    /// Peer stopped responding (too many failures)
    Dead = 2,
}

impl ConnectionState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => ConnectionState::Unknown,
            1 => ConnectionState::Connected,
            2 => ConnectionState::Dead,
            _ => ConnectionState::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionState::Unknown => "unknown",
            ConnectionState::Connected => "connected",
            ConnectionState::Dead => "dead",
        }
    }
}

/// Health tracking based on request/response success
#[derive(Debug)]
pub struct HealthTracker {
    state: AtomicU8,
    consecutive_failures: AtomicU32,
    last_success: RwLock<Option<Instant>>,
}

impl HealthTracker {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(ConnectionState::Unknown as u8),
            consecutive_failures: AtomicU32::new(0),
            last_success: RwLock::new(None),
        }
    }

    pub fn get_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Relaxed))
    }

    pub fn set_state(&self, state: ConnectionState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }

    pub fn is_connected(&self) -> bool {
        self.get_state() == ConnectionState::Connected
    }

    pub fn is_alive(&self) -> bool {
        self.get_state() != ConnectionState::Dead
    }

    pub async fn record_success(&self) {
        *self.last_success.write().await = Some(Instant::now());
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.set_state(ConnectionState::Connected);
    }

    pub fn record_failure(&self) -> u32 {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn get_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }

    pub fn reset_failures(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    pub async fn health_summary(&self) -> serde_json::Value {
        let last = self.last_success.read().await;
        let last_secs = last.map(|t| t.elapsed().as_secs());

        serde_json::json!({
            "state": self.get_state().as_str(),
            "connected": self.is_connected(),
            "consecutive_failures": self.consecutive_failures.load(Ordering::Relaxed),
            "last_message_secs_ago": last_secs,
        })
    }
}

impl Default for HealthTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for HootClient
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Service name for logging
    pub name: String,
    /// ZMQ endpoint (e.g., "tcp://localhost:5580")
    pub endpoint: String,
    /// Request timeout in milliseconds (per attempt)
    pub timeout_ms: u64,
    /// Number of retries before failing a request (Lazy Pirate)
    pub max_retries: u32,
    /// Maximum consecutive heartbeat failures before marking dead
    pub max_failures: u32,
}

impl ClientConfig {
    pub fn new(name: &str, endpoint: &str) -> Self {
        Self {
            name: name.to_string(),
            endpoint: endpoint.to_string(),
            timeout_ms: 30_000,
            max_retries: 3,
            max_failures: 5,
        }
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }
}

/// ZMQ DEALER client following Lazy Pirate pattern.
///
/// Key design decisions:
/// - Socket is created and connected immediately (ZMQ connect is non-blocking)
/// - Socket is NEVER destroyed - ZMQ handles reconnection automatically
/// - Requests are retried on timeout (Lazy Pirate pattern)
/// - Health tracks if peer is RESPONDING, not if socket is connected
pub struct HootClient {
    config: ClientConfig,
    #[allow(dead_code)]
    context: Context,
    socket: RwLock<Socket>,
    pub health: Arc<HealthTracker>,
}

impl HootClient {
    /// Create a new client and connect immediately.
    ///
    /// ZMQ's connect() is non-blocking - the peer doesn't need to exist yet.
    /// ZMQ will automatically handle reconnection if the peer appears later.
    pub async fn new(config: ClientConfig) -> Arc<Self> {
        let context = Context::new().expect("Failed to create ZMQ context");
        let socket = context
            .socket(SocketType::Dealer)
            .expect("Failed to create DEALER socket");

        // Set socket options for proper Lazy Pirate behavior
        // ROUTING_ID: Stable identity so ROUTER can route back after reconnect
        if let Err(e) = socket
            .set_option_raw(ROUTING_ID, config.name.as_bytes())
            .await
        {
            warn!("{}: Failed to set ROUTING_ID: {}", config.name, e);
        }

        // RECONNECT_IVL: Initial reconnect interval (1 second)
        if let Err(e) = socket
            .set_option_raw(RECONNECT_IVL, &1000i32.to_ne_bytes())
            .await
        {
            warn!("{}: Failed to set RECONNECT_IVL: {}", config.name, e);
        }

        // RECONNECT_IVL_MAX: Max backoff (30 seconds)
        if let Err(e) = socket
            .set_option_raw(RECONNECT_IVL_MAX, &30000i32.to_ne_bytes())
            .await
        {
            warn!("{}: Failed to set RECONNECT_IVL_MAX: {}", config.name, e);
        }

        // LINGER: Don't block on close (immediate)
        if let Err(e) = socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await {
            warn!("{}: Failed to set LINGER: {}", config.name, e);
        }

        // ZMQ connect is non-blocking - it just configures the socket.
        // The peer doesn't need to exist. ZMQ will reconnect automatically.
        if let Err(e) = socket.connect(&config.endpoint).await {
            warn!("{}: Socket connect configuration failed: {}", config.name, e);
        }

        info!(
            "{}: Socket configured for {} (ZMQ will connect when peer available)",
            config.name, config.endpoint
        );

        Arc::new(Self {
            health: Arc::new(HealthTracker::new()),
            context,
            socket: RwLock::new(socket),
            config,
        })
    }

    /// Create client (alias for new() for compatibility).
    pub async fn connect(config: ClientConfig) -> Result<Arc<Self>> {
        Ok(Self::new(config).await)
    }

    /// Send a request and wait for response with retries (Lazy Pirate pattern).
    pub async fn request(&self, payload: Payload) -> Result<Payload> {
        self.request_with_trace(payload, None).await
    }

    /// Send a request with traceparent and retries.
    pub async fn request_with_trace(
        &self,
        payload: Payload,
        traceparent: Option<String>,
    ) -> Result<Payload> {
        let request_id = Uuid::new_v4();
        let timeout = Duration::from_millis(self.config.timeout_ms);

        // Build the HOOT01 frame once
        let message = payload_to_capnp_envelope(request_id, &payload)
            .context("Failed to encode payload")?;
        let body_bytes = capnp::serialize::write_message_to_words(&message);

        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::CapnProto,
            request_id,
            service: self.config.name.clone(),
            traceparent: traceparent.clone(),
            body: Bytes::from(body_bytes),
        };

        let msgs = frames_to_msgs(&frame.to_frames());

        // Lazy Pirate: retry on timeout
        let mut attempts = 0;
        let max_attempts = self.config.max_retries + 1;

        loop {
            attempts += 1;

            debug!(
                "{}: Sending request {} (attempt {}/{})",
                self.config.name, request_id, attempts, max_attempts
            );

            match self.send_receive(&msgs, request_id, timeout).await {
                Ok(response) => {
                    self.health.record_success().await;
                    return Ok(response);
                }
                Err(e) if attempts < max_attempts => {
                    warn!(
                        "{}: Request {} attempt {} failed: {}, retrying...",
                        self.config.name, request_id, attempts, e
                    );
                    // Small delay before retry
                    tokio::time::sleep(Duration::from_millis(100 * attempts as u64)).await;
                }
                Err(e) => {
                    self.health.record_failure();
                    return Err(e.context(format!(
                        "{}: Request failed after {} attempts",
                        self.config.name, attempts
                    )));
                }
            }
        }
    }

    /// Internal send/receive for a single attempt.
    async fn send_receive(
        &self,
        msgs: &[Msg],
        request_id: Uuid,
        timeout: Duration,
    ) -> Result<Payload> {
        let socket = self.socket.write().await;

        // Send multipart
        let send_result =
            tokio::time::timeout(timeout, socket.send_multipart(msgs.to_vec())).await;
        match send_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(anyhow::anyhow!("Send failed: {}", e)),
            Err(_) => return Err(anyhow::anyhow!("Send timeout")),
        }

        // Receive with correlation loop
        let start = Instant::now();
        loop {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(anyhow::anyhow!("Receive timeout waiting for {}", request_id));
            }

            let recv_result = tokio::time::timeout(remaining, socket.recv_multipart()).await;
            let response = match recv_result {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => return Err(anyhow::anyhow!("Receive failed: {}", e)),
                Err(_) => return Err(anyhow::anyhow!("Receive timeout")),
            };

            // Parse response - convert Vec<Msg> to Vec<Bytes>
            let response_frames: Vec<Bytes> = response
                .into_iter()
                .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                .collect();

            let response_frame = match HootFrame::from_frames(&response_frames) {
                Ok(f) => f,
                Err(e) => {
                    // Log frame details on parse failure for debugging
                    debug!(
                        "{}: Parse failed - {} frames received, first bytes: {:?}",
                        self.config.name,
                        response_frames.len(),
                        response_frames.first().map(|f| &f[..f.len().min(20)])
                    );
                    return Err(anyhow::anyhow!("Failed to parse response frame: {}", e));
                }
            };

            // Check correlation
            if response_frame.request_id != request_id {
                debug!(
                    "{}: Discarding response for {} (expected {})",
                    self.config.name, response_frame.request_id, request_id
                );
                continue;
            }

            // Parse payload
            let reader = response_frame.read_capnp().context("Failed to read capnp")?;
            let envelope = reader
                .get_root::<envelope_capnp::envelope::Reader>()
                .context("Failed to get envelope")?;

            return capnp_envelope_to_payload(envelope).context("Failed to decode payload");
        }
    }

    /// Send a heartbeat ping to verify peer is responding.
    ///
    /// This is application-level heartbeating, not ZMQ connection state.
    pub async fn heartbeat(&self) -> Result<()> {
        let frame = HootFrame::heartbeat(&self.config.name);
        let msgs = frames_to_msgs(&frame.to_frames());
        let timeout = Duration::from_secs(5);

        debug!("{}: Acquiring socket lock for heartbeat...", self.config.name);
        let socket = self.socket.write().await;
        debug!("{}: Socket lock acquired, sending {} frame heartbeat", self.config.name, msgs.len());

        // Send
        match tokio::time::timeout(timeout, socket.send_multipart(msgs)).await {
            Ok(Ok(())) => {
                debug!("{}: Heartbeat send completed successfully", self.config.name);
            }
            Ok(Err(e)) => return Err(anyhow::anyhow!("Heartbeat send failed: {}", e)),
            Err(_) => return Err(anyhow::anyhow!("Heartbeat send timeout")),
        }

        // Receive
        debug!("{}: Waiting for heartbeat response...", self.config.name);
        let response = match tokio::time::timeout(timeout, socket.recv_multipart()).await {
            Ok(Ok(r)) => {
                debug!("{}: Received heartbeat response: {} frames", self.config.name, r.len());
                r
            }
            Ok(Err(e)) => return Err(anyhow::anyhow!("Heartbeat receive failed: {}", e)),
            Err(_) => return Err(anyhow::anyhow!("Heartbeat receive timeout")),
        };

        // Check for HOOT01 heartbeat response
        let frames: Vec<Bytes> = response
            .iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        debug!(
            "{}: Heartbeat received {} frames, checking for HOOT01",
            self.config.name, frames.len()
        );

        if frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            match HootFrame::from_frames(&frames) {
                Ok(resp) if resp.command == Command::Heartbeat => {
                    self.health.record_success().await;
                    Ok(())
                }
                Ok(_) => {
                    // Got different command, still alive
                    self.health.record_success().await;
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!("Parse error: {}", e)),
            }
        } else {
            // Legacy response
            self.health.record_success().await;
            Ok(())
        }
    }

    /// Get config
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Get endpoint
    pub fn endpoint(&self) -> &str {
        &self.config.endpoint
    }

    /// Get service name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Check if peer has ever responded
    pub fn is_connected(&self) -> bool {
        self.health.is_connected()
    }

    /// For compatibility - always returns true since socket is always created
    pub async fn is_socket_ready(&self) -> bool {
        true
    }
}

/// Spawn a heartbeat task to verify peer is responding.
///
/// This doesn't manage socket reconnection (ZMQ does that automatically).
/// It only tracks whether the peer is actively responding.
///
/// Following zguide Chapter 4 (Paranoid Pirate pattern):
/// - Send heartbeats immediately (ZMQ handles connection timing)
/// - Only count failures AFTER we've ever successfully connected
/// - During initial Unknown phase, silently wait for connection
pub fn spawn_health_task(
    client: Arc<HootClient>,
    heartbeat_interval: Duration,
    max_failures: u32,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
    on_connected: Option<Box<dyn Fn() + Send + Sync + 'static>>,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(heartbeat_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut was_connected = false;
        let mut ever_connected = false;

        info!(
            "{}: Heartbeat task started (interval: {:?})",
            client.config.name, heartbeat_interval
        );

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match client.heartbeat().await {
                        Ok(()) => {
                            let is_connected = client.health.is_connected();
                            if is_connected && !was_connected {
                                if !ever_connected {
                                    info!("{}: Initial connection established", client.config.name);
                                } else {
                                    info!("{}: Peer reconnected", client.config.name);
                                }
                                if let Some(ref callback) = on_connected {
                                    callback();
                                }
                                ever_connected = true;
                            }
                            was_connected = is_connected;
                        }
                        Err(e) => {
                            // Only count failures if we've ever been connected
                            // During initial connection phase, just wait silently
                            if ever_connected {
                                let failures = client.health.record_failure();
                                if failures == 1 || failures.is_multiple_of(5) {
                                    debug!("{}: Peer still not responding (failures={})", client.config.name, failures);
                                }

                                if failures >= max_failures && client.health.get_state() != ConnectionState::Dead {
                                    client.health.set_state(ConnectionState::Dead);
                                    warn!("{}: Peer marked DEAD (not responding)", client.config.name);
                                }
                                was_connected = false;
                            } else {
                                // Waiting for initial connection - this is normal during startup
                                debug!(
                                    "{}: Waiting for peer (heartbeat: {})",
                                    client.config.name, e
                                );
                            }
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("{}: Heartbeat task shutting down", client.config.name);
                    break;
                }
            }
        }
    });
}

/// Convert frames to Vec<Msg> for rzmq multipart
fn frames_to_msgs(frames: &[Bytes]) -> Vec<Msg> {
    frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect()
}
