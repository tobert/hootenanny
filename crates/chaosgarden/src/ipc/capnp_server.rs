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

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope,
    request::ToolRequest,
    responses::{
        GardenRegionInfo, GardenRegionsResponse, GardenStatusResponse, ToolResponse,
        TransportState,
    },
    socket_config::create_and_bind,
    Command, ContentType, HootFrame, Payload, ResponseEnvelope, PROTOCOL_VERSION,
};
use rzmq::{Context, Msg, MsgFlags, Socket, SocketType};
use std::sync::Arc;
use tokio::select;
use tracing::{debug, error, info, warn};

use crate::daemon::GardenDaemon;
use uuid::Uuid;

/// Convert frames to Vec<Msg> for rzmq multipart
fn frames_to_msgs(frames: &[Bytes]) -> Vec<Msg> {
    frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect()
}

/// Convert Vec<Msg> to Vec<Bytes> for frame processing
fn msgs_to_frames(msgs: &[Msg]) -> Vec<Bytes> {
    msgs.iter()
        .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
        .collect()
}

/// Send multipart using individual send() calls with MORE flags.
async fn send_multipart_individually(socket: &Socket, msgs: Vec<Msg>) -> anyhow::Result<()> {
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
        let context = Context::new()
            .with_context(|| "Failed to create ZMQ context")?;

        // Bind all 5 sockets (Jupyter-inspired protocol)
        let control_socket =
            create_and_bind(&context, SocketType::Router, &self.endpoints.control, "control")
                .await?;
        let shell_socket =
            create_and_bind(&context, SocketType::Router, &self.endpoints.shell, "shell").await?;
        let _iopub_socket =
            create_and_bind(&context, SocketType::Pub, &self.endpoints.iopub, "iopub").await?;
        let heartbeat_socket =
            create_and_bind(&context, SocketType::Rep, &self.endpoints.heartbeat, "heartbeat")
                .await?;
        let query_socket =
            create_and_bind(&context, SocketType::Rep, &self.endpoints.query, "query").await?;

        info!("🎵 chaosgarden server ready (5 sockets bound)");

        // Main event loop - handle all sockets concurrently
        loop {
            select! {
                // Control socket - priority commands
                msg = control_socket.recv_multipart() => {
                    match msg {
                        Ok(msgs) => {
                            let frames = msgs_to_frames(&msgs);
                            if let Err(e) = self.handle_router_message(
                                &control_socket,
                                &handler,
                                frames,
                                "control"
                            ).await {
                                error!("Error handling control message: {}", e);
                            }
                        }
                        Err(e) => error!("Control socket recv error: {}", e),
                    }
                }

                // Shell socket - normal commands
                msg = shell_socket.recv_multipart() => {
                    match msg {
                        Ok(msgs) => {
                            let frames = msgs_to_frames(&msgs);
                            if let Err(e) = self.handle_router_message(
                                &shell_socket,
                                &handler,
                                frames,
                                "shell"
                            ).await {
                                error!("Error handling shell message: {}", e);
                            }
                        }
                        Err(e) => error!("Shell socket recv error: {}", e),
                    }
                }

                // Heartbeat socket - liveness detection
                msg = heartbeat_socket.recv_multipart() => {
                    match msg {
                        Ok(msgs) => {
                            let frames = msgs_to_frames(&msgs);
                            if let Err(e) = self.handle_heartbeat(&heartbeat_socket, frames).await {
                                error!("Error handling heartbeat: {}", e);
                            }
                        }
                        Err(e) => error!("Heartbeat socket recv error: {}", e),
                    }
                }

                // Query socket - Trustfall queries
                msg = query_socket.recv_multipart() => {
                    match msg {
                        Ok(msgs) => {
                            let frames = msgs_to_frames(&msgs);
                            if let Err(e) = self.handle_query(&query_socket, &handler, frames).await {
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
        socket: &Socket,
        handler: &Arc<GardenDaemon>,
        frames: Vec<Bytes>,
        channel: &str,
    ) -> Result<()> {
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
                let reply = frames_to_msgs(&reply_frames);
                send_multipart_individually(socket, reply).await?;
                debug!("[{}] Heartbeat response sent", channel);
            }

            Command::Request => {
                match frame.content_type {
                    ContentType::Json => {
                        // JSON request - parse as Jupyter Message<ShellRequest> for backward compatibility
                        debug!("[{}] Processing JSON request", channel);
                        let reply_content = match serde_json::from_slice::<hooteproto::garden::Message<hooteproto::garden::ShellRequest>>(&frame.body) {
                            Ok(msg) => {
                                // Call handle_shell and get ShellReply
                                handler.handle_shell(msg.content)
                            }
                            Err(e) => {
                                hooteproto::garden::ShellReply::Error {
                                    error: format!("Failed to parse JSON ShellRequest: {}", e),
                                    traceback: None,
                                }
                            }
                        };
                        // Wrap in Message envelope and serialize back to JSON
                        let reply_msg = hooteproto::garden::Message::new(
                            frame.request_id,
                            "shell_reply",
                            reply_content,
                        );
                        let reply_json = serde_json::to_vec(&reply_msg).unwrap_or_default();
                        let response_frame = HootFrame {
                            command: Command::Reply,
                            content_type: ContentType::Json,
                            request_id: frame.request_id,
                            service: "chaosgarden".to_string(),
                            traceparent: None,
                            body: reply_json.into(),
                        };
                        let reply_frames = response_frame.to_frames_with_identity(&identity);
                        let reply = frames_to_msgs(&reply_frames);
                        send_multipart_individually(socket, reply).await?;
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
                        let reply = frames_to_msgs(&reply_frames);
                        send_multipart_individually(socket, reply).await?;
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
    async fn handle_heartbeat(&self, socket: &Socket, frames: Vec<Bytes>) -> Result<()> {
        // Check for HOOT01 frame
        if frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            // Parse and respond with HOOT01 heartbeat
            match HootFrame::from_frames(&frames) {
                Ok(_frame) => {
                    let response = HootFrame::heartbeat("chaosgarden");
                    let reply_frames = response.to_frames();
                    let reply = frames_to_msgs(&reply_frames);
                    send_multipart_individually(socket, reply).await?;
                    debug!("💓 Heartbeat response sent");
                }
                Err(e) => {
                    warn!("Failed to parse heartbeat frame: {}", e);
                    // Echo back anyway for compatibility
                    let reply = frames_to_msgs(&frames);
                    send_multipart_individually(socket, reply).await?;
                }
            }
        } else {
            // Legacy heartbeat - just echo back
            let reply = frames_to_msgs(&frames);
            send_multipart_individually(socket, reply).await?;
        }

        Ok(())
    }

    /// Handle Trustfall query messages (REP socket)
    async fn handle_query(
        &self,
        socket: &Socket,
        handler: &Arc<GardenDaemon>,
        frames: Vec<Bytes>,
    ) -> Result<()> {
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
            let reply = frames_to_msgs(&response_frame.to_frames());
            send_multipart_individually(socket, reply).await?;
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
        let reply = frames_to_msgs(&response_frame.to_frames());
        send_multipart_individually(socket, reply).await?;

        Ok(())
    }

    /// Dispatch a Payload to the appropriate handler
    async fn dispatch_payload(&self, handler: &Arc<GardenDaemon>, payload: Payload) -> Payload {
        match payload {
            Payload::StreamStart { uri, definition, chunk_path } => {
                // Convert hooteproto::StreamDefinition to garden::StreamDefinition (IPC type)
                let ipc_definition = convert_to_ipc_stream_definition(definition);

                match handler.handle_stream_start(uri.clone(), ipc_definition, chunk_path) {
                    Ok(()) => Payload::TypedResponse(ResponseEnvelope::ack(format!(
                        "stream_started: {}",
                        uri
                    ))),
                    Err(e) => Payload::Error {
                        code: "stream_start_failed".to_string(),
                        message: e,
                        details: None,
                    },
                }
            }

            Payload::StreamStop { uri } => {
                match handler.handle_stream_stop(uri.clone()) {
                    Ok(()) => Payload::TypedResponse(ResponseEnvelope::ack(format!(
                        "stream_stopped: {}",
                        uri
                    ))),
                    Err(e) => Payload::Error {
                        code: "stream_stop_failed".to_string(),
                        message: e,
                        details: None,
                    },
                }
            }

            Payload::StreamSwitchChunk { uri, new_chunk_path } => {
                match handler.handle_stream_switch_chunk(uri.clone(), new_chunk_path) {
                    Ok(()) => Payload::TypedResponse(ResponseEnvelope::ack(format!(
                        "chunk_switched: {}",
                        uri
                    ))),
                    Err(e) => Payload::Error {
                        code: "stream_switch_chunk_failed".to_string(),
                        message: e,
                        details: None,
                    },
                }
            }

            // Handle ToolRequest variants for garden commands
            Payload::ToolRequest(req) => self.dispatch_tool_request(handler, req),

            // For other payloads, return not implemented
            _ => Payload::Error {
                code: "not_implemented".to_string(),
                message: format!("Payload type not implemented in chaosgarden: {:?}", payload),
                details: None,
            },
        }
    }

    /// Dispatch a ToolRequest to the appropriate handler
    ///
    /// Converts ToolRequest to ShellRequest, calls handle_shell, and converts back to Payload.
    fn dispatch_tool_request(&self, handler: &Arc<GardenDaemon>, req: ToolRequest) -> Payload {
        use hooteproto::garden::{ShellRequest, Beat as IpcBeat, Behavior as IpcBehavior};

        // Convert ToolRequest to ShellRequest
        let shell_req = match req {
            ToolRequest::GardenStatus => ShellRequest::GetTransportState,
            ToolRequest::GardenPlay => ShellRequest::Play,
            ToolRequest::GardenPause => ShellRequest::Pause,
            ToolRequest::GardenStop => ShellRequest::Stop,
            ToolRequest::GardenSeek(r) => ShellRequest::Seek { beat: IpcBeat(r.beat) },
            ToolRequest::GardenSetTempo(r) => ShellRequest::SetTempo { bpm: r.bpm },
            ToolRequest::GardenGetRegions(r) => ShellRequest::GetRegions {
                range: match (r.start, r.end) {
                    (Some(s), Some(e)) => Some((IpcBeat(s), IpcBeat(e))),
                    _ => None,
                },
            },
            ToolRequest::GardenCreateRegion(r) => {
                let behavior = match r.behavior_type.as_str() {
                    "latent" => IpcBehavior::Latent { job_id: r.content_id.clone() },
                    _ => IpcBehavior::PlayContent { artifact_id: r.content_id.clone() },
                };
                ShellRequest::CreateRegion {
                    position: IpcBeat(r.position),
                    duration: IpcBeat(r.duration),
                    behavior,
                }
            }
            ToolRequest::GardenDeleteRegion(r) => {
                match Uuid::parse_str(&r.region_id) {
                    Ok(id) => ShellRequest::DeleteRegion { region_id: id },
                    Err(e) => return Payload::Error {
                        code: "invalid_uuid".to_string(),
                        message: e.to_string(),
                        details: None,
                    },
                }
            }
            ToolRequest::GardenMoveRegion(r) => {
                match Uuid::parse_str(&r.region_id) {
                    Ok(id) => ShellRequest::MoveRegion { region_id: id, new_position: IpcBeat(r.new_position) },
                    Err(e) => return Payload::Error {
                        code: "invalid_uuid".to_string(),
                        message: e.to_string(),
                        details: None,
                    },
                }
            }
            ToolRequest::GardenEmergencyPause => ShellRequest::Pause,
            ToolRequest::GardenAttachAudio(r) => ShellRequest::AttachAudio {
                device_name: r.device_name,
                sample_rate: r.sample_rate,
                latency_frames: r.latency_frames,
            },
            ToolRequest::GardenDetachAudio => ShellRequest::DetachAudio,
            ToolRequest::GardenAudioStatus => ShellRequest::GetAudioStatus,
            ToolRequest::GardenAttachInput(r) => ShellRequest::AttachInput {
                device_name: r.device_name,
                sample_rate: r.sample_rate,
            },
            ToolRequest::GardenDetachInput => ShellRequest::DetachInput,
            ToolRequest::GardenInputStatus => ShellRequest::GetInputStatus,
            ToolRequest::GardenSetMonitor(r) => ShellRequest::SetMonitor { enabled: r.enabled, gain: r.gain },
            _ => return Payload::Error {
                code: "not_implemented".to_string(),
                message: format!("ToolRequest not implemented in chaosgarden: {:?}", req),
                details: None,
            },
        };

        // Call handle_shell and convert result
        let reply = handler.handle_shell(shell_req);
        shell_reply_to_payload(reply)
    }
}

/// Convert a ShellReply to a Payload
fn shell_reply_to_payload(reply: hooteproto::garden::ShellReply) -> Payload {
    use hooteproto::garden::ShellReply;

    match reply {
        ShellReply::Ok { result: _ } => {
            Payload::TypedResponse(ResponseEnvelope::ack("ok".to_string()))
        }
        ShellReply::TransportState { playing, position, tempo } => {
            let response = GardenStatusResponse {
                state: if playing { TransportState::Playing } else { TransportState::Stopped },
                position_beats: position.0,
                tempo_bpm: tempo,
                region_count: 0, // Would need separate query
            };
            Payload::TypedResponse(ResponseEnvelope::success(ToolResponse::GardenStatus(response)))
        }
        ShellReply::RegionCreated { region_id } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::GardenRegionCreated(hooteproto::responses::GardenRegionCreatedResponse {
                    region_id: region_id.to_string(),
                    position: 0.0, // Not available in ShellReply
                    duration: 0.0, // Not available in ShellReply
                })
            ))
        }
        ShellReply::Regions { regions } => {
            let converted: Vec<GardenRegionInfo> = regions
                .into_iter()
                .map(|r| GardenRegionInfo {
                    region_id: r.region_id.to_string(),
                    position: r.position.0,
                    duration: r.duration.0,
                    behavior_type: if r.is_latent { "latent" } else { "content" }.to_string(),
                    content_id: r.artifact_id.unwrap_or_default(),
                })
                .collect();
            let count = converted.len();
            Payload::TypedResponse(ResponseEnvelope::success(ToolResponse::GardenRegions(
                GardenRegionsResponse { regions: converted, count }
            )))
        }
        ShellReply::AudioStatus { attached, device_name, sample_rate, latency_frames, callbacks, samples_written, underruns, .. } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::GardenAudioStatus(hooteproto::responses::GardenAudioStatusResponse {
                    attached,
                    device_name,
                    sample_rate,
                    latency_frames,
                    callbacks,
                    samples_written,
                    underruns,
                })
            ))
        }
        ShellReply::InputStatus { attached, device_name, sample_rate, channels, monitor_enabled, monitor_gain, callbacks, samples_captured, overruns } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::GardenInputStatus(hooteproto::responses::GardenInputStatusResponse {
                    attached,
                    device_name,
                    sample_rate,
                    channels,
                    monitor_enabled,
                    monitor_gain,
                    callbacks,
                    samples_captured,
                    overruns,
                })
            ))
        }
        ShellReply::PendingApprovals { approvals: _ } => {
            Payload::TypedResponse(ResponseEnvelope::ack("pending_approvals".to_string()))
        }
        ShellReply::Error { error, traceback } => {
            Payload::Error {
                code: "shell_error".to_string(),
                message: error,
                details: traceback.map(|t| serde_json::json!({ "traceback": t })),
            }
        }
        // Catch-all for other ShellReply variants (NodeAdded, etc.)
        other => {
            Payload::TypedResponse(ResponseEnvelope::ack(format!("{:?}", other)))
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
