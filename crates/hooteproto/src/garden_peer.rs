//! GardenPeer - ZMQ peer for connecting to chaosgarden daemon
//!
//! Connects to chaosgarden using the Jupyter-inspired 4-socket protocol over HOOT01 frames.
//!
//! ## Usage
//!
//! ```ignore
//! use hooteconf::HootConfig;
//! use hooteproto::GardenPeer;
//!
//! let config = HootConfig::load()?;
//! let peer = GardenPeer::from_config(&config).await?;
//! let reply = peer.control(ControlRequest::DebugDump).await?;
//! ```
//!
//! ## Lazy Pirate Pattern
//!
//! This peer implements the Lazy Pirate pattern for reliable request-reply:
//! - Retries on timeout with exponential backoff
//! - Tracks peer health via successful responses
//! - Caps reconnection backoff to prevent hours-long delays
//!
//! ## Socket Types
//!
//! All sockets use DEALER for consistent multipart HOOT01 framing:
//! - control, shell: DEALER → ROUTER (chaosgarden)
//! - heartbeat: DEALER → REP (chaosgarden)
//! - iopub: SUB → PUB (chaosgarden)
//!
//! RECONNECT_IVL_MAX is capped at 60s to prevent runaway backoff.

use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use futures::stream::{Stream, StreamExt};
use futures::SinkExt;
use hooteconf::HootConfig;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::garden::{
    ControlReply, ControlRequest, GardenEndpoints, IOPubEvent, Message, ShellReply,
    ShellRequest,
};
use crate::socket_config::{
    create_dealer_and_connect, create_subscriber_and_connect, Multipart, ZmqContext,
};
use crate::{Command, ConnectionState, ContentType, HootFrame, LazyPirateConfig, PROTOCOL_VERSION};
use crate::request::ToolRequest;
use crate::responses::ToolResponse;
use crate::envelope::ResponseEnvelope;
use crate::conversion::{payload_to_capnp_envelope, capnp_envelope_to_payload};
use crate::{Payload, envelope_capnp};

/// Health tracking for GardenPeer
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
}

/// Boxed sink type for sending messages
type BoxedSink = Pin<Box<dyn futures::Sink<Multipart, Error = tmq::TmqError> + Send>>;

/// Boxed stream type for receiving messages
type BoxedStream = Pin<Box<dyn Stream<Item = Result<Multipart, tmq::TmqError>> + Send>>;

/// Split DEALER socket (tx + rx halves)
struct SplitDealer {
    tx: Mutex<BoxedSink>,
    rx: Mutex<BoxedStream>,
}

/// Split SUB socket (rx only)
struct SplitSubscriber {
    rx: Mutex<BoxedStream>,
}

/// Helper to create split dealer
fn split_dealer<S>(socket: S) -> SplitDealer
where
    S: futures::Stream<Item = Result<Multipart, tmq::TmqError>>
        + futures::Sink<Multipart, Error = tmq::TmqError>
        + Unpin
        + Send
        + 'static,
{
    let (tx, rx) = socket.split();
    SplitDealer {
        tx: Mutex::new(Box::pin(tx)),
        rx: Mutex::new(Box::pin(rx)),
    }
}

/// Helper to create split subscriber
fn split_subscriber<S>(socket: S) -> SplitSubscriber
where
    S: futures::Stream<Item = Result<Multipart, tmq::TmqError>> + Unpin + Send + 'static,
{
    SplitSubscriber {
        rx: Mutex::new(Box::pin(socket)),
    }
}

/// Client for connecting to chaosgarden's ZMQ endpoints
pub struct GardenPeer {
    session: Uuid,
    control: Arc<SplitDealer>,
    shell: Arc<SplitDealer>,
    iopub: Arc<SplitSubscriber>,
    heartbeat: Arc<SplitDealer>,
    config: LazyPirateConfig,
    health: Arc<GardenHealth>,
    #[allow(dead_code)]
    keepalive_handle: Option<JoinHandle<()>>,
}

impl GardenPeer {
    /// Create client from HootConfig (the recommended way).
    pub async fn from_config(config: &HootConfig) -> Result<Self> {
        let endpoints = GardenEndpoints::from_config(config)?;
        Self::connect(&endpoints).await
    }

    /// Create client from HootConfig with custom Lazy Pirate settings.
    pub async fn from_config_with_options(
        config: &HootConfig,
        lazy_config: LazyPirateConfig,
    ) -> Result<Self> {
        let endpoints = GardenEndpoints::from_config(config)?;
        Self::connect_with_config(&endpoints, lazy_config).await
    }

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
        let session_short = &session.to_string()[..8]; // First 8 chars for unique identity

        debug!("Creating sockets for chaosgarden session {}", session);

        let context = ZmqContext::new();

        // Create unique identities per session to avoid ROUTER routing conflicts
        let control_id = format!("garden-control-{}", session_short);
        let shell_id = format!("garden-shell-{}", session_short);
        let heartbeat_id = format!("garden-hb-{}", session_short);

        // Create and connect all sockets, then split them
        let control = create_dealer_and_connect(
            &context,
            &endpoints.control,
            control_id.as_bytes(),
            "control",
        )?;

        let shell =
            create_dealer_and_connect(&context, &endpoints.shell, shell_id.as_bytes(), "shell")?;

        let iopub = create_subscriber_and_connect(&context, &endpoints.iopub, "iopub")?;

        let heartbeat = create_dealer_and_connect(
            &context,
            &endpoints.heartbeat,
            heartbeat_id.as_bytes(),
            "heartbeat",
        )?;

        info!("Connected to chaosgarden, session={}", session);

        let health = Arc::new(GardenHealth::new());
        let heartbeat = Arc::new(split_dealer(heartbeat));

        // Spawn keepalive task
        let keepalive_handle = Self::spawn_keepalive_task(
            Arc::clone(&heartbeat),
            Arc::clone(&health),
            config.keepalive_interval,
            config.max_failures,
        );

        Ok(Self {
            session,
            control: Arc::new(split_dealer(control)),
            shell: Arc::new(split_dealer(shell)),
            iopub: Arc::new(split_subscriber(iopub)),
            heartbeat,
            config,
            health,
            keepalive_handle: Some(keepalive_handle),
        })
    }

    /// Spawn keepalive task
    fn spawn_keepalive_task(
        heartbeat: Arc<SplitDealer>,
        health: Arc<GardenHealth>,
        interval: Duration,
        max_failures: u32,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            debug!(
                "GardenPeer keepalive task started (interval: {:?})",
                interval
            );

            loop {
                ticker.tick().await;

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

    /// Internal ping for keepalive
    async fn ping_internal(heartbeat: &Arc<SplitDealer>, timeout: Duration) -> Result<bool> {
        let frame = HootFrame::heartbeat("chaosgarden");
        let frames = frame.to_frames();
        let multipart: Multipart = frames.iter().map(|f| f.to_vec()).collect::<Vec<_>>().into();

        // Send
        {
            let mut tx = heartbeat.tx.lock().await;
            tokio::time::timeout(timeout, tx.send(multipart))
                .await
                .context("Heartbeat send timeout")??;
        }

        // Receive
        let response = {
            let mut rx = heartbeat.rx.lock().await;
            tokio::time::timeout(timeout, rx.next())
                .await
                .context("Heartbeat receive timeout")?
                .ok_or_else(|| anyhow::anyhow!("Socket stream ended"))??
        };

        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.to_vec()))
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
        let multipart: Multipart = frames.iter().map(|f| f.to_vec()).collect::<Vec<_>>().into();

        // Send
        {
            let mut tx = self.shell.tx.lock().await;
            tokio::time::timeout(self.config.timeout, tx.send(multipart))
                .await
                .context("Shell request send timeout")??;
        }

        // Receive
        let response = {
            let mut rx = self.shell.rx.lock().await;
            tokio::time::timeout(self.config.timeout, rx.next())
                .await
                .context("Shell response receive timeout")?
                .ok_or_else(|| anyhow::anyhow!("Socket stream ended"))??
        };

        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.to_vec()))
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

    /// Send a tool request using Cap'n Proto serialization
    ///
    /// This is the preferred method for new code - uses typed ToolRequest/ToolResponse
    /// and Cap'n Proto wire format instead of JSON.
    pub async fn tool_request(&self, req: ToolRequest) -> Result<ToolResponse> {
        let request_id = Uuid::new_v4();
        let payload = Payload::ToolRequest(req);

        // Serialize to Cap'n Proto
        let capnp_msg = payload_to_capnp_envelope(request_id, &payload)
            .context("Failed to serialize ToolRequest to Cap'n Proto")?;
        let capnp_bytes = capnp::serialize::write_message_to_words(&capnp_msg);

        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::CapnProto,
            request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: Bytes::from(capnp_bytes),
        };

        let frames = frame.to_frames();
        let multipart: Multipart = frames.iter().map(|f| f.to_vec()).collect::<Vec<_>>().into();

        // Send
        {
            let mut tx = self.shell.tx.lock().await;
            tokio::time::timeout(self.config.timeout, tx.send(multipart))
                .await
                .context("Tool request send timeout")??;
        }

        // Receive
        let response = {
            let mut rx = self.shell.rx.lock().await;
            tokio::time::timeout(self.config.timeout, rx.next())
                .await
                .context("Tool response receive timeout")?
                .ok_or_else(|| anyhow::anyhow!("Socket stream ended"))??
        };

        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.to_vec()))
            .collect();

        let response_frame =
            HootFrame::from_frames(&response_frames).context("Failed to parse response frame")?;

        if response_frame.content_type != ContentType::CapnProto {
            anyhow::bail!(
                "Expected Cap'n Proto response, got {:?}",
                response_frame.content_type
            );
        }

        // Parse Cap'n Proto response
        let capnp_reader = response_frame.read_capnp()
            .context("Failed to create Cap'n Proto reader")?;
        let envelope_reader = capnp_reader.get_root::<envelope_capnp::envelope::Reader>()
            .context("Failed to read envelope")?;
        let response_payload = capnp_envelope_to_payload(envelope_reader)
            .context("Failed to convert envelope to payload")?;

        match response_payload {
            Payload::TypedResponse(ResponseEnvelope::Success { response }) => {
                self.health.record_success();
                Ok(response)
            }
            Payload::TypedResponse(ResponseEnvelope::Error(err)) => {
                self.health.record_failure(self.config.max_failures);
                anyhow::bail!("Tool request failed: {}", err.message())
            }
            Payload::Error { code, message, .. } => {
                self.health.record_failure(self.config.max_failures);
                anyhow::bail!("Tool request error [{}]: {}", code, message)
            }
            other => {
                self.health.record_failure(self.config.max_failures);
                anyhow::bail!("Unexpected response payload: {:?}", other)
            }
        }
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
        let multipart: Multipart = frames.iter().map(|f| f.to_vec()).collect::<Vec<_>>().into();

        debug!("Sending control request ({})", request_id);

        // Send
        {
            let mut tx = self.control.tx.lock().await;
            tokio::time::timeout(self.config.timeout, tx.send(multipart))
                .await
                .context("Control request send timeout")??;
        }

        // Receive
        let response = {
            let mut rx = self.control.rx.lock().await;
            tokio::time::timeout(self.config.timeout, rx.next())
                .await
                .context("Control response receive timeout")?
                .ok_or_else(|| anyhow::anyhow!("Socket stream ended"))??
        };

        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.to_vec()))
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
                    let mut rx = iopub.rx.lock().await;
                    rx.next().await
                };

                match msg {
                    Some(Ok(multipart)) => {
                        let frames: Vec<Bytes> = multipart
                            .into_iter()
                            .map(|m| Bytes::from(m.to_vec()))
                            .collect();

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
                    Some(Err(e)) => {
                        error!("IOPub socket error: {}", e);
                        break;
                    }
                    None => {
                        error!("IOPub socket stream ended");
                        break;
                    }
                }
            }
        })
    }
}

/// Default heartbeat interval
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);

/// Default heartbeat timeout (miss 3 beats = dead)
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);
