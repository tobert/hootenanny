//! ZMQ ROUTER server for Luanette
//!
//! Binds a ROUTER socket and handles hooteproto messages from:
//! - Holler (MCP gateway)
//! - Chaosgarden (real-time triggers)
//! - holler CLI (direct access)

use anyhow::{Context, Result};
use hooteproto::{Envelope, Payload};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::dispatch::Dispatcher;

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
        let mut socket = RouterSocket::new();
        socket
            .bind(&self.config.bind_address)
            .await
            .with_context(|| format!("Failed to bind to {}", self.config.bind_address))?;

        info!("Luanette ZMQ server listening on {}", self.config.bind_address);

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
        // ROUTER sockets prepend identity frame(s)
        // Frame 0: identity
        // Frame 1: payload
        let identity = msg
            .get(0)
            .context("Missing identity frame")?
            .to_vec();

        let payload_bytes = msg
            .get(1)
            .context("Missing payload frame")?;

        debug!("Received {} bytes", payload_bytes.len());

        // Parse the envelope from MsgPack
        let envelope: Envelope = rmp_serde::from_slice(payload_bytes)
            .with_context(|| "Failed to parse MsgPack envelope")?;

        // Dispatch and get response
        let response_payload = self.dispatch(envelope.payload).await;

        // Build response envelope
        let response = Envelope {
            id: envelope.id,
            traceparent: envelope.traceparent,
            payload: response_payload,
        };

        // Serialize response to MsgPack
        let response_bytes = rmp_serde::to_vec(&response)?;
        debug!("Sending response: {} bytes", response_bytes.len());

        // Send response with identity frame
        let mut reply = ZmqMessage::from(identity);
        reply.push_back(response_bytes.into());
        socket.send(reply).await?;

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