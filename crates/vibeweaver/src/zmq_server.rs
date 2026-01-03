//! ZMQ ROUTER server for Vibeweaver
//!
//! Binds a ROUTER socket and handles HOOT01 + Cap'n Proto messages from:
//! - Hootenanny (proxying weave_* tool calls)

use anyhow::Result;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope, Command, ContentType,
    HootFrame, Payload, ResponseEnvelope, PROTOCOL_VERSION,
};
use hooteproto::request::ToolRequest;
use hooteproto::responses::{
    ToolResponse, WeaveEvalResponse, WeaveHelpResponse, WeaveOutputType, WeaveResetResponse,
    WeaveSessionInfo, WeaveSessionResponse,
};
use hooteproto::socket_config::{create_router_and_bind, ZmqContext, Multipart};
use pyo3::prelude::*;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::broadcast::BroadcastHandler;
use crate::callbacks::{fire_artifact_callbacks, fire_beat_callbacks, fire_marker_callbacks};
use crate::kernel::Kernel;
use crate::session::Session;
use crate::zmq_client::{Broadcast, BroadcastReceiver};

/// Boxed sink type for sending messages
type BoxedSink = Pin<Box<dyn futures::Sink<Multipart, Error = tmq::TmqError> + Send>>;

/// Convert tmq Multipart to Vec<Bytes> for frame processing
fn multipart_to_frames(mp: Multipart) -> Vec<Bytes> {
    mp.into_iter()
        .map(|msg| Bytes::from(msg.to_vec()))
        .collect()
}

/// Convert Vec<Bytes> frames to tmq Multipart
fn frames_to_multipart(frames: &[Bytes]) -> Multipart {
    frames.iter()
        .map(|f| f.to_vec())
        .collect::<Vec<_>>()
        .into()
}

/// ZMQ server configuration
pub struct ServerConfig {
    pub bind_address: String,
    pub worker_name: String,
    /// Hootenanny PUB socket endpoint for broadcasts
    pub broadcast_endpoint: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "tcp://0.0.0.0:5575".to_string(),
            worker_name: "vibeweaver".to_string(),
            broadcast_endpoint: None,
        }
    }
}

/// ZMQ ROUTER server for vibeweaver
pub struct Server {
    config: ServerConfig,
    kernel: Arc<RwLock<Kernel>>,
    session: Arc<RwLock<Option<Session>>>,
    start_time: Instant,
}

impl Server {
    /// Create a new server with kernel
    pub fn new(config: ServerConfig, kernel: Kernel) -> Self {
        Self {
            config,
            kernel: Arc::new(RwLock::new(kernel)),
            session: Arc::new(RwLock::new(None)),
            start_time: Instant::now(),
        }
    }

    /// Run the server until shutdown signal
    ///
    /// Uses concurrent request handling to avoid deadlocks when Python code
    /// calls back to hootenanny during request processing.
    pub async fn run(self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Result<()> {
        let context = ZmqContext::new();
        let socket = create_router_and_bind(&context, &self.config.bind_address, &self.config.worker_name)?;

        info!(
            "Vibeweaver ZMQ server listening on {}",
            self.config.bind_address
        );

        // Split socket into tx/rx halves
        let (tx, mut rx) = socket.split();
        let socket_tx: Arc<Mutex<BoxedSink>> = Arc::new(Mutex::new(Box::pin(tx)));

        // Channel for sending responses back to the main loop for transmission
        let (response_tx, mut response_rx) = mpsc::channel::<Multipart>(256);

        // Wrap self in Arc for sharing across spawned tasks
        let server = Arc::new(self);

        // Spawn broadcast listener if endpoint configured
        if let Some(ref broadcast_endpoint) = server.config.broadcast_endpoint {
            let endpoint = broadcast_endpoint.clone();
            let shutdown_rx_broadcast = shutdown_rx.resubscribe();
            tokio::spawn(async move {
                Self::broadcast_listener(endpoint, shutdown_rx_broadcast).await;
            });
        }

        loop {
            tokio::select! {
                result = rx.next() => {
                    match result {
                        Some(Ok(mp)) => {
                            let frames = multipart_to_frames(mp);

                            // Only accept HOOT01 frames
                            if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
                                warn!("Received non-HOOT01 message, ignoring");
                                continue;
                            }

                            match HootFrame::from_frames_with_identity(&frames) {
                                Ok((identity, frame)) => {
                                    debug!(
                                        "HOOT01 {:?} from service={} request_id={}",
                                        frame.command, frame.service, frame.request_id
                                    );

                                    match frame.command {
                                        Command::Heartbeat => {
                                            // Handle heartbeats synchronously (fast path)
                                            let response = HootFrame::heartbeat("vibeweaver");
                                            let reply_frames = response.to_frames_with_identity(&identity);
                                            let reply = frames_to_multipart(&reply_frames);
                                            if let Err(e) = socket_tx.lock().await.send(reply).await {
                                                error!("Failed to send heartbeat response: {}", e);
                                            }
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
                                        other => {
                                            debug!("Ignoring command: {:?}", other);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to parse HOOT01 frame: {}", e);
                                }
                            }
                        }
                        Some(Err(e)) => {
                            error!("Error receiving message: {}", e);
                        }
                        None => {
                            warn!("Socket stream ended");
                            break;
                        }
                    }
                }

                // Send queued responses
                Some(reply) = response_rx.recv() => {
                    if let Err(e) = socket_tx.lock().await.send(reply).await {
                        error!("Failed to send response: {}", e);
                    }
                }

                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, stopping server");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a request and return the multipart message to send as response
    async fn handle_request(&self, identity: Vec<Bytes>, frame: HootFrame) -> Multipart {
        // Parse Cap'n Proto envelope to Payload
        let payload_result = match frame.read_capnp() {
            Ok(reader) => match reader.get_root::<envelope_capnp::envelope::Reader>() {
                Ok(envelope_reader) => {
                    capnp_envelope_to_payload(envelope_reader).map_err(|e| e.to_string())
                }
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e.to_string()),
        };

        let result_payload = match payload_result {
            Ok(payload) => self.dispatch(payload).await,
            Err(e) => {
                error!("Failed to parse capnp envelope: {}", e);
                Payload::Error {
                    code: "capnp_parse_error".to_string(),
                    message: e,
                    details: None,
                }
            }
        };

        // Convert result to Cap'n Proto envelope
        let response_msg = match payload_to_capnp_envelope(frame.request_id, &result_payload) {
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
            service: "vibeweaver".to_string(),
            traceparent: None,
            body: bytes.into(),
        };

        let reply_frames = response_frame.to_frames_with_identity(&identity);
        frames_to_multipart(&reply_frames)
    }

    /// Dispatch a payload to the appropriate handler
    async fn dispatch(&self, payload: Payload) -> Payload {
        match payload {
            Payload::Ping => Payload::Pong {
                worker_id: Uuid::new_v4(),
                uptime_secs: self.start_time.elapsed().as_secs(),
            },

            // Handle typed weave payloads
            Payload::ToolRequest(ToolRequest::WeaveEval(req)) => self.weave_eval(&req.code).await,
            Payload::ToolRequest(ToolRequest::WeaveSession) => self.weave_session().await,
            Payload::ToolRequest(ToolRequest::WeaveReset(req)) => self.weave_reset(req.clear_session).await,
            Payload::ToolRequest(ToolRequest::WeaveHelp(req)) => self.weave_help(req.topic.as_deref()).await,

            other => {
                warn!("Unhandled payload type: {:?}", other);
                Payload::Error {
                    code: "unhandled_payload".to_string(),
                    message: "Vibeweaver does not handle this payload type".to_string(),
                    details: Some(serde_json::to_value(&other).unwrap_or_default()),
                }
            }
        }
    }

    /// Execute Python code in the kernel
    async fn weave_eval(&self, code: &str) -> Payload {
        let kernel = self.kernel.read().await;

        // Try to evaluate as expression first, fall back to exec
        let result = kernel.eval(code);

        match result {
            Ok(py_obj) => {
                // Try to convert result to JSON-friendly value
                Python::with_gil(|py| {
                    let result_str = py_obj.bind(py).repr().map(|r| r.to_string());
                    match result_str {
                        Ok(s) => Payload::TypedResponse(ResponseEnvelope::success(
                            ToolResponse::WeaveEval(WeaveEvalResponse {
                                output_type: WeaveOutputType::Expression,
                                result: Some(s),
                                stdout: None,
                                stderr: None,
                            }),
                        )),
                        Err(e) => Payload::Error {
                            code: "repr_error".to_string(),
                            message: e.to_string(),
                            details: None,
                        },
                    }
                })
            }
            Err(_) => {
                // Try exec for statements
                match kernel.exec_with_capture(code) {
                    Ok((stdout, stderr)) => Payload::TypedResponse(ResponseEnvelope::success(
                        ToolResponse::WeaveEval(WeaveEvalResponse {
                            output_type: WeaveOutputType::Statement,
                            result: None,
                            stdout: Some(stdout),
                            stderr: Some(stderr),
                        }),
                    )),
                    Err(e) => Payload::Error {
                        code: "python_error".to_string(),
                        message: e.to_string(),
                        details: None,
                    },
                }
            }
        }
    }

    /// Get current session state
    async fn weave_session(&self) -> Payload {
        let session = self.session.read().await;
        let response = match session.as_ref() {
            Some(s) => WeaveSessionResponse {
                session: Some(WeaveSessionInfo {
                    id: s.id.to_string(),
                    name: s.name.clone(),
                    vibe: s.vibe.clone(),
                }),
                message: None,
            },
            None => WeaveSessionResponse {
                session: None,
                message: Some("No active session".to_string()),
            },
        };
        Payload::TypedResponse(ResponseEnvelope::success(ToolResponse::WeaveSession(response)))
    }

    /// Reset the kernel
    async fn weave_reset(&self, _clear_session: bool) -> Payload {
        let kernel = self.kernel.read().await;
        match kernel.clear() {
            Ok(()) => Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::WeaveReset(WeaveResetResponse {
                    reset: true,
                    message: "Kernel reset successfully".to_string(),
                }),
            )),
            Err(e) => Payload::Error {
                code: "reset_error".to_string(),
                message: e.to_string(),
                details: None,
            },
        }
    }

    /// Get help documentation
    async fn weave_help(&self, topic: Option<&str>) -> Payload {
        let help_text = match topic {
            Some("api") => {
                r#"Vibeweaver Python API:
- session(name, vibe): Create or load a session
- tempo(bpm): Set tempo
- sample(space, prompt): Generate audio sample
- schedule(content, at, duration): Schedule content at beat
- play(), pause(), stop(), seek(beat): Transport controls
"#
            }
            Some("session") => {
                r#"Session Management:
Sessions persist state across evaluations.
- weave_session: Get current session info
- weave_reset: Clear kernel state
"#
            }
            Some("examples") => {
                r#"Examples:
>>> 1 + 1
2
>>> x = 42
>>> x * 2
84
>>> import math
>>> math.sqrt(16)
4.0
"#
            }
            _ => {
                r#"Vibeweaver - Python Kernel for AI Music Agents

Tools:
- weave_eval: Execute Python code
- weave_session: Get session state
- weave_reset: Reset kernel
- weave_help: Show this help

Use weave_help(topic="api|session|examples") for more info.
"#
            }
        };

        Payload::TypedResponse(ResponseEnvelope::success(ToolResponse::WeaveHelp(
            WeaveHelpResponse {
                help: help_text.to_string(),
                topic: topic.map(|t| t.to_string()),
            },
        )))
    }

    /// Broadcast listener task
    ///
    /// Connects to hootenanny's PUB socket and receives broadcast events.
    /// Dispatches to registered Python callbacks.
    async fn broadcast_listener(
        endpoint: String,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) {
        info!("Broadcast listener connecting to {}", endpoint);

        let mut receiver = match BroadcastReceiver::connect(&endpoint) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to connect broadcast receiver: {}", e);
                return;
            }
        };

        info!("Broadcast listener connected, waiting for events");

        loop {
            tokio::select! {
                result = receiver.recv() => {
                    match result {
                        Ok(broadcast) => {
                            debug!("Received broadcast: {:?}", broadcast);
                            Self::dispatch_broadcast(broadcast).await;
                        }
                        Err(e) => {
                            error!("Broadcast receive error: {}", e);
                            // Retry after brief delay
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Broadcast listener shutting down");
                    break;
                }
            }
        }
    }

    /// Dispatch a broadcast to registered callbacks and the handler
    async fn dispatch_broadcast(broadcast: Broadcast) {
        // First, forward to BroadcastHandler for job waiters and state updates
        if let Some(handler) = BroadcastHandler::global() {
            let updates = handler.handle(broadcast.clone()).await;
            if !updates.is_empty() {
                debug!("BroadcastHandler processed {} state updates", updates.len());
            }
        }

        // Then fire Python callbacks
        match broadcast {
            Broadcast::BeatTick { beat, tempo_bpm: _ } => {
                fire_beat_callbacks(beat);
            }
            Broadcast::MarkerReached { name, beat } => {
                fire_marker_callbacks(&name, beat);
            }
            Broadcast::ArtifactCreated {
                artifact_id,
                content_hash,
                tags,
            } => {
                fire_artifact_callbacks(&artifact_id, &content_hash, &tags);
            }
            Broadcast::JobStateChanged { job_id, state, .. } => {
                debug!("Job {} state changed to {}", job_id, state);
            }
            Broadcast::TransportStateChanged { state, position_beats } => {
                debug!("Transport {} at beat {}", state, position_beats);
            }
            Broadcast::Unknown { topic, .. } => {
                debug!("Unknown broadcast topic: {}", topic);
            }
        }
    }
}