//! ZMQ ROUTER server for Hootenanny using hooteproto
//!
//! Exposes all hootenanny tools over ZMQ for Holler to route to.
//! This is the primary interface for the MCP-over-ZMQ architecture where
//! holler acts as the MCP gateway and hootenanny handles the actual tool execution.
//!
//! Uses the HOOT01 frame protocol with Cap'n Proto serialization.
//! The HOOT01 protocol enables:
//! - Routing without deserialization (fixed-width header fields)
//! - Efficient heartbeats (lightweight frame header)
//! - Native binary payloads (no base64 encoding)
//!
//! Bidirectional heartbeating:
//! - Tracks connected clients via ClientTracker
//! - Sends heartbeats to clients (holler → hootenanny and hootenanny → holler)
//! - Cleans up stale clients automatically

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope, Command, ContentType,
    HootFrame, Payload, PROTOCOL_VERSION,
};
use hooteproto::request::ToolRequest;
use hooteproto::socket_config::create_router_and_bind;
use rzmq::{Context, Msg, MsgFlags, Socket};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn, Instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

use crate::api::service::EventDualityServer;
use crate::artifact_store;
use crate::cas::FileStore;
use crate::telemetry;
use crate::zmq::client_tracker::ClientTracker;
use crate::zmq::VibeweaverClient;

/// ZMQ server for hooteproto messages
///
/// Can operate in two modes:
/// 1. Standalone - with direct CAS/artifact access (legacy, for basic operations)
/// 2. Full - with EventDualityServer for full tool dispatch
///
/// Tracks connected clients for bidirectional heartbeating.
pub struct HooteprotoServer {
    bind_address: String,
    cas: Arc<FileStore>,
    artifacts: Arc<RwLock<artifact_store::FileStore>>,
    start_time: Instant,
    /// Optional EventDualityServer for full tool dispatch
    event_server: Option<Arc<EventDualityServer>>,
    /// Optional vibeweaver client for Python kernel proxy
    vibeweaver: Option<Arc<VibeweaverClient>>,
    /// Connected client tracker for bidirectional heartbeats
    client_tracker: Arc<ClientTracker>,
}

impl HooteprotoServer {
    /// Create a new server in standalone mode (CAS + artifacts only)
    pub fn new(
        bind_address: String,
        cas: Arc<FileStore>,
        artifacts: Arc<RwLock<artifact_store::FileStore>>,
    ) -> Self {
        Self {
            bind_address,
            cas,
            artifacts,
            start_time: Instant::now(),
            event_server: None,
            vibeweaver: None,
            client_tracker: Arc::new(ClientTracker::new()),
        }
    }

    /// Create a new server with full tool dispatch via EventDualityServer
    pub fn with_event_server(
        bind_address: String,
        cas: Arc<FileStore>,
        artifacts: Arc<RwLock<artifact_store::FileStore>>,
        event_server: Arc<EventDualityServer>,
    ) -> Self {
        Self {
            bind_address,
            cas,
            artifacts,
            start_time: Instant::now(),
            event_server: Some(event_server),
            vibeweaver: None,
            client_tracker: Arc::new(ClientTracker::new()),
        }
    }

    /// Add vibeweaver client for Python kernel proxy
    pub fn with_vibeweaver(mut self, vibeweaver: Option<Arc<VibeweaverClient>>) -> Self {
        self.vibeweaver = vibeweaver;
        self
    }

    /// Get the client tracker for monitoring connected clients
    pub fn client_tracker(&self) -> Arc<ClientTracker> {
        Arc::clone(&self.client_tracker)
    }

    /// Run the server until shutdown signal
    ///
    /// Uses concurrent request handling to avoid deadlocks when proxied services
    /// (like vibeweaver) call back to hootenanny during request processing.
    pub async fn run(self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Result<()> {
        let context = Context::new()
            .with_context(|| "Failed to create ZMQ context")?;

        let socket =
            create_router_and_bind(&context, &self.bind_address, "hooteproto-server").await?;

        info!("Hootenanny ZMQ server listening on {}", self.bind_address);

        // Channel for sending responses back to the main loop for transmission
        let (response_tx, mut response_rx) = mpsc::channel::<Vec<Msg>>(256);

        // Wrap self in Arc for sharing across spawned tasks
        let server = Arc::new(self);

        // Periodic cleanup of stale clients (every 30 seconds)
        let mut cleanup_interval = tokio::time::interval(std::time::Duration::from_secs(30));

        loop {
            tokio::select! {
                // Receive incoming messages
                result = socket.recv_multipart() => {
                    match result {
                        Ok(msgs) => {
                            debug!("📥 Received multipart: {} frames", msgs.len());
                            let frames: Vec<Bytes> = msgs
                                .iter()
                                .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                                .collect();

                            // Check for HOOT01 protocol
                            if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
                                warn!("Received non-HOOT01 message ({} frames), rejecting", frames.len());
                                continue;
                            }

                            // Parse frame to check command type
                            match HootFrame::from_frames_with_identity(&frames) {
                                Ok((identity, frame)) => {
                                    debug!(
                                        "HOOT01 {:?} from service={} request_id={}",
                                        frame.command, frame.service, frame.request_id
                                    );

                                    match frame.command {
                                        Command::Heartbeat => {
                                            // Handle heartbeats synchronously (fast path)
                                            debug!("💓 Heartbeat received from {}", frame.service);
                                            if let Some(client_id) = identity.first() {
                                                server.client_tracker.record_activity(client_id).await;
                                            }
                                            let response = HootFrame::heartbeat("hootenanny");
                                            let reply_frames = response.to_frames_with_identity(&identity);
                                            let reply = frames_to_msgs(&reply_frames);
                                            // Use individual send() - rzmq ROUTER send_multipart has a bug
                                            if let Err(e) = send_multipart_individually(&socket, reply).await {
                                                error!("Failed to send heartbeat response: {}", e);
                                            }
                                            debug!("💓 Heartbeat response sent to {}", frame.service);
                                        }
                                        Command::Request => {
                                            // Spawn async task for request handling (allows concurrency)
                                            let server_clone = Arc::clone(&server);
                                            let tx = response_tx.clone();
                                            tokio::spawn(async move {
                                                let reply = server_clone.handle_request(identity, frame).await;
                                                if let Err(e) = tx.send(reply).await {
                                                    error!("Failed to queue response: {}", e);
                                                }
                                            });
                                        }
                                        Command::Ready => {
                                            // Register client for bidirectional heartbeating
                                            let service = frame.service.clone();
                                            if let Some(client_id) = identity.first() {
                                                server.client_tracker
                                                    .register(client_id.clone(), service.clone())
                                                    .await;
                                            }
                                            info!("Client registered: service={}", service);
                                        }
                                        Command::Disconnect => {
                                            // Remove client from tracker
                                            if let Some(client_id) = identity.first() {
                                                server.client_tracker.remove(client_id).await;
                                            }
                                            info!("Client disconnected: service={}", frame.service);
                                        }
                                        Command::Reply => {
                                            // Unexpected - we're the server, we shouldn't receive replies
                                            warn!("Unexpected Reply command received at server");
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to parse HOOT01 frame: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error receiving message: {}", e);
                        }
                    }
                }

                // Send queued responses
                Some(reply) = response_rx.recv() => {
                    // Use individual send() - rzmq ROUTER send_multipart has a bug
                    if let Err(e) = send_multipart_individually(&socket, reply).await {
                        error!("Failed to send response: {}", e);
                    }
                }

                // Periodic cleanup of stale clients
                _ = cleanup_interval.tick() => {
                    let removed = server.client_tracker.cleanup_stale().await;
                    if !removed.is_empty() {
                        info!("🧹 Cleaned up {} stale clients: {:?}", removed.len(), removed);
                    }
                }

                // Handle shutdown
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a request and return the ZMQ message to send as response
    async fn handle_request(&self, identity: Vec<Bytes>, frame: HootFrame) -> Vec<Msg> {
        // Create span with traceparent
        let span = tracing::info_span!(
            "hoot_request",
            otel.name = "hoot_request",
            request_id = %frame.request_id,
            service = %frame.service,
        );

        if let Some(ref tp) = frame.traceparent {
            if let Some(parent_ctx) = telemetry::parse_traceparent(Some(tp.as_str())) {
                span.set_parent(parent_ctx);
            }
        }

        let response_payload = self.dispatch_request(&frame).instrument(span).await;

        // Build response frame
        let response_msg = match payload_to_capnp_envelope(frame.request_id, &response_payload) {
            Ok(msg) => msg,
            Err(e) => {
                error!("Failed to encode response: {}", e);
                let err_payload = Payload::Error {
                    code: "encode_error".to_string(),
                    message: e.to_string(),
                    details: None,
                };
                payload_to_capnp_envelope(frame.request_id, &err_payload)
                    .expect("Error payload encoding should not fail")
            }
        };

        let bytes = capnp::serialize::write_message_to_words(&response_msg);
        let response_frame = HootFrame {
            command: Command::Reply,
            content_type: ContentType::CapnProto,
            request_id: frame.request_id,
            service: "hootenanny".to_string(),
            traceparent: None,
            body: bytes.into(),
        };

        let reply_frames = response_frame.to_frames_with_identity(&identity);
        frames_to_msgs(&reply_frames)
    }

    /// Dispatch a request and return the response payload
    async fn dispatch_request(&self, frame: &HootFrame) -> Payload {
        match frame.content_type {
            ContentType::CapnProto => {
                let payload_result: Result<Payload, String> = match frame.read_capnp() {
                    Ok(reader) => match reader.get_root::<envelope_capnp::envelope::Reader>() {
                        Ok(envelope_reader) => {
                            capnp_envelope_to_payload(envelope_reader).map_err(|e| e.to_string())
                        }
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(e.to_string()),
                };

                match payload_result {
                    Ok(payload) => self.dispatch(payload).await,
                    Err(e) => {
                        error!("Failed to parse capnp envelope: {}", e);
                        Payload::Error {
                            code: "capnp_parse_error".to_string(),
                            message: e,
                            details: None,
                        }
                    }
                }
            }
            other => {
                error!("Unsupported content type: {:?}", other);
                Payload::Error {
                    code: "unsupported_content_type".to_string(),
                    message: format!("{:?}", other),
                    details: None,
                }
            }
        }
    }

    async fn dispatch(&self, payload: Payload) -> Payload {
        // Handle protocol-level messages
        if let Payload::Ping = &payload {
            return Payload::Pong {
                worker_id: Uuid::new_v4(),
                uptime_secs: self.start_time.elapsed().as_secs(),
            };
        }

        // Route weave_* payloads to vibeweaver if connected
        if self.should_route_to_vibeweaver(&payload) {
            if let Some(ref vibeweaver) = self.vibeweaver {
                return self.dispatch_via_vibeweaver(vibeweaver, payload).await;
            } else {
                return Payload::Error {
                    code: "vibeweaver_not_connected".to_string(),
                    message: "Python kernel requires vibeweaver connection. Configure bootstrap.connections.vibeweaver in config.".to_string(),
                    details: None,
                };
            }
        }

        // Route everything else through TypedDispatcher
        if let Some(ref server) = self.event_server {
            return self.dispatch_via_server(server, payload).await;
        }

        // No EventDualityServer configured
        Payload::Error {
            code: "no_server".to_string(),
            message: format!(
                "Tool '{}' requires EventDualityServer. Start hootenanny with full services.",
                payload_type_name(&payload)
            ),
            details: None,
        }
    }

    /// Dispatch via EventDualityServer for full tool functionality
    ///
    /// Dispatches payload via typed dispatcher.
    async fn dispatch_via_server(&self, server: &EventDualityServer, payload: Payload) -> Payload {
        use crate::api::typed_dispatcher::TypedDispatcher;
        use hooteproto::{envelope_to_payload, payload_to_request};

        // Typed dispatch path
        match payload_to_request(&payload) {
            Ok(Some(request)) => {
                // Typed path available - use TypedDispatcher
                debug!("Using typed dispatch for: {}", request.name());
                let dispatcher = TypedDispatcher::new(std::sync::Arc::new(server.clone()));
                let envelope = dispatcher.dispatch(request).await;
                envelope_to_payload(envelope)
            }
            Ok(None) => {
                // No typed request available for this payload
                Payload::Error {
                    code: "unhandled_payload".to_string(),
                    message: format!(
                        "No typed dispatch for payload type: {}",
                        payload_type_name(&payload)
                    ),
                    details: None,
                }
            }
            Err(e) => {
                // Conversion error
                Payload::Error {
                    code: e.code().to_string(),
                    message: e.message().to_string(),
                    details: None,
                }
            }
        }
    }

    /// Check if a payload should be routed to vibeweaver
    fn should_route_to_vibeweaver(&self, payload: &Payload) -> bool {
        // Typed weave payloads go to vibeweaver for Python kernel execution
        if let Payload::ToolRequest(tr) = payload {
            matches!(
                tr,
                ToolRequest::WeaveEval(_)
                    | ToolRequest::WeaveSession
                    | ToolRequest::WeaveReset(_)
                    | ToolRequest::WeaveHelp(_)
            )
        } else {
            false
        }
    }

    /// Dispatch a payload to vibeweaver via ZMQ proxy
    async fn dispatch_via_vibeweaver(
        &self,
        vibeweaver: &VibeweaverClient,
        payload: Payload,
    ) -> Payload {
        debug!("Proxying to vibeweaver: {}", payload_type_name(&payload));

        match vibeweaver.request(payload).await {
            Ok(response) => response,
            Err(e) => {
                warn!("Vibeweaver proxy error: {}", e);
                Payload::Error {
                    code: "vibeweaver_proxy_error".to_string(),
                    message: e.to_string(),
                    details: None,
                }
            }
        }
    }

}

/// Get a human-readable name for a payload type (for span naming)
fn payload_type_name(payload: &Payload) -> &'static str {
    match payload {
        Payload::Register(_) => "register",
        Payload::Ping => "ping",
        Payload::Pong { .. } => "pong",
        Payload::Shutdown { .. } => "shutdown",
        Payload::ToolRequest(tr) => tr.name(),
        Payload::ToolList { .. } => "tool_list",
        Payload::TypedResponse(_) => "typed_response",
        Payload::Error { .. } => "error",
        Payload::StreamStart { .. } => "stream_start",
        Payload::StreamSwitchChunk { .. } => "stream_switch_chunk",
        Payload::StreamStop { .. } => "stream_stop",
        Payload::TransportPlay => "transport_play",
        Payload::TransportStop => "transport_stop",
        Payload::TransportSeek { .. } => "transport_seek",
        Payload::TransportStatus => "transport_status",
        Payload::TimelineQuery { .. } => "timeline_query",
        Payload::TimelineAddMarker { .. } => "timeline_add_marker",
        Payload::TimelineEvent { .. } => "timeline_event",
    }
}

/// Convert a Vec<Bytes> to Vec<Msg> for rzmq multipart
fn frames_to_msgs(frames: &[Bytes]) -> Vec<Msg> {
    frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect()
}

/// Send a multipart message using individual send() calls with MORE flags.
///
/// rzmq's ROUTER socket has a bug in send_multipart that drops frames.
/// This workaround sends each frame individually with the MORE flag set
/// for all but the last frame.
async fn send_multipart_individually(socket: &Socket, msgs: Vec<Msg>) -> Result<()> {
    let last_idx = msgs.len().saturating_sub(1);
    let total_frames = msgs.len();
    for (i, mut msg) in msgs.into_iter().enumerate() {
        if i < last_idx {
            msg.set_flags(MsgFlags::MORE);
        }
        if let Err(e) = socket.send(msg).await {
            // Log the actual error from rzmq for debugging
            tracing::debug!(
                frame = i,
                total = total_frames,
                error = %e,
                "ROUTER send failed"
            );
            return Err(anyhow::anyhow!(
                "Failed to send frame {} of {} multipart: {}",
                i,
                total_frames,
                e
            ));
        }
    }
    Ok(())
}