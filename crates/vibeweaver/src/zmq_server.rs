//! ZMQ ROUTER server for Vibeweaver
//!
//! Binds a ROUTER socket and handles HOOT01 + Cap'n Proto messages from:
//! - Hootenanny (proxying weave_* tool calls)

use anyhow::{Context, Result};
use bytes::Bytes;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope, Command, ContentType,
    HootFrame, Payload, PROTOCOL_VERSION,
};
use pyo3::prelude::*;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::kernel::Kernel;
use crate::session::Session;

/// Convert frames to ZmqMessage
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
    pub async fn run(self, mut shutdown_rx: tokio::sync::broadcast::Receiver<()>) -> Result<()> {
        let mut socket = RouterSocket::new();
        socket
            .bind(&self.config.bind_address)
            .await
            .with_context(|| format!("Failed to bind to {}", self.config.bind_address))?;

        info!(
            "Vibeweaver ZMQ server listening on {}",
            self.config.bind_address
        );

        loop {
            tokio::select! {
                result = socket.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.handle_message(&mut socket, msg).await {
                                error!("Error handling message: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Error receiving message: {}", e);
                        }
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

    /// Handle a single incoming message
    async fn handle_message(&self, socket: &mut RouterSocket, msg: ZmqMessage) -> Result<()> {
        let frames: Vec<Bytes> = msg.iter().map(|f| Bytes::copy_from_slice(f)).collect();

        // Only accept HOOT01 frames
        if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            warn!("Received non-HOOT01 message, ignoring");
            return Ok(());
        }

        let (identity, frame) = HootFrame::from_frames_with_identity(&frames)?;

        debug!(
            "HOOT01 {:?} from service={} request_id={}",
            frame.command, frame.service, frame.request_id
        );

        match frame.command {
            Command::Heartbeat => {
                let response = HootFrame::heartbeat("vibeweaver");
                let reply_frames = response.to_frames_with_identity(&identity);
                let reply = frames_to_zmq_message(&reply_frames);
                socket.send(reply).await?;
                debug!("Heartbeat response sent");
            }

            Command::Request => {
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
                let response_msg = payload_to_capnp_envelope(frame.request_id, &result_payload)?;

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
                let reply = frames_to_zmq_message(&reply_frames);
                socket.send(reply).await?;
            }

            other => {
                debug!("Ignoring command: {:?}", other);
            }
        }

        Ok(())
    }

    /// Dispatch a payload to the appropriate handler
    async fn dispatch(&self, payload: Payload) -> Payload {
        match payload {
            Payload::Ping => Payload::Pong {
                worker_id: Uuid::new_v4(),
                uptime_secs: self.start_time.elapsed().as_secs(),
            },

            // Handle ToolCall payloads (how hootenanny sends weave_* calls)
            Payload::ToolCall { name, args } => match name.as_str() {
                "weave_eval" => {
                    let code = args
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    self.weave_eval(code).await
                }
                "weave_session" => self.weave_session().await,
                "weave_reset" => {
                    let clear_session = args
                        .get("clear_session")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    self.weave_reset(clear_session).await
                }
                "weave_help" => {
                    let topic = args.get("topic").and_then(|v| v.as_str());
                    self.weave_help(topic).await
                }
                _ => Payload::Error {
                    code: "unknown_tool".to_string(),
                    message: format!("Unknown tool: {}", name),
                    details: None,
                },
            },

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
                        Ok(s) => Payload::Success {
                            result: serde_json::json!({
                                "result": s,
                                "type": "expression"
                            }),
                        },
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
                    Ok((stdout, stderr)) => Payload::Success {
                        result: serde_json::json!({
                            "stdout": stdout,
                            "stderr": stderr,
                            "type": "statement"
                        }),
                    },
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
        match session.as_ref() {
            Some(s) => Payload::Success {
                result: serde_json::json!({
                    "id": s.id.to_string(),
                    "name": s.name,
                    "vibe": s.vibe,
                }),
            },
            None => Payload::Success {
                result: serde_json::json!({
                    "session": null,
                    "message": "No active session"
                }),
            },
        }
    }

    /// Reset the kernel
    async fn weave_reset(&self, _clear_session: bool) -> Payload {
        let kernel = self.kernel.read().await;
        match kernel.clear() {
            Ok(()) => Payload::Success {
                result: serde_json::json!({
                    "reset": true,
                    "message": "Kernel reset successfully"
                }),
            },
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

        Payload::Success {
            result: serde_json::json!({
                "help": help_text
            }),
        }
    }
}
