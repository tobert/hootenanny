//! Cap'n Proto-based ZMQ server for chaosgarden
//!
//! Replaces the old MessagePack wire format with HOOT01 frame protocol
//! and Cap'n Proto envelope messages.

use anyhow::{Context, Result};
use bytes::Bytes;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope,
    Command, ContentType, HootFrame, Payload, PROTOCOL_VERSION,
};
use std::sync::Arc;
use tracing::{debug, error, info};
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::daemon::GardenDaemon;

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
        // Bind shell socket (main command channel)
        let mut shell_socket = RouterSocket::new();
        shell_socket
            .bind(&self.endpoints.shell)
            .await
            .context("Failed to bind shell socket")?;

        info!("chaosgarden Cap'n Proto server listening on {}", self.endpoints.shell);

        loop {
            match shell_socket.recv().await {
                Ok(msg) => {
                    if let Err(e) = self.handle_shell_message(&mut shell_socket, &handler, msg).await {
                        error!("Error handling shell message: {}", e);
                    }
                }
                Err(e) => {
                    error!("Error receiving message: {}", e);
                }
            }
        }
    }

    async fn handle_shell_message(
        &self,
        socket: &mut RouterSocket,
        handler: &Arc<GardenDaemon>,
        msg: ZmqMessage,
    ) -> Result<()> {
        // Convert to Bytes frames
        let frames: Vec<Bytes> = msg.iter().map(|f| Bytes::copy_from_slice(f)).collect();

        // Only accept HOOT01 frames
        if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            error!("Received non-HOOT01 message, ignoring");
            return Ok(());
        }

        // Parse HOOT01 frame
        let (identity, frame) = HootFrame::from_frames_with_identity(&frames)?;

        debug!(
            "HOOT01 {:?} from service={} request_id={}",
            frame.command, frame.service, frame.request_id
        );

        match frame.command {
            Command::Heartbeat => {
                // Respond with heartbeat
                let response = HootFrame::heartbeat("chaosgarden");
                let reply_frames = response.to_frames_with_identity(&identity);
                let reply = frames_to_zmq_message(&reply_frames);
                socket.send(reply).await?;
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
                        self.dispatch_payload(handler, payload).await
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
                    service: "chaosgarden".to_string(),
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
