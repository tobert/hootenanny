//! ZMQ ROUTER server for Vibeweaver
//!
//! Binds a ROUTER socket and handles HOOT01 + Cap'n Proto messages from:
//! - Hootenanny (proxying weave_* tool calls)

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope, Command, ContentType,
    HootFrame, Payload, ResponseEnvelope, PROTOCOL_VERSION,
};
use hooteproto::request::ToolRequest;
use hooteproto::responses::{
    ToolResponse, WeaveEvalResponse, WeaveHelpResponse, WeaveOutputType, WeaveResetResponse,
    WeaveSessionInfo, WeaveSessionResponse,
};
use pyo3::prelude::*;
use rzmq::{Context, Msg, MsgFlags, Socket, SocketType};
use rzmq::socket::options::LINGER;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::kernel::Kernel;
use crate::session::Session;

/// Convert frames to Vec<Msg> for rzmq multipart
fn frames_to_msgs(frames: &[Bytes]) -> Vec<Msg> {
    frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect()
}

/// Send multipart using individual send() calls with MORE flags.
/// rzmq's ROUTER socket has a bug in send_multipart that drops frames.
async fn send_multipart_individually(socket: &Socket, msgs: Vec<Msg>) -> anyhow::Result<()> {
    use anyhow::Context;
    let last_idx = msgs.len().saturating_sub(1);
    for (i, mut msg) in msgs.into_iter().enumerate() {
        if i < last_idx {
            msg.set_flags(MsgFlags::MORE);
        }
        socket.send(msg).await
            .with_context(|| format!("Failed to send frame {} of multipart", i))?;
    }
    Ok(())
}

/// ZMQ server configuration
pub struct ServerConfig {
    pub bind_address: String,
    pub worker_name: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "tcp://0.0.0.0:5575".to_string(),
            worker_name: "vibeweaver".to_string(),
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
        let context = Context::new()
            .with_context(|| "Failed to create ZMQ context")?;
        let socket = context
            .socket(SocketType::Router)
            .with_context(|| "Failed to create ROUTER socket")?;

        // Set LINGER to 0 for immediate close
        if let Err(e) = socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await {
            warn!("Failed to set LINGER: {}", e);
        }

        socket
            .bind(&self.config.bind_address)
            .await
            .with_context(|| format!("Failed to bind to {}", self.config.bind_address))?;

        info!(
            "Vibeweaver ZMQ server listening on {}",
            self.config.bind_address
        );

        // Channel for sending responses back to the main loop for transmission
        let (response_tx, mut response_rx) = mpsc::channel::<Vec<Msg>>(256);

        // Wrap self in Arc for sharing across spawned tasks
        let server = Arc::new(self);

        loop {
            tokio::select! {
                result = socket.recv_multipart() => {
                    match result {
                        Ok(msgs) => {
                            let frames: Vec<Bytes> = msgs
                                .iter()
                                .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                                .collect();

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
                                            let reply = frames_to_msgs(&reply_frames);
                                            // Use individual send() - rzmq ROUTER send_multipart has a bug
                                            if let Err(e) = send_multipart_individually(&socket, reply).await {
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

                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, stopping server");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a request and return the ZMQ message to send as response
    async fn handle_request(&self, identity: Vec<Bytes>, frame: HootFrame) -> Vec<Msg> {
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
        frames_to_msgs(&reply_frames)
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
}