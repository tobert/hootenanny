//! Cap'n Proto-based ZMQ server for chaosgarden
//!
//! Implements the Jupyter-inspired 5-socket protocol with HOOT01 frames
//! and Cap'n Proto envelope messages.
//!
//! Socket types:
//! - control (ROUTER): Priority commands (shutdown, interrupt)
//! - shell (ROUTER): Normal commands (transport, streams)
//! - iopub (PUB): Event broadcasts (state changes, metrics)
//! - heartbeat (ROUTER): Liveness detection (ROUTER for DEALER clients)
//! - query (ROUTER): Trustfall queries (ROUTER for DEALER clients)

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope,
    garden_listener::{GardenListener, SplitRouter},
    request::ToolRequest,
    responses::{
        GardenRegionInfo, GardenRegionsResponse, GardenStatusResponse, ToolResponse,
        TransportState,
    },
    socket_config::Multipart,
    Command, ContentType, HootFrame, Payload, ResponseEnvelope, PROTOCOL_VERSION,
};
use std::sync::Arc;
use tokio::select;
use tracing::{debug, error, info, warn};

use crate::daemon::GardenDaemon;
use uuid::Uuid;

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

/// ZMQ server using Cap'n Proto for chaosgarden
pub struct CapnpGardenServer {
    config: hooteconf::HootConfig,
}

impl CapnpGardenServer {
    pub fn new(config: hooteconf::HootConfig) -> Self {
        Self { config }
    }

    /// Run the server with the garden daemon handler
    pub async fn run(self, handler: Arc<GardenDaemon>) -> Result<()> {
        // Bind all 5 sockets using GardenListener
        let listener = GardenListener::from_config(&self.config)
            .with_context(|| "Failed to create garden listener")?;
        let sockets = listener.bind()
            .with_context(|| "Failed to bind garden sockets")?;

        info!("🎵 chaosgarden server ready (5 sockets bound)");

        // Main event loop - handle all sockets concurrently
        loop {
            select! {
                // Control socket - priority commands
                msg = async {
                    sockets.control.rx.lock().await.next().await
                } => {
                    match msg {
                        Some(Ok(mp)) => {
                            let frames = multipart_to_frames(mp);
                            if let Err(e) = self.handle_router_message(
                                &sockets.control,
                                &handler,
                                frames,
                                "control"
                            ).await {
                                error!("Error handling control message: {}", e);
                            }
                        }
                        Some(Err(e)) => error!("Control socket recv error: {}", e),
                        None => {
                            warn!("Control socket stream ended");
                            break;
                        }
                    }
                }

                // Shell socket - normal commands
                msg = async {
                    sockets.shell.rx.lock().await.next().await
                } => {
                    match msg {
                        Some(Ok(mp)) => {
                            let frames = multipart_to_frames(mp);
                            if let Err(e) = self.handle_router_message(
                                &sockets.shell,
                                &handler,
                                frames,
                                "shell"
                            ).await {
                                error!("Error handling shell message: {}", e);
                            }
                        }
                        Some(Err(e)) => error!("Shell socket recv error: {}", e),
                        None => {
                            warn!("Shell socket stream ended");
                            break;
                        }
                    }
                }

                // Heartbeat socket - liveness detection
                msg = async {
                    sockets.heartbeat.rx.lock().await.next().await
                } => {
                    match msg {
                        Some(Ok(mp)) => {
                            let frames = multipart_to_frames(mp);
                            if let Err(e) = self.handle_heartbeat(&sockets.heartbeat, frames).await {
                                error!("Error handling heartbeat: {}", e);
                            }
                        }
                        Some(Err(e)) => error!("Heartbeat socket recv error: {}", e),
                        None => {
                            warn!("Heartbeat socket stream ended");
                            break;
                        }
                    }
                }

                // Query socket - Trustfall queries
                msg = async {
                    sockets.query.rx.lock().await.next().await
                } => {
                    match msg {
                        Some(Ok(mp)) => {
                            let frames = multipart_to_frames(mp);
                            if let Err(e) = self.handle_query(&sockets.query, &handler, frames).await {
                                error!("Error handling query: {}", e);
                            }
                        }
                        Some(Err(e)) => error!("Query socket recv error: {}", e),
                        None => {
                            warn!("Query socket stream ended");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle messages on ROUTER sockets (control/shell)
    async fn handle_router_message(
        &self,
        socket: &SplitRouter,
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
                let reply = frames_to_multipart(&reply_frames);
                socket.tx.lock().await.send(reply).await
                    .with_context(|| format!("[{}] Failed to send heartbeat response", channel))?;
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
                        let reply = frames_to_multipart(&reply_frames);
                        socket.tx.lock().await.send(reply).await
                            .with_context(|| format!("[{}] Failed to send JSON response", channel))?;
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
                        let reply = frames_to_multipart(&reply_frames);
                        socket.tx.lock().await.send(reply).await
                            .with_context(|| format!("[{}] Failed to send capnp response", channel))?;
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

    /// Handle heartbeat messages (ROUTER socket - extract identity, echo response)
    async fn handle_heartbeat(&self, socket: &SplitRouter, frames: Vec<Bytes>) -> Result<()> {
        // Check for HOOT01 frame
        if frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            // Parse with identity (ROUTER socket prepends identity frame)
            match HootFrame::from_frames_with_identity(&frames) {
                Ok((identity, _frame)) => {
                    let response = HootFrame::heartbeat("chaosgarden");
                    let reply_frames = response.to_frames_with_identity(&identity);
                    let reply = frames_to_multipart(&reply_frames);
                    socket.tx.lock().await.send(reply).await
                        .with_context(|| "Failed to send heartbeat response")?;
                    debug!("💓 Heartbeat response sent");
                }
                Err(e) => {
                    warn!("Failed to parse heartbeat frame: {}", e);
                    // Echo back anyway for compatibility (with identity preserved)
                    let reply = frames_to_multipart(&frames);
                    socket.tx.lock().await.send(reply).await
                        .with_context(|| "Failed to echo heartbeat")?;
                }
            }
        } else {
            // Legacy heartbeat - just echo back (identity should be first frame)
            let reply = frames_to_multipart(&frames);
            socket.tx.lock().await.send(reply).await
                .with_context(|| "Failed to echo legacy heartbeat")?;
        }

        Ok(())
    }

    /// Handle Trustfall query messages (ROUTER socket)
    async fn handle_query(
        &self,
        socket: &SplitRouter,
        handler: &Arc<GardenDaemon>,
        frames: Vec<Bytes>,
    ) -> Result<()> {
        // Extract identity frames (first frame(s) before HOOT01 from ROUTER socket)
        // We'll get the proper identity from from_frames_with_identity, but need a fallback
        let fallback_identity: Vec<Bytes> = frames.first().cloned().map(|f| vec![f]).unwrap_or_default();

        // Check for HOOT01 frame
        if !frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            warn!("[query] Received non-HOOT01 message");
            // Send error response with identity
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
            let reply = frames_to_multipart(&response_frame.to_frames_with_identity(&fallback_identity));
            socket.tx.lock().await.send(reply).await
                .with_context(|| "[query] Failed to send error response")?;
            return Ok(());
        }

        // Parse with identity extraction
        let (identity, frame) = HootFrame::from_frames_with_identity(&frames)?;

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

        // Send response with identity
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
        let reply = frames_to_multipart(&response_frame.to_frames_with_identity(&identity));
        socket.tx.lock().await.send(reply).await
            .with_context(|| "[query] Failed to send response")?;

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

            // GardenQuery bypasses ShellRequest - directly executes Trustfall query
            ToolRequest::GardenQuery(r) => {
                use std::collections::HashMap;
                let vars: HashMap<String, serde_json::Value> = r.variables
                    .and_then(|v| v.as_object().cloned())
                    .map(|m| m.into_iter().collect())
                    .unwrap_or_default();
                let result = handler.execute_query(&r.query, &vars);
                return match result {
                    hooteproto::garden::QueryReply::Results { rows } => {
                        Payload::TypedResponse(ResponseEnvelope::success(
                            ToolResponse::GardenQueryResult(hooteproto::responses::GardenQueryResultResponse {
                                results: rows,
                                count: 0, // Could count rows but already consumed
                            })
                        ))
                    }
                    hooteproto::garden::QueryReply::Error { error } => {
                        Payload::Error {
                            code: "query_error".to_string(),
                            message: error,
                            details: None,
                        }
                    }
                };
            }

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
