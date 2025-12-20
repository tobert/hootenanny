//! ZMQ ROUTER server for Luanette
//!
//! Binds a ROUTER socket and handles HOOT01 + Cap'n Proto messages from:
//! - Holler (MCP gateway)
//! - Hootenanny (proxying tools)
//! - holler CLI (direct access)

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope,
    Command, ContentType, HootFrame, Payload, PROTOCOL_VERSION,
};
use rzmq::{Context, Msg, MsgFlags, Socket, SocketType};
use rzmq::socket::options::LINGER;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::dispatch::Dispatcher;

/// Frames to Vec<Msg> helper for rzmq multipart
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
    pub _worker_name: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "tcp://0.0.0.0:5570".to_string(),
            _worker_name: "luanette".to_string(),
        }
    }
}

/// ZMQ ROUTER server
pub struct Server {
    config: ServerConfig,
    dispatcher: Arc<RwLock<Dispatcher>>,
    start_time: Instant,
}

impl Server {
    pub fn new(config: ServerConfig, dispatcher: Dispatcher) -> Self {
        Self {
            config,
            dispatcher: Arc::new(RwLock::new(dispatcher)),
            start_time: Instant::now(),
        }
    }

    /// Run the server until shutdown signal
    #[instrument(skip(self, shutdown_rx), fields(bind = %self.config.bind_address))]
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

        info!("Luanette ZMQ server listening on {}", self.config.bind_address);

        loop {
            tokio::select! {
                result = socket.recv_multipart() => {
                    match result {
                        Ok(msgs) => {
                            if let Err(e) = self.handle_message(&socket, msgs).await {
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
    async fn handle_message(&self, socket: &Socket, msgs: Vec<Msg>) -> Result<()> {
        // Convert ZMQ message to frames
        let frames: Vec<Bytes> = msgs
            .iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        // Only accept HOOT01 frames
        if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            warn!("Received non-HOOT01 message, ignoring");
            return Ok(());
        }

        // Parse HOOT01 frame (with identity for ROUTER socket)
        let (identity, frame) = HootFrame::from_frames_with_identity(&frames)?;

        debug!(
            "HOOT01 {:?} from service={} request_id={}",
            frame.command, frame.service, frame.request_id
        );

        match frame.command {
            Command::Heartbeat => {
                // Respond with heartbeat - use individual send() as rzmq ROUTER send_multipart has a bug
                let response = HootFrame::heartbeat("luanette");
                let reply_frames = response.to_frames_with_identity(&identity);
                let reply = frames_to_msgs(&reply_frames);
                send_multipart_individually(socket, reply).await?;
                debug!("Heartbeat response sent");
            }

            Command::Request => {
                // Parse Cap'n Proto envelope to Payload
                let payload_result = match frame.read_capnp() {
                    Ok(reader) => match reader.get_root::<envelope_capnp::envelope::Reader>() {
                        Ok(envelope_reader) => capnp_envelope_to_payload(envelope_reader).map_err(|e| e.to_string()),
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) => Err(e.to_string()),
                };

                let result_payload = match payload_result {
                    Ok(payload) => {
                        // Dispatch to handler
                        self.dispatch(payload).await
                    }
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

                // Serialize and send
                let bytes = capnp::serialize::write_message_to_words(&response_msg);
                let response_frame = HootFrame {
                    command: Command::Reply,
                    content_type: ContentType::CapnProto,
                    request_id: frame.request_id,
                    service: "luanette".to_string(),
                    traceparent: None,
                    body: bytes.into(),
                };

                let reply_frames = response_frame.to_frames_with_identity(&identity);
                let reply = frames_to_msgs(&reply_frames);
                // Use individual send() - rzmq ROUTER send_multipart has a bug
                send_multipart_individually(socket, reply).await?;
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
                worker_id: Uuid::new_v4(), // TODO: use a stable worker ID
                uptime_secs: self.start_time.elapsed().as_secs(),
            },

            Payload::LuaEval { code, params } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.lua_eval(&code, params).await
            }

            Payload::JobStatus { job_id } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.job_status(&job_id).await
            }

            Payload::JobList { status } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.job_list(status.as_deref()).await
            }

            Payload::JobCancel { job_id } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.job_cancel(&job_id).await
            }

            Payload::JobExecute {
                script_hash,
                params,
                tags,
            } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.job_execute(&script_hash, params, tags).await
            }

            Payload::JobPoll {
                job_ids,
                timeout_ms,
                mode,
            } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.job_poll(job_ids, timeout_ms, mode).await
            }

            Payload::ScriptStore {
                content,
                tags,
                creator,
            } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.script_store(&content, tags, creator).await
            }

            Payload::ScriptSearch { tag, creator, vibe } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.script_search(tag, creator, vibe).await
            }

            Payload::LuaDescribe { script_hash } => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.lua_describe(&script_hash).await
            }

            Payload::ListTools => {
                let dispatcher = self.dispatcher.read().await;
                dispatcher.list_tools().await
            }

            // Not implemented yet
            Payload::TimelineEvent { .. } => {
                warn!("TimelineEvent not yet implemented");
                Payload::Error {
                    code: "not_implemented".to_string(),
                    message: "TimelineEvent handling not yet implemented".to_string(),
                    details: None,
                }
            }

            // Pass through other payloads
            other => {
                warn!("Unhandled payload type: {:?}", other);
                Payload::Error {
                    code: "unhandled_payload".to_string(),
                    message: "Luanette does not handle this payload type".to_string(),
                    details: Some(serde_json::to_value(&other).unwrap_or_default()),
                }
            }
        }
    }
}