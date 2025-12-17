//! Cap'n Proto-based ZMQ server for chaosgarden
//!
//! Implements the Jupyter-inspired 5-socket protocol with HOOT01 frames
//! and Cap'n Proto envelope messages.
//!
//! Socket types:
//! - control (ROUTER): Priority commands (shutdown, interrupt)
//! - shell (ROUTER): Normal commands (transport, streams)
//! - iopub (PUB): Event broadcasts (state changes, metrics)
//! - heartbeat (REP): Liveness detection
//! - query (REP): Trustfall queries

use anyhow::{Context, Result};
use bytes::Bytes;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope, Command, ContentType,
    HootFrame, Payload, PROTOCOL_VERSION,
};
use std::sync::Arc;
use tokio::select;
use tracing::{debug, error, info, warn};
use zeromq::{PubSocket, RepSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::daemon::GardenDaemon;
use crate::ipc::{Message, ShellReply, ShellRequest};
use uuid::Uuid;

/// Frames to ZmqMessage helper
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

/// ZMQ server using Cap'n Proto for chaosgarden
pub struct CapnpGardenServer {
    endpoints: crate::ipc::GardenEndpoints,
}

impl CapnpGardenServer {
    pub fn new(endpoints: crate::ipc::GardenEndpoints) -> Self {
        Self { endpoints }
    }

    /// Run the server with the garden daemon handler
    pub async fn run(self, handler: Arc<GardenDaemon>) -> Result<()> {
        // Bind all 5 sockets (Jupyter-inspired protocol)

        // Control socket (ROUTER) - priority commands
        let mut control_socket = RouterSocket::new();
        control_socket
            .bind(&self.endpoints.control)
            .await
            .context("Failed to bind control socket")?;
        info!("📡 control socket bound to {}", self.endpoints.control);

        // Shell socket (ROUTER) - normal commands
        let mut shell_socket = RouterSocket::new();
        shell_socket
            .bind(&self.endpoints.shell)
            .await
            .context("Failed to bind shell socket")?;
        info!("📡 shell socket bound to {}", self.endpoints.shell);

        // IOPub socket (PUB) - event broadcasts
        let mut iopub_socket = PubSocket::new();
        iopub_socket
            .bind(&self.endpoints.iopub)
            .await
            .context("Failed to bind iopub socket")?;
        info!("📡 iopub socket bound to {}", self.endpoints.iopub);

        // Heartbeat socket (REP) - liveness detection
        let mut heartbeat_socket = RepSocket::new();
        heartbeat_socket
            .bind(&self.endpoints.heartbeat)
            .await
            .context("Failed to bind heartbeat socket")?;
        info!("📡 heartbeat socket bound to {}", self.endpoints.heartbeat);

        // Query socket (REP) - Trustfall queries
        let mut query_socket = RepSocket::new();
        query_socket
            .bind(&self.endpoints.query)
            .await
            .context("Failed to bind query socket")?;
        info!("📡 query socket bound to {}", self.endpoints.query);

        info!(
            "🎵 chaosgarden server ready (5 sockets bound)"
        );

        // Main event loop - handle all sockets concurrently
        loop {
            select! {
                // Control socket - priority commands
                msg = control_socket.recv() => {
                    match msg {
                        Ok(zmq_msg) => {
                            if let Err(e) = self.handle_router_message(
                                &mut control_socket,
                                &handler,
                                zmq_msg,
                                "control"
                            ).await {
                                error!("Error handling control message: {}", e);
                            }
                        }
                        Err(e) => error!("Control socket recv error: {}", e),
                    }
                }

                // Shell socket - normal commands
                msg = shell_socket.recv() => {
                    match msg {
                        Ok(zmq_msg) => {
                            if let Err(e) = self.handle_router_message(
                                &mut shell_socket,
                                &handler,
                                zmq_msg,
                                "shell"
                            ).await {
                                error!("Error handling shell message: {}", e);
                            }
                        }
                        Err(e) => error!("Shell socket recv error: {}", e),
                    }
                }

                // Heartbeat socket - liveness detection
                msg = heartbeat_socket.recv() => {
                    match msg {
                        Ok(zmq_msg) => {
                            if let Err(e) = self.handle_heartbeat(&mut heartbeat_socket, zmq_msg).await {
                                error!("Error handling heartbeat: {}", e);
                            }
                        }
                        Err(e) => error!("Heartbeat socket recv error: {}", e),
                    }
                }

                // Query socket - Trustfall queries
                msg = query_socket.recv() => {
                    match msg {
                        Ok(zmq_msg) => {
                            if let Err(e) = self.handle_query(&mut query_socket, &handler, zmq_msg).await {
                                error!("Error handling query: {}", e);
                            }
                        }
                        Err(e) => error!("Query socket recv error: {}", e),
                    }
                }
            }
        }
    }

    /// Handle messages on ROUTER sockets (control/shell)
    async fn handle_router_message(
        &self,
        socket: &mut RouterSocket,
        handler: &Arc<GardenDaemon>,
        msg: ZmqMessage,
        channel: &str,
    ) -> Result<()> {
        // Convert to Bytes frames
        let frames: Vec<Bytes> = msg.iter().map(|f| Bytes::copy_from_slice(f)).collect();

        // Only accept HOOT01 frames
        if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            warn!("[{}] Received non-HOOT01 message, ignoring", channel);
            return Ok(());
        }

        // Parse HOOT01 frame
        let (identity, frame) = HootFrame::from_frames_with_identity(&frames)?;

        debug!(
            "[{}] HOOT01 {:?} from service={} request_id={}",
            channel, frame.command, frame.service, frame.request_id
        );

        match frame.command {
            Command::Heartbeat => {
                // Respond with heartbeat
                let response = HootFrame::heartbeat("chaosgarden");
                let reply_frames = response.to_frames_with_identity(&identity);
                let reply = frames_to_zmq_message(&reply_frames);
                socket.send(reply).await?;
                debug!("[{}] Heartbeat response sent", channel);
            }

            Command::Request => {
                match frame.content_type {
                    ContentType::Json => {
                        // JSON request - parse as Message<ShellRequest>
                        let result = self
                            .handle_json_shell_request(handler, &frame.body, frame.request_id)
                            .await;

                        // Send JSON response
                        let reply_frames = result.to_frames_with_identity(&identity);
                        let reply = frames_to_zmq_message(&reply_frames);
                        socket.send(reply).await?;
                    }
                    ContentType::CapnProto => {
                        // Cap'n Proto request - parse as Payload
                        let payload_result = match frame.read_capnp() {
                            Ok(reader) => {
                                match reader.get_root::<envelope_capnp::envelope::Reader>() {
                                    Ok(envelope_reader) => capnp_envelope_to_payload(envelope_reader)
                                        .map_err(|e| e.to_string()),
                                    Err(e) => Err(e.to_string()),
                                }
                            }
                            Err(e) => Err(e.to_string()),
                        };

                        let result_payload = match payload_result {
                            Ok(payload) => {
                                // Dispatch to handler
                                self.dispatch_payload(handler, payload).await
                            }
                            Err(e) => {
                                error!("[{}] Failed to parse capnp envelope: {}", channel, e);
                                Payload::Error {
                                    code: "capnp_parse_error".to_string(),
                                    message: e,
                                    details: None,
                                }
                            }
                        };

                        // Convert result to Cap'n Proto envelope
                        let response_msg =
                            payload_to_capnp_envelope(frame.request_id, &result_payload)?;

                        // Serialize and send
                        let bytes = capnp::serialize::write_message_to_words(&response_msg);
                        let response_frame = HootFrame {
                            command: Command::Reply,
                            content_type: ContentType::CapnProto,
                            request_id: frame.request_id,
                            service: "chaosgarden".to_string(),
                            traceparent: None,
                            body: bytes.into(),
                        };

                        let reply_frames = response_frame.to_frames_with_identity(&identity);
                        let reply = frames_to_zmq_message(&reply_frames);
                        socket.send(reply).await?;
                    }
                    other => {
                        warn!(
                            "[{}] Unsupported content type: {:?}, ignoring",
                            channel, other
                        );
                    }
                }
            }

            other => {
                debug!("[{}] Ignoring command: {:?}", channel, other);
            }
        }

        Ok(())
    }

    /// Handle heartbeat messages (REP socket - simple echo)
    async fn handle_heartbeat(&self, socket: &mut RepSocket, msg: ZmqMessage) -> Result<()> {
        let frames: Vec<Bytes> = msg.iter().map(|f| Bytes::copy_from_slice(f)).collect();

        // Check for HOOT01 frame
        if frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            // Parse and respond with HOOT01 heartbeat
            match HootFrame::from_frames(&frames) {
                Ok(_frame) => {
                    let response = HootFrame::heartbeat("chaosgarden");
                    let reply_frames = response.to_frames();
                    let reply = frames_to_zmq_message(&reply_frames);
                    socket.send(reply).await?;
                    debug!("💓 Heartbeat response sent");
                }
                Err(e) => {
                    warn!("Failed to parse heartbeat frame: {}", e);
                    // Echo back anyway for compatibility
                    socket.send(msg).await?;
                }
            }
        } else {
            // Legacy heartbeat - just echo back
            socket.send(msg).await?;
        }

        Ok(())
    }

    /// Handle Trustfall query messages (REP socket)
    async fn handle_query(
        &self,
        socket: &mut RepSocket,
        handler: &Arc<GardenDaemon>,
        msg: ZmqMessage,
    ) -> Result<()> {
        let frames: Vec<Bytes> = msg.iter().map(|f| Bytes::copy_from_slice(f)).collect();

        // Check for HOOT01 frame
        if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            warn!("[query] Received non-HOOT01 message");
            // Send error response
            let error_payload = Payload::Error {
                code: "protocol_error".to_string(),
                message: "Expected HOOT01 frame".to_string(),
                details: None,
            };
            let error_msg = payload_to_capnp_envelope(Uuid::nil(), &error_payload)?;
            let bytes = capnp::serialize::write_message_to_words(&error_msg);
            let response_frame = HootFrame {
                command: Command::Reply,
                content_type: ContentType::CapnProto,
                request_id: Uuid::nil(),
                service: "chaosgarden".to_string(),
                traceparent: None,
                body: bytes.into(),
            };
            let reply = frames_to_zmq_message(&response_frame.to_frames());
            socket.send(reply).await?;
            return Ok(());
        }

        let frame = HootFrame::from_frames(&frames)?;

        debug!(
            "[query] HOOT01 {:?} request_id={}",
            frame.command, frame.request_id
        );

        // Parse and dispatch query
        let result_payload = match frame.read_capnp() {
            Ok(reader) => match reader.get_root::<envelope_capnp::envelope::Reader>() {
                Ok(envelope_reader) => {
                    match capnp_envelope_to_payload(envelope_reader) {
                        Ok(payload) => self.dispatch_payload(handler, payload).await,
                        Err(e) => Payload::Error {
                            code: "capnp_parse_error".to_string(),
                            message: e.to_string(),
                            details: None,
                        },
                    }
                }
                Err(e) => Payload::Error {
                    code: "capnp_read_error".to_string(),
                    message: e.to_string(),
                    details: None,
                },
            },
            Err(e) => Payload::Error {
                code: "frame_parse_error".to_string(),
                message: e.to_string(),
                details: None,
            },
        };

        // Send response
        let response_msg = payload_to_capnp_envelope(frame.request_id, &result_payload)?;
        let bytes = capnp::serialize::write_message_to_words(&response_msg);
        let response_frame = HootFrame {
            command: Command::Reply,
            content_type: ContentType::CapnProto,
            request_id: frame.request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: bytes.into(),
        };
        let reply = frames_to_zmq_message(&response_frame.to_frames());
        socket.send(reply).await?;

        Ok(())
    }

    /// Handle JSON ShellRequest messages (from GardenClient)
    async fn handle_json_shell_request(
        &self,
        handler: &Arc<GardenDaemon>,
        body: &[u8],
        request_id: Uuid,
    ) -> HootFrame {
        // Parse Message<ShellRequest>
        let msg_result: Result<Message<ShellRequest>, _> = serde_json::from_slice(body);

        let (reply_content, parent_header) = match msg_result {
            Ok(msg) => {
                debug!("Received ShellRequest: {:?}", msg.content);
                let reply = handler.handle_shell(msg.content);
                (reply, Some(msg.header))
            }
            Err(e) => {
                error!("Failed to parse ShellRequest: {}", e);
                (
                    ShellReply::Error {
                        error: format!("JSON parse error: {}", e),
                        traceback: None,
                    },
                    None,
                )
            }
        };

        // Build reply message
        let reply_msg = Message {
            header: crate::ipc::MessageHeader::new(Uuid::nil(), "shell_reply"),
            parent_header,
            metadata: std::collections::HashMap::new(),
            content: reply_content,
        };

        let reply_json = serde_json::to_vec(&reply_msg).unwrap_or_else(|e| {
            error!("Failed to serialize reply: {}", e);
            b"{}".to_vec()
        });

        HootFrame {
            command: Command::Reply,
            content_type: ContentType::Json,
            request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: Bytes::from(reply_json),
        }
    }

    /// Dispatch a Payload to the appropriate handler
    async fn dispatch_payload(&self, handler: &Arc<GardenDaemon>, payload: Payload) -> Payload {
        match payload {
            Payload::StreamStart { uri, definition, chunk_path } => {
                // Convert hooteproto::StreamDefinition to garden::StreamDefinition (IPC type)
                let ipc_definition = convert_to_ipc_stream_definition(definition);

                match handler.handle_stream_start(uri.clone(), ipc_definition, chunk_path) {
                    Ok(()) => Payload::Success {
                        result: serde_json::json!({"status": "stream_started", "uri": uri}),
                    },
                    Err(e) => Payload::Error {
                        code: "stream_start_failed".to_string(),
                        message: e,
                        details: None,
                    },
                }
            }

            Payload::StreamStop { uri } => {
                match handler.handle_stream_stop(uri.clone()) {
                    Ok(()) => Payload::Success {
                        result: serde_json::json!({"status": "stream_stopped", "uri": uri}),
                    },
                    Err(e) => Payload::Error {
                        code: "stream_stop_failed".to_string(),
                        message: e,
                        details: None,
                    },
                }
            }

            Payload::StreamSwitchChunk { uri, new_chunk_path } => {
                match handler.handle_stream_switch_chunk(uri.clone(), new_chunk_path) {
                    Ok(()) => Payload::Success {
                        result: serde_json::json!({"status": "chunk_switched", "uri": uri}),
                    },
                    Err(e) => Payload::Error {
                        code: "stream_switch_chunk_failed".to_string(),
                        message: e,
                        details: None,
                    },
                }
            }

            // For other payloads, return not implemented
            _ => Payload::Error {
                code: "not_implemented".to_string(),
                message: format!("Payload type not implemented in chaosgarden: {:?}", payload),
                details: None,
            },
        }
    }
}

/// Convert hooteproto::StreamDefinition to garden::StreamDefinition (IPC type)
fn convert_to_ipc_stream_definition(proto_def: hooteproto::StreamDefinition) -> crate::ipc::StreamDefinition {
    crate::ipc::StreamDefinition {
        device_identity: proto_def.device_identity,
        format: convert_to_ipc_stream_format(proto_def.format),
        chunk_size_bytes: proto_def.chunk_size_bytes,
    }
}

/// Convert hooteproto::StreamFormat to garden::StreamFormat (IPC type)
fn convert_to_ipc_stream_format(proto_format: hooteproto::StreamFormat) -> crate::ipc::StreamFormat {
    match proto_format {
        hooteproto::StreamFormat::Audio { sample_rate, channels, sample_format } => {
            crate::ipc::StreamFormat::Audio {
                sample_rate,
                channels: channels as u16,
                sample_format: convert_to_ipc_sample_format(sample_format),
            }
        }
        hooteproto::StreamFormat::Midi => crate::ipc::StreamFormat::Midi,
    }
}

/// Convert hooteproto::SampleFormat to garden::SampleFormat (IPC type)
fn convert_to_ipc_sample_format(proto_format: hooteproto::SampleFormat) -> crate::ipc::SampleFormat {
    match proto_format {
        hooteproto::SampleFormat::F32 => crate::ipc::SampleFormat::F32Le,
        hooteproto::SampleFormat::I16 => crate::ipc::SampleFormat::S16Le,
        hooteproto::SampleFormat::I24 => crate::ipc::SampleFormat::S24Le,
    }
}
