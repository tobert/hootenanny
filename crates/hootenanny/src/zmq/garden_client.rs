//! GardenClient - ZMQ client for chaosgarden daemon
//!
//! Connects to chaosgarden using the Jupyter-inspired 5-socket protocol over HOOT01 frames.
//! Uses JSON serialization for garden message envelopes.
//!
//! ## Lazy Pirate Pattern
//!
//! This client implements the Lazy Pirate pattern for reliable request-reply:
//! - Retries on timeout with exponential backoff
//! - Tracks peer health via successful responses
//! - Caps reconnection backoff to prevent hours-long delays
//!
//! ## Workarounds for rzmq Issues
//!
//! REQ sockets (heartbeat, query) include workarounds for rzmq issues:
//! - RECONNECT_IVL_MAX capped at 60s to prevent runaway backoff
//! - Periodic keepalives to prevent 300s idle timeout
//!
//! See: docs/issues/rzmq-req-idle-timeout.md, docs/issues/rzmq-backoff-cap.md

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use chaosgarden::ipc::{
    ControlReply, ControlRequest, GardenEndpoints, IOPubEvent, Message, QueryReply,
    QueryRequest, ShellReply, ShellRequest,
};
use futures::stream::Stream;
use hooteproto::{Command, ConnectionState, ContentType, HootFrame, LazyPirateConfig, PROTOCOL_VERSION};
use rzmq::socket::options::{LINGER, RECONNECT_IVL, RECONNECT_IVL_MAX, ROUTING_ID, SUBSCRIBE};
use rzmq::{Context, Msg, Socket, SocketType};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Health tracking for GardenClient
struct GardenHealth {
    state: AtomicU8,
    consecutive_failures: AtomicU32,
}

impl GardenHealth {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(ConnectionState::Unknown as u8),
            consecutive_failures: AtomicU32::new(0),
        }
    }

    fn get_state(&self) -> ConnectionState {
        match self.state.load(Ordering::Relaxed) {
            1 => ConnectionState::Connected,
            2 => ConnectionState::Dead,
            _ => ConnectionState::Unknown,
        }
    }

    fn set_state(&self, state: ConnectionState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }

    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.set_state(ConnectionState::Connected);
    }

    fn record_failure(&self, max_failures: u32) -> u32 {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures >= max_failures {
            self.set_state(ConnectionState::Dead);
        }
        failures
    }

    fn reset_failures(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }
}

/// Client for connecting to chaosgarden's ZMQ endpoints
pub struct GardenClient {
    session: Uuid,
    #[allow(dead_code)]
    context: Context,
    control: Arc<RwLock<Socket>>,
    shell: Arc<RwLock<Socket>>,
    iopub: Arc<RwLock<Socket>>,
    heartbeat: Arc<RwLock<Socket>>,
    query: Arc<RwLock<Socket>>,
    config: LazyPirateConfig,
    health: Arc<GardenHealth>,
    keepalive_handle: Option<JoinHandle<()>>,
}

impl GardenClient {
    /// Connect to chaosgarden at the given endpoints with default config
    pub async fn connect(endpoints: &GardenEndpoints) -> Result<Self> {
        Self::connect_with_config(endpoints, LazyPirateConfig::default()).await
    }

    /// Connect to chaosgarden with custom config
    pub async fn connect_with_config(
        endpoints: &GardenEndpoints,
        config: LazyPirateConfig,
    ) -> Result<Self> {
        let session = Uuid::new_v4();

        debug!("Creating sockets for chaosgarden session {}", session);

        let context = Context::new().with_context(|| "Failed to create ZMQ context")?;

        // Helper to set common socket options
        // Includes RECONNECT_IVL_MAX cap (workaround for rzmq unbounded backoff)
        async fn set_socket_opts(socket: &Socket, name: &str) {
            if let Err(e) = socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await {
                warn!("{}: Failed to set LINGER: {}", name, e);
            }
            if let Err(e) = socket
                .set_option_raw(RECONNECT_IVL, &1000i32.to_ne_bytes())
                .await
            {
                warn!("{}: Failed to set RECONNECT_IVL: {}", name, e);
            }
            // Cap reconnect backoff at 60s (workaround for rzmq unbounded backoff)
            // See: docs/issues/rzmq-backoff-cap.md
            if let Err(e) = socket
                .set_option_raw(RECONNECT_IVL_MAX, &60000i32.to_ne_bytes())
                .await
            {
                warn!("{}: Failed to set RECONNECT_IVL_MAX: {}", name, e);
            }
        }

        // Create and connect all sockets
        let control = context
            .socket(SocketType::Dealer)
            .with_context(|| "Failed to create control socket")?;
        set_socket_opts(&control, "control").await;
        if let Err(e) = control
            .set_option_raw(ROUTING_ID, b"garden-control")
            .await
        {
            warn!("control: Failed to set ROUTING_ID: {}", e);
        }
        control.connect(&endpoints.control).await.with_context(|| {
            format!("Failed to connect control socket to {}", endpoints.control)
        })?;

        let shell = context
            .socket(SocketType::Dealer)
            .with_context(|| "Failed to create shell socket")?;
        set_socket_opts(&shell, "shell").await;
        if let Err(e) = shell.set_option_raw(ROUTING_ID, b"garden-shell").await {
            warn!("shell: Failed to set ROUTING_ID: {}", e);
        }
        shell
            .connect(&endpoints.shell)
            .await
            .with_context(|| format!("Failed to connect shell socket to {}", endpoints.shell))?;

        let iopub = context
            .socket(SocketType::Sub)
            .with_context(|| "Failed to create iopub socket")?;
        set_socket_opts(&iopub, "iopub").await;
        // Subscribe to all messages
        if let Err(e) = iopub.set_option_raw(SUBSCRIBE, b"").await {
            warn!("iopub: Failed to subscribe: {}", e);
        }
        iopub
            .connect(&endpoints.iopub)
            .await
            .with_context(|| format!("Failed to connect iopub socket to {}", endpoints.iopub))?;

        let heartbeat = context
            .socket(SocketType::Req)
            .with_context(|| "Failed to create heartbeat socket")?;
        set_socket_opts(&heartbeat, "heartbeat").await;
        heartbeat
            .connect(&endpoints.heartbeat)
            .await
            .with_context(|| {
                format!(
                    "Failed to connect heartbeat socket to {}",
                    endpoints.heartbeat
                )
            })?;

        let query = context
            .socket(SocketType::Req)
            .with_context(|| "Failed to create query socket")?;
        set_socket_opts(&query, "query").await;
        query
            .connect(&endpoints.query)
            .await
            .with_context(|| format!("Failed to connect query socket to {}", endpoints.query))?;

        info!("Connected to chaosgarden, session={}", session);

        let health = Arc::new(GardenHealth::new());
        let heartbeat = Arc::new(RwLock::new(heartbeat));

        // Spawn keepalive task to prevent 300s idle timeout on REQ sockets
        // See: docs/issues/rzmq-req-idle-timeout.md
        let keepalive_handle = Self::spawn_keepalive_task(
            Arc::clone(&heartbeat),
            Arc::clone(&health),
            config.keepalive_interval,
            config.max_failures,
        );

        Ok(Self {
            session,
            context,
            control: Arc::new(RwLock::new(control)),
            shell: Arc::new(RwLock::new(shell)),
            iopub: Arc::new(RwLock::new(iopub)),
            heartbeat,
            query: Arc::new(RwLock::new(query)),
            config,
            health,
            keepalive_handle: Some(keepalive_handle),
        })
    }

    /// Spawn keepalive task to prevent rzmq's 300s idle timeout on REQ sockets.
    ///
    /// This is a workaround for rzmq issue where REQ sockets timeout after
    /// 300 seconds of idle time because SessionConnectionActorX unconditionally
    /// reads in Operational phase.
    fn spawn_keepalive_task(
        heartbeat: Arc<RwLock<Socket>>,
        health: Arc<GardenHealth>,
        interval: Duration,
        max_failures: u32,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            debug!("GardenClient keepalive task started (interval: {:?})", interval);

            loop {
                ticker.tick().await;

                // Send a heartbeat to keep the REQ socket alive
                let ping_result = Self::ping_internal(&heartbeat, Duration::from_secs(5)).await;

                match ping_result {
                    Ok(true) => {
                        health.record_success();
                        debug!("Keepalive heartbeat successful");
                    }
                    Ok(false) | Err(_) => {
                        let failures = health.record_failure(max_failures);
                        if failures == 1 || failures % 5 == 0 {
                            debug!("Keepalive heartbeat failed (failures={})", failures);
                        }
                    }
                }
            }
        })
    }

    /// Internal ping for keepalive (doesn't record health - caller does that)
    async fn ping_internal(heartbeat: &Arc<RwLock<Socket>>, timeout: Duration) -> Result<bool> {
        let frame = HootFrame::heartbeat("chaosgarden");
        let frames = frame.to_frames();
        let msgs = frames_to_msgs(&frames);

        let socket = heartbeat.write().await;

        // Send heartbeat
        tokio::time::timeout(timeout, socket.send_multipart(msgs))
            .await
            .context("Heartbeat send timeout")??;

        // Wait for response
        let response = tokio::time::timeout(timeout, socket.recv_multipart())
            .await
            .context("Heartbeat receive timeout")??;

        // Check for HOOT01 heartbeat reply
        let response_frames: Vec<Bytes> = response
            .iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        if response_frames
            .iter()
            .any(|f| f.as_ref() == PROTOCOL_VERSION)
        {
            match HootFrame::from_frames(&response_frames) {
                Ok(resp_frame) if resp_frame.command == Command::Heartbeat => Ok(true),
                Ok(_) => Ok(true),
                Err(e) => {
                    warn!("Heartbeat parse error: {}", e);
                    Ok(false)
                }
            }
        } else {
            Ok(true)
        }
    }

    /// Get the session ID
    pub fn session(&self) -> Uuid {
        self.session
    }

    /// Get current connection state
    pub fn health_state(&self) -> ConnectionState {
        self.health.get_state()
    }

    /// Check if peer is responding
    pub fn is_connected(&self) -> bool {
        self.health.get_state() == ConnectionState::Connected
    }

    /// Send a shell request
    #[allow(dead_code)]
    pub async fn request(&self, req: ShellRequest) -> Result<ShellReply> {
        self.request_with_job_id(req, None).await
    }

    /// Send a shell request with job_id for correlation
    pub async fn request_with_job_id(
        &self,
        req: ShellRequest,
        _job_id: Option<&str>,
    ) -> Result<ShellReply> {
        let msg = Message::new(self.session, "shell_request", req);
        let msg_json = serde_json::to_vec(&msg).context("Failed to serialize shell request")?;

        let request_id = msg.header.msg_id;

        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::Json,
            request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: Bytes::from(msg_json),
        };

        let frames = frame.to_frames();
        let msgs = frames_to_msgs(&frames);

        debug!("Sending shell request ({})", request_id);

        let socket = self.shell.write().await;

        // Send
        tokio::time::timeout(self.config.timeout, socket.send_multipart(msgs))
            .await
            .context("Shell request send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.config.timeout, socket.recv_multipart())
            .await
            .context("Shell response receive timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        let response_frame =
            HootFrame::from_frames(&response_frames).context("Failed to parse response frame")?;

        if response_frame.content_type != ContentType::Json {
            anyhow::bail!(
                "Expected JSON response, got {:?}",
                response_frame.content_type
            );
        }

        let response_msg: Message<ShellReply> = serde_json::from_slice(&response_frame.body)
            .context("Failed to deserialize shell reply")?;

        self.health.record_success();
        Ok(response_msg.content)
    }

    /// Send a control request (priority channel)
    pub async fn control(&self, req: ControlRequest) -> Result<ControlReply> {
        let msg = Message::new(self.session, "control_request", req);
        let msg_json = serde_json::to_vec(&msg).context("Failed to serialize control request")?;

        let request_id = msg.header.msg_id;

        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::Json,
            request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: Bytes::from(msg_json),
        };

        let frames = frame.to_frames();
        let msgs = frames_to_msgs(&frames);

        debug!("Sending control request ({})", request_id);

        let socket = self.control.write().await;

        // Send
        tokio::time::timeout(self.config.timeout, socket.send_multipart(msgs))
            .await
            .context("Control request send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.config.timeout, socket.recv_multipart())
            .await
            .context("Control response receive timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        let response_frame =
            HootFrame::from_frames(&response_frames).context("Failed to parse response frame")?;

        if response_frame.content_type != ContentType::Json {
            anyhow::bail!(
                "Expected JSON response, got {:?}",
                response_frame.content_type
            );
        }

        let response_msg: Message<ControlReply> = serde_json::from_slice(&response_frame.body)
            .context("Failed to deserialize control reply")?;

        self.health.record_success();
        Ok(response_msg.content)
    }

    /// Execute a Trustfall query with Lazy Pirate retry logic.
    ///
    /// The query socket uses REQ pattern, which needs retries because:
    /// 1. REQ sockets can timeout on idle (rzmq issue)
    /// 2. Chaosgarden serializes requests, so we retry on timeout
    pub async fn query(
        &self,
        query_str: &str,
        variables: HashMap<String, serde_json::Value>,
    ) -> Result<QueryReply> {
        let max_attempts = self.config.max_retries + 1;
        let mut attempts = 0;

        loop {
            attempts += 1;

            match self.query_single_attempt(query_str, &variables).await {
                Ok(reply) => {
                    self.health.record_success();
                    return Ok(reply);
                }
                Err(e) if attempts < max_attempts => {
                    self.health.record_failure(self.config.max_failures);
                    let backoff = self.config.backoff_for_attempt(attempts);
                    warn!(
                        "Query attempt {} failed: {}, retrying in {:?}...",
                        attempts, e, backoff
                    );
                    tokio::time::sleep(backoff).await;
                }
                Err(e) => {
                    self.health.record_failure(self.config.max_failures);
                    return Err(e.context(format!("Query failed after {} attempts", attempts)));
                }
            }
        }
    }

    /// Single query attempt (used by retry loop)
    async fn query_single_attempt(
        &self,
        query_str: &str,
        variables: &HashMap<String, serde_json::Value>,
    ) -> Result<QueryReply> {
        let req = QueryRequest {
            query: query_str.to_string(),
            variables: variables.clone(),
        };

        let msg = Message::new(self.session, "query_request", req);
        let msg_json = serde_json::to_vec(&msg).context("Failed to serialize query request")?;

        let request_id = msg.header.msg_id;

        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::Json,
            request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: Bytes::from(msg_json),
        };

        let frames = frame.to_frames();
        let msgs = frames_to_msgs(&frames);

        debug!("Sending query ({}) attempt", request_id);

        let socket = self.query.write().await;

        // Send
        tokio::time::timeout(self.config.timeout, socket.send_multipart(msgs))
            .await
            .context("Query send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.config.timeout, socket.recv_multipart())
            .await
            .context("Query response timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        let response_frame =
            HootFrame::from_frames(&response_frames).context("Failed to parse response frame")?;

        if response_frame.content_type != ContentType::Json {
            anyhow::bail!(
                "Expected JSON response, got {:?}",
                response_frame.content_type
            );
        }

        let response_msg: Message<QueryReply> = serde_json::from_slice(&response_frame.body)
            .context("Failed to deserialize query reply")?;

        Ok(response_msg.content)
    }

    /// Ping the daemon via heartbeat
    pub async fn ping(&self, timeout: Duration) -> Result<bool> {
        match Self::ping_internal(&self.heartbeat, timeout).await {
            Ok(alive) => {
                if alive {
                    self.health.record_success();
                } else {
                    self.health.record_failure(self.config.max_failures);
                }
                Ok(alive)
            }
            Err(e) => {
                self.health.record_failure(self.config.max_failures);
                Err(e)
            }
        }
    }

    /// Get IOPub event stream
    pub fn events(&self) -> Pin<Box<dyn Stream<Item = IOPubEvent> + Send + 'static>> {
        let iopub = self.iopub.clone();

        Box::pin(async_stream::stream! {
            loop {
                let msg = {
                    let socket = iopub.write().await;
                    socket.recv_multipart().await
                };

                match msg {
                    Ok(zmq_msgs) => {
                        let frames: Vec<Bytes> = zmq_msgs
                            .into_iter()
                            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                            .collect();

                        // Skip subscription filter frame if present
                        let frame_result = if frames.len() > 1 && frames[0].is_empty() {
                            HootFrame::from_frames(&frames[1..])
                        } else {
                            HootFrame::from_frames(&frames)
                        };

                        match frame_result {
                            Ok(frame) if frame.content_type == ContentType::Json => {
                                match serde_json::from_slice::<Message<IOPubEvent>>(&frame.body) {
                                    Ok(msg) => yield msg.content,
                                    Err(e) => {
                                        error!("Failed to deserialize IOPub event: {}", e);
                                    }
                                }
                            }
                            Ok(frame) => {
                                warn!("Unexpected IOPub content type: {:?}", frame.content_type);
                            }
                            Err(e) => {
                                error!("Failed to parse IOPub frame: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("IOPub socket error: {}", e);
                        break;
                    }
                }
            }
        })
    }
}

/// Convert Vec<Bytes> to Vec<Msg> for rzmq multipart
fn frames_to_msgs(frames: &[Bytes]) -> Vec<Msg> {
    frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect()
}
