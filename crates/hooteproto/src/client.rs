//! Shared ZMQ DEALER client following zguide's Lazy Pirate pattern.
//!
//! Key principles from zguide Chapter 4:
//! - ZMQ handles reconnection automatically - don't destroy sockets
//! - connect() is async/non-blocking - peer doesn't need to exist
//! - Retry failed requests (Lazy Pirate), don't assume connection is dead
//! - Heartbeat = application-level ping to verify peer is responding
//!
//! Architecture: Reactor pattern to avoid lock contention
//! - Socket owned by dedicated reactor task
//! - Requests flow through mpsc channel
//! - Responses routed via oneshot channels keyed by request_id
//!
//! Usage:
//! ```ignore
//! let client = HootClient::new(config).await;  // Spawns reactor
//! let response = client.request(payload).await?;  // Retries on timeout
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use crate::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope,
    socket_config::{create_dealer_and_connect, DealerSocket, Multipart, ZmqContext},
    Command, ContentType, HootFrame, Payload,
};

/// Command sent to the reactor task
enum ReactorCommand {
    /// Send a request and return response via oneshot
    Request {
        frames: Vec<Bytes>,
        request_id: Uuid,
        timeout: Duration,
        response_tx: oneshot::Sender<Result<Payload>>,
    },
    /// Shutdown the reactor gracefully
    Shutdown,
}

/// A pending request waiting for its response
struct PendingRequest {
    response_tx: oneshot::Sender<Result<Payload>>,
    deadline: Instant,
}

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

/// The reactor task - owns the socket, handles all I/O.
///
/// This task runs continuously, interleaving:
/// - Processing commands from callers (send requests)
/// - Receiving responses from the socket
/// - Cleaning up timed-out requests
async fn reactor_task<S: DealerSocket>(
    mut socket: S,
    mut cmd_rx: mpsc::Receiver<ReactorCommand>,
    health: Arc<HealthTracker>,
    name: String,
) {
    let mut pending: HashMap<Uuid, PendingRequest> = HashMap::new();
    let mut cleanup_interval = tokio::time::interval(Duration::from_secs(1));
    cleanup_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    debug!("{}: Reactor task started", name);

    loop {
        tokio::select! {
            // Bias towards processing commands first to avoid starvation
            biased;

            // Process commands from callers
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(ReactorCommand::Request { frames, request_id, timeout, response_tx }) => {
                        trace!("{}: Sending request {}", name, request_id);

                        // Convert frames to Multipart for tmq
                        let multipart: Multipart = frames
                            .iter()
                            .map(|f| f.to_vec())
                            .collect::<Vec<_>>()
                            .into();

                        // Send via Sink trait
                        if let Err(e) = socket.send(multipart).await {
                            warn!("{}: Send failed for {}: {}", name, request_id, e);
                            let _ = response_tx.send(Err(anyhow::anyhow!("Send failed: {}", e)));
                            continue;
                        }

                        // Register pending request
                        pending.insert(request_id, PendingRequest {
                            response_tx,
                            deadline: Instant::now() + timeout,
                        });
                        trace!("{}: Request {} registered, {} pending", name, request_id, pending.len());
                    }
                    Some(ReactorCommand::Shutdown) => {
                        info!("{}: Reactor shutting down, failing {} pending requests", name, pending.len());
                        for (id, req) in pending.drain() {
                            let _ = req.response_tx.send(Err(anyhow::anyhow!("Reactor shutdown")));
                            trace!("{}: Failed pending request {} due to shutdown", name, id);
                        }
                        break;
                    }
                    None => {
                        info!("{}: Command channel closed, reactor exiting", name);
                        break;
                    }
                }
            }

            // Receive responses from socket via Stream trait
            result = socket.next() => {
                match result {
                    Some(Ok(multipart)) => {
                        // Convert Multipart to Vec<Bytes>
                        let frames: Vec<Bytes> = multipart
                            .into_iter()
                            .map(|msg| Bytes::from(msg.to_vec()))
                            .collect();

                        match HootFrame::from_frames(&frames) {
                            Ok(frame) => {
                                trace!("{}: Received response for {}", name, frame.request_id);

                                if let Some(req) = pending.remove(&frame.request_id) {
                                    // Heartbeat responses have empty body - don't parse
                                    let payload_result = if frame.command == Command::Heartbeat {
                                        Ok(Payload::Ping)
                                    } else {
                                        parse_response_payload(&frame)
                                    };

                                    if payload_result.is_ok() {
                                        health.record_success().await;
                                    }
                                    let _ = req.response_tx.send(payload_result);
                                } else {
                                    debug!(
                                        "{}: Discarding orphan response for {} (not in {} pending)",
                                        name, frame.request_id, pending.len()
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("{}: Failed to parse response frame: {}", name, e);
                            }
                        }
                    }
                    Some(Err(e)) => {
                        warn!("{}: Receive error: {}", name, e);
                        // ZMQ handles reconnection - don't panic
                    }
                    None => {
                        // Stream ended - shouldn't happen with ZMQ sockets
                        warn!("{}: Socket stream ended unexpectedly", name);
                        break;
                    }
                }
            }

            // Cleanup expired requests
            _ = cleanup_interval.tick() => {
                let now = Instant::now();
                let expired_ids: Vec<Uuid> = pending
                    .iter()
                    .filter(|(_, req)| now > req.deadline)
                    .map(|(id, _)| *id)
                    .collect();

                for id in &expired_ids {
                    if let Some(req) = pending.remove(id) {
                        debug!("{}: Request {} timed out", name, id);
                        let _ = req.response_tx.send(Err(anyhow::anyhow!("Request timed out")));
                    }
                }

                if !expired_ids.is_empty() {
                    debug!("{}: Expired {} requests, {} remaining", name, expired_ids.len(), pending.len());
                }
            }
        }
    }

    debug!("{}: Reactor task exiting", name);
}

/// Parse response frame into Payload
fn parse_response_payload(frame: &HootFrame) -> Result<Payload> {
    let reader = frame.read_capnp().context("Failed to read capnp")?;
    let envelope = reader
        .get_root::<envelope_capnp::envelope::Reader>()
        .context("Failed to get envelope")?;
    capnp_envelope_to_payload(envelope).context("Failed to decode payload")
}

/// ZMQ DEALER client following Lazy Pirate pattern with reactor architecture.
///
/// Key design decisions:
/// - Socket owned by background reactor task (no lock contention)
/// - Requests sent via channel, responses via oneshot
/// - Retries happen in caller (Lazy Pirate pattern preserved)
/// - Health tracks if peer is RESPONDING, not if socket is connected
pub struct HootClient {
    config: ClientConfig,
    cmd_tx: mpsc::Sender<ReactorCommand>,
    pub health: Arc<HealthTracker>,
    /// ZMQ context must outlive the socket - keep it alive here
    #[allow(dead_code)]
    context: ZmqContext,
}

impl HootClient {
    /// Create a new client and spawn the reactor task.
    ///
    /// ZMQ's connect() is non-blocking - the peer doesn't need to exist yet.
    /// ZMQ will automatically handle reconnection if the peer appears later.
    pub async fn new(config: ClientConfig) -> Arc<Self> {
        let context = ZmqContext::new();

        // Use centralized socket configuration
        let socket = match create_dealer_and_connect(
            &context,
            &config.endpoint,
            config.name.as_bytes(),
            &config.name,
        ) {
            Ok(s) => s,
            Err(e) => {
                panic!("{}: Failed to create socket: {}", config.name, e);
            }
        };

        info!(
            "{}: Socket configured for {} (ZMQ will connect when peer available)",
            config.name, config.endpoint
        );

        // Create channel for reactor commands (256 buffer should be plenty)
        let (cmd_tx, cmd_rx) = mpsc::channel(256);
        let health = Arc::new(HealthTracker::new());

        // Spawn reactor task (owns the socket)
        let reactor_health = health.clone();
        let reactor_name = config.name.clone();
        tokio::spawn(async move {
            reactor_task(socket, cmd_rx, reactor_health, reactor_name).await;
        });

        Arc::new(Self {
            config,
            cmd_tx,
            health,
            context,
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
    ///
    /// Retries happen in the caller (Lazy Pirate pattern). Each retry gets a
    /// fresh request_id for clean correlation.
    pub async fn request_with_trace(
        &self,
        payload: Payload,
        traceparent: Option<String>,
    ) -> Result<Payload> {
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let max_attempts = self.config.max_retries + 1;
        let mut attempts = 0;

        loop {
            attempts += 1;

            // Fresh request_id for each attempt (clean correlation)
            let request_id = Uuid::new_v4();

            // Build the HOOT01 frame
            let frames = self.build_request_frames(request_id, &payload, &traceparent)?;

            debug!(
                "{}: Sending request {} (attempt {}/{})",
                self.config.name, request_id, attempts, max_attempts
            );

            match self.send_single_request(frames, request_id, timeout).await {
                Ok(response) => {
                    return Ok(response);
                }
                Err(e) => {
                    let error_msg = e.to_string();

                    // Connection lost is not retriable - fail immediately
                    if error_msg.contains("Connection lost") {
                        self.health.record_failure();
                        return Err(e.context(format!(
                            "{}: Connection lost, not retrying",
                            self.config.name
                        )));
                    }

                    // Other errors (timeout, send failure) can be retried
                    if attempts < max_attempts {
                        warn!(
                            "{}: Request {} attempt {} failed: {}, retrying...",
                            self.config.name, request_id, attempts, e
                        );
                        self.health.record_failure();
                        // Small delay before retry with backoff
                        tokio::time::sleep(Duration::from_millis(100 * attempts as u64)).await;
                    } else {
                        self.health.record_failure();
                        return Err(e.context(format!(
                            "{}: Request failed after {} attempts",
                            self.config.name, attempts
                        )));
                    }
                }
            }
        }
    }

    /// Build HOOT01 request frames from a payload.
    fn build_request_frames(
        &self,
        request_id: Uuid,
        payload: &Payload,
        traceparent: &Option<String>,
    ) -> Result<Vec<Bytes>> {
        let message = payload_to_capnp_envelope(request_id, payload)
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

        Ok(frame.to_frames())
    }

    /// Send a single request to the reactor and wait for response.
    async fn send_single_request(
        &self,
        frames: Vec<Bytes>,
        request_id: Uuid,
        timeout: Duration,
    ) -> Result<Payload> {
        let (response_tx, response_rx) = oneshot::channel();

        // Send command to reactor
        self.cmd_tx
            .send(ReactorCommand::Request {
                frames,
                request_id,
                timeout,
                response_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Reactor channel closed"))?;

        // Wait for response (reactor handles timeout via deadline)
        response_rx
            .await
            .map_err(|_| anyhow::anyhow!("Reactor dropped response channel"))?
    }

    /// Send a heartbeat ping to verify peer is responding.
    ///
    /// This is application-level heartbeating, not ZMQ connection state.
    /// Uses the same reactor channel as regular requests (no lock contention).
    pub async fn heartbeat(&self) -> Result<()> {
        let frame = HootFrame::heartbeat(&self.config.name);
        let request_id = frame.request_id; // Use the frame's request_id for correlation
        let frames = frame.to_frames();
        let timeout = Duration::from_secs(5);

        debug!("{}: Sending heartbeat {}", self.config.name, request_id);

        let (response_tx, response_rx) = oneshot::channel();

        // Send heartbeat through reactor (same as regular request)
        self.cmd_tx
            .send(ReactorCommand::Request {
                frames,
                request_id,
                timeout,
                response_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Reactor channel closed"))?;

        // Wait for response
        match response_rx.await {
            Ok(Ok(_)) => {
                debug!("{}: Heartbeat {} successful", self.config.name, request_id);
                Ok(())
            }
            Ok(Err(e)) => {
                debug!("{}: Heartbeat {} failed: {}", self.config.name, request_id, e);
                Err(e)
            }
            Err(_) => Err(anyhow::anyhow!("Reactor dropped heartbeat channel")),
        }
    }

    /// Gracefully shut down the reactor task.
    pub async fn shutdown(&self) {
        let _ = self.cmd_tx.send(ReactorCommand::Shutdown).await;
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
                                if failures == 1 || failures % 5 == 0 {
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
