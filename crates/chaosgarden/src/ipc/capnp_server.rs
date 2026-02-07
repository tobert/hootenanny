//! Cap'n Proto-based ZMQ server for chaosgarden
//!
//! Implements the Jupyter-inspired 4-socket protocol with HOOT01 frames
//! and Cap'n Proto envelope messages.
//!
//! Socket types:
//! - control (ROUTER): Priority commands (shutdown, interrupt)
//! - shell (ROUTER): Normal commands (transport, streams)
//! - iopub (PUB): Event broadcasts (state changes, metrics)
//! - heartbeat (ROUTER): Liveness detection (ROUTER for DEALER clients)

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
        // Bind all 4 sockets using GardenListener
        let listener = GardenListener::from_config(&self.config)
            .with_context(|| "Failed to create garden listener")?;
        let sockets = listener.bind()
            .with_context(|| "Failed to bind garden sockets")?;

        info!("🎵 chaosgarden server ready (4 sockets bound)");

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
                let result_payload = match frame.content_type {
                    ContentType::Json => {
                        error!("[{}] JSON requests no longer supported - use Cap'n Proto", channel);
                        Payload::Error {
                            code: "json_not_supported".to_string(),
                            message: "JSON requests are no longer supported - use Cap'n Proto".to_string(),
                            details: None,
                        }
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

                        match payload_result {
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
                        }
                    }
                    other => {
                        warn!(
                            "[{}] Unsupported content type: {:?}, ignoring",
                            channel, other
                        );
                        Payload::Error {
                            code: "unsupported_content_type".to_string(),
                            message: format!("Unsupported content type: {:?}", other),
                            details: None,
                        }
                    }
                };

                // Convert result to Cap'n Proto envelope and send
                let response_msg =
                    payload_to_capnp_envelope(frame.request_id, &result_payload)?;

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
                    .with_context(|| format!("[{}] Failed to send response", channel))?;
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
                    _ => IpcBehavior::PlayContent { content_hash: r.content_id.clone() },
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
            ToolRequest::GardenGetAudioSnapshot(r) => ShellRequest::GetAudioSnapshot { frames: r.frames },

            // MIDI I/O (direct ALSA)
            ToolRequest::MidiListPorts => ShellRequest::ListMidiPorts,
            ToolRequest::MidiInputAttach(r) => ShellRequest::AttachMidiInput { port_pattern: r.port_pattern.clone() },
            ToolRequest::MidiInputDetach(r) => ShellRequest::DetachMidiInput { port_pattern: r.port_pattern.clone() },
            ToolRequest::MidiOutputAttach(r) => ShellRequest::AttachMidiOutput { port_pattern: r.port_pattern.clone() },
            ToolRequest::MidiOutputDetach(r) => ShellRequest::DetachMidiOutput { port_pattern: r.port_pattern.clone() },
            ToolRequest::MidiSend(r) => {
                let msg = match &r.message {
                    hooteproto::request::MidiMessageSpec::NoteOn { channel, pitch, velocity } => {
                        hooteproto::garden::MidiMessageSpec::NoteOn { channel: *channel, pitch: *pitch, velocity: *velocity }
                    }
                    hooteproto::request::MidiMessageSpec::NoteOff { channel, pitch } => {
                        hooteproto::garden::MidiMessageSpec::NoteOff { channel: *channel, pitch: *pitch }
                    }
                    hooteproto::request::MidiMessageSpec::ControlChange { channel, controller, value } => {
                        hooteproto::garden::MidiMessageSpec::ControlChange { channel: *channel, controller: *controller, value: *value }
                    }
                    hooteproto::request::MidiMessageSpec::ProgramChange { channel, program } => {
                        hooteproto::garden::MidiMessageSpec::ProgramChange { channel: *channel, program: *program }
                    }
                    hooteproto::request::MidiMessageSpec::PitchBend { channel, value } => {
                        hooteproto::garden::MidiMessageSpec::PitchBend { channel: *channel, value: *value }
                    }
                    hooteproto::request::MidiMessageSpec::Raw { bytes } => {
                        hooteproto::garden::MidiMessageSpec::Raw { bytes: bytes.clone() }
                    }
                    hooteproto::request::MidiMessageSpec::Start => {
                        hooteproto::garden::MidiMessageSpec::Start
                    }
                    hooteproto::request::MidiMessageSpec::Stop => {
                        hooteproto::garden::MidiMessageSpec::Stop
                    }
                    hooteproto::request::MidiMessageSpec::Continue => {
                        hooteproto::garden::MidiMessageSpec::Continue
                    }
                    hooteproto::request::MidiMessageSpec::TimingClock => {
                        hooteproto::garden::MidiMessageSpec::TimingClock
                    }
                };
                ShellRequest::SendMidi { port_pattern: r.port_pattern.clone(), message: msg }
            }
            ToolRequest::MidiStatus => ShellRequest::GetMidiStatus,
            ToolRequest::MidiPlay(r) => ShellRequest::PlayMidi {
                content_hash: r.artifact_id.clone(), // artifact_id contains content_hash from hootenanny
                port_pattern: r.port_pattern,
                start_beat: r.start_beat,
            },
            ToolRequest::MidiStop(r) => {
                match Uuid::parse_str(&r.region_id) {
                    Ok(id) => ShellRequest::StopMidi { region_id: id },
                    Err(e) => return Payload::Error {
                        code: "invalid_uuid".to_string(),
                        message: e.to_string(),
                        details: None,
                    },
                }
            }

            // RAVE streaming (realtime neural audio processing)
            ToolRequest::RaveStreamStart(r) => ShellRequest::RaveStreamStart {
                model: r.model,
                input_identity: r.input_identity,
                output_identity: r.output_identity,
                buffer_size: r.buffer_size,
            },
            ToolRequest::RaveStreamStop(r) => ShellRequest::RaveStreamStop {
                stream_id: r.stream_id,
            },
            ToolRequest::RaveStreamStatus(r) => ShellRequest::RaveStreamStatus {
                stream_id: r.stream_id,
            },

            // GardenQuery is handled by hootenanny, not chaosgarden
            ToolRequest::GardenQuery(_) => {
                return Payload::Error {
                    code: "not_supported".to_string(),
                    message: "Trustfall queries are now handled by hootenanny via GetSnapshot".to_string(),
                    details: None,
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
        ShellReply::TransportState { playing, position, tempo, region_count } => {
            let response = GardenStatusResponse {
                state: if playing { TransportState::Playing } else { TransportState::Stopped },
                position_beats: position.0,
                tempo_bpm: tempo,
                region_count,
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
        ShellReply::AudioStatus { attached, device_name, sample_rate, latency_frames, callbacks, samples_written, underruns, monitor_reads, monitor_samples } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::GardenAudioStatus(hooteproto::responses::GardenAudioStatusResponse {
                    attached,
                    device_name,
                    sample_rate,
                    latency_frames,
                    callbacks,
                    samples_written,
                    underruns,
                    monitor_reads,
                    monitor_samples,
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
        ShellReply::MonitorStatus { enabled, gain } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::GardenMonitorStatus(hooteproto::responses::GardenMonitorStatusResponse {
                    enabled,
                    gain: gain as f64,
                })
            ))
        }
        ShellReply::AudioSnapshot { sample_rate, channels, format, samples } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::GardenAudioSnapshot(hooteproto::responses::GardenAudioSnapshotResponse {
                    sample_rate,
                    channels,
                    format,
                    samples,
                })
            ))
        }
        ShellReply::PendingApprovals { approvals: _ } => {
            Payload::TypedResponse(ResponseEnvelope::ack("pending_approvals".to_string()))
        }
        // MIDI I/O responses
        ShellReply::MidiPorts { inputs, outputs } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::MidiPorts(hooteproto::responses::MidiPortsResponse {
                    inputs: inputs.into_iter().map(|p| hooteproto::responses::MidiPortInfo {
                        index: p.index,
                        name: p.name,
                    }).collect(),
                    outputs: outputs.into_iter().map(|p| hooteproto::responses::MidiPortInfo {
                        index: p.index,
                        name: p.name,
                    }).collect(),
                })
            ))
        }
        ShellReply::MidiInputAttached { port_name } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::MidiAttached(hooteproto::responses::MidiAttachedResponse { port_name })
            ))
        }
        ShellReply::MidiOutputAttached { port_name } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::MidiAttached(hooteproto::responses::MidiAttachedResponse { port_name })
            ))
        }
        ShellReply::MidiStatus { inputs, outputs } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::MidiStatus(hooteproto::responses::MidiStatusResponse {
                    inputs: inputs.into_iter().map(|c| hooteproto::responses::MidiConnectionInfo {
                        port_name: c.port_name,
                        messages: c.messages,
                    }).collect(),
                    outputs: outputs.into_iter().map(|c| hooteproto::responses::MidiConnectionInfo {
                        port_name: c.port_name,
                        messages: c.messages,
                    }).collect(),
                })
            ))
        }
        ShellReply::MidiPlayStarted { region_id, duration_beats, event_count } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::MidiPlayStarted(hooteproto::responses::MidiPlayStartedResponse {
                    region_id: region_id.to_string(),
                    duration_beats,
                    event_count,
                })
            ))
        }
        ShellReply::MidiPlayStopped { region_id } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::MidiPlayStopped(hooteproto::responses::MidiPlayStoppedResponse {
                    region_id: region_id.to_string(),
                })
            ))
        }
        ShellReply::Error { error, traceback } => {
            Payload::Error {
                code: "shell_error".to_string(),
                message: error,
                details: traceback.map(|t| serde_json::json!({ "traceback": t })),
            }
        }
        // RAVE streaming responses
        ShellReply::RaveStreamStarted { stream_id, model, input_identity, output_identity, latency_ms } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::RaveStreamStarted(hooteproto::responses::RaveStreamStartedResponse {
                    stream_id,
                    model,
                    input_identity,
                    output_identity,
                    latency_ms,
                })
            ))
        }
        ShellReply::RaveStreamStopped { stream_id, duration_seconds } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::RaveStreamStopped(hooteproto::responses::RaveStreamStoppedResponse {
                    stream_id,
                    duration_seconds,
                })
            ))
        }
        ShellReply::RaveStreamStatus { stream_id, running, model, input_identity, output_identity, frames_processed, latency_ms, .. } => {
            Payload::TypedResponse(ResponseEnvelope::success(
                ToolResponse::RaveStreamStatus(hooteproto::responses::RaveStreamStatusResponse {
                    stream_id,
                    running,
                    model,
                    input_identity,
                    output_identity,
                    frames_processed,
                    latency_ms,
                })
            ))
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
