//! Typed protocol conversions for hooteproto
//!
//! Provides conversions between:
//! - Payload variants ↔ Typed ToolRequest structs (for dispatch)
//! - ResponseEnvelope ↔ Payload (for ZMQ transport)
//! - Payload ↔ Cap'n Proto (for wire serialization)
//!
//! Note: Generic tool calls use Payload::ToolCall { name, args } which routes
//! to dispatch.rs by name. This avoids needing Payload variants for each tool.

use crate::{
    Payload, PollMode, SampleFormat, StreamDefinition, StreamFormat,
    TimelineEventType, WorkerType,
};

// Cap'n Proto imports for reading requests
use crate::{common_capnp, envelope_capnp, streams_capnp, tools_capnp};

// =============================================================================
// Typed Protocol Conversions (Protocol v2)
// =============================================================================

use crate::envelope::ResponseEnvelope;
use crate::request::*;
use crate::ToolError;

/// Convert a Payload to a ToolRequest for typed dispatch.
///
/// Returns Ok(Some(request)) for supported tools, Ok(None) for tools that
/// should use the legacy JSON path, and Err for invalid requests.
pub fn payload_to_request(payload: &Payload) -> Result<Option<ToolRequest>, ToolError> {
    match payload {
        // === ABC Notation (Sync) ===
        Payload::AbcParse { abc } => Ok(Some(ToolRequest::AbcParse(AbcParseRequest {
            abc: abc.clone(),
        }))),
        Payload::AbcValidate { abc } => Ok(Some(ToolRequest::AbcValidate(AbcValidateRequest {
            abc: abc.clone(),
        }))),
        Payload::AbcTranspose {
            abc,
            semitones,
            target_key,
        } => Ok(Some(ToolRequest::AbcTranspose(AbcTransposeRequest {
            abc: abc.clone(),
            semitones: *semitones,
            target_key: target_key.clone(),
        }))),

        // === SoundFont (Sync) ===
        Payload::SoundfontInspect {
            soundfont_hash,
            include_drum_map,
        } => Ok(Some(ToolRequest::SoundfontInspect(SoundfontInspectRequest {
            soundfont_hash: soundfont_hash.clone(),
            include_drum_map: *include_drum_map,
        }))),
        Payload::SoundfontPresetInspect {
            soundfont_hash,
            bank,
            program,
        } => Ok(Some(ToolRequest::SoundfontPresetInspect(
            SoundfontPresetInspectRequest {
                soundfont_hash: soundfont_hash.clone(),
                bank: *bank as u16,
                program: *program as u16,
            },
        ))),

        // === Garden (Sync status, FireAndForget controls) ===
        Payload::GardenStatus => Ok(Some(ToolRequest::GardenStatus)),
        Payload::GardenPlay => Ok(Some(ToolRequest::GardenPlay)),
        Payload::GardenPause => Ok(Some(ToolRequest::GardenPause)),
        Payload::GardenStop => Ok(Some(ToolRequest::GardenStop)),
        Payload::GardenSeek { beat } => Ok(Some(ToolRequest::GardenSeek(GardenSeekRequest {
            beat: *beat,
        }))),
        Payload::GardenSetTempo { bpm } => {
            Ok(Some(ToolRequest::GardenSetTempo(GardenSetTempoRequest {
                bpm: *bpm,
            })))
        }
        Payload::GardenGetRegions { start, end } => Ok(Some(ToolRequest::GardenGetRegions(
            GardenGetRegionsRequest {
                start: *start,
                end: *end,
            },
        ))),
        Payload::GardenCreateRegion {
            position,
            duration,
            behavior_type,
            content_id,
        } => Ok(Some(ToolRequest::GardenCreateRegion(
            GardenCreateRegionRequest {
                position: *position,
                duration: *duration,
                behavior_type: behavior_type.clone(),
                content_id: content_id.clone(),
            },
        ))),
        Payload::GardenDeleteRegion { region_id } => Ok(Some(ToolRequest::GardenDeleteRegion(
            GardenDeleteRegionRequest {
                region_id: region_id.clone(),
            },
        ))),
        Payload::GardenMoveRegion {
            region_id,
            new_position,
        } => Ok(Some(ToolRequest::GardenMoveRegion(GardenMoveRegionRequest {
            region_id: region_id.clone(),
            new_position: *new_position,
        }))),
        Payload::GardenEmergencyPause => Ok(Some(ToolRequest::GardenEmergencyPause)),

        // === Jobs (Sync) ===
        Payload::JobStatus { job_id } => Ok(Some(ToolRequest::JobStatus(JobStatusRequest {
            job_id: job_id.clone(),
        }))),
        Payload::JobList { status } => Ok(Some(ToolRequest::JobList(JobListRequest {
            status: status.clone(),
        }))),

        // === Config (Sync) ===
        Payload::ConfigGet { section, key } => Ok(Some(ToolRequest::ConfigGet(ConfigGetRequest {
            section: section.clone(),
            key: key.clone(),
        }))),

        // === Admin (Sync) ===
        Payload::Ping => Ok(Some(ToolRequest::Ping)),
        Payload::ListTools => Ok(Some(ToolRequest::ListTools)),

        // === Tools not yet converted - use legacy path ===
        _ => Ok(None),
    }
}

/// Convert a ResponseEnvelope back to Payload for ZMQ transport.
pub fn envelope_to_payload(envelope: ResponseEnvelope) -> Payload {
    match envelope {
        ResponseEnvelope::Success { response } => {
            // Convert typed response to JSON for legacy Payload::Success
            let result = response.to_json();
            Payload::Success { result }
        }
        ResponseEnvelope::JobStarted { job_id, tool, .. } => Payload::Success {
            result: serde_json::json!({
                "job_id": job_id,
                "tool": tool,
                "status": "started",
            }),
        },
        ResponseEnvelope::Ack { message } => Payload::Success {
            result: serde_json::json!({
                "status": "ok",
                "message": message,
            }),
        },
        ResponseEnvelope::Error(err) => Payload::Error {
            code: err.code().to_string(),
            message: err.message().to_string(),
            details: None,
        },
    }
}

/// Convert a Cap'n Proto Envelope reader to Payload
///
/// This enables the server to read Cap'n Proto requests from Python/Lua clients
/// and convert them to the internal Payload representation for dispatch.
pub fn capnp_envelope_to_payload(
    reader: envelope_capnp::envelope::Reader,
) -> capnp::Result<Payload> {
    let payload_reader = reader.get_payload()?;

    // Check which payload variant is set
    match payload_reader.which()? {
        // === Worker Management ===
        envelope_capnp::payload::Ping(()) => Ok(Payload::Ping),

        envelope_capnp::payload::Shutdown(shutdown) => {
            let reason = shutdown?.get_reason()?.to_str()?.to_string();
            Ok(Payload::Shutdown { reason })
        }

        // === Tool Requests ===
        envelope_capnp::payload::ToolRequest(tool_req) => {
            let tool_req = tool_req?;
            capnp_tool_request_to_payload(tool_req)
        }

        // === Garden/Timeline ===
        envelope_capnp::payload::GardenStatus(()) => Ok(Payload::GardenStatus),
        envelope_capnp::payload::GardenPlay(()) => Ok(Payload::GardenPlay),
        envelope_capnp::payload::GardenPause(()) => Ok(Payload::GardenPause),
        envelope_capnp::payload::GardenStop(()) => Ok(Payload::GardenStop),

        envelope_capnp::payload::GardenSeek(seek) => {
            let seek = seek?;
            Ok(Payload::GardenSeek {
                beat: seek.get_beat(),
            })
        }

        envelope_capnp::payload::GardenSetTempo(tempo) => {
            let tempo = tempo?;
            Ok(Payload::GardenSetTempo {
                bpm: tempo.get_bpm(),
            })
        }

        envelope_capnp::payload::GardenQuery(query) => {
            let query = query?;
            let query_str = query.get_query()?.to_str()?.to_string();
            let variables_str = query.get_variables()?.to_str()?;
            let variables = if variables_str.is_empty() {
                None
            } else {
                serde_json::from_str(variables_str).ok()
            };

            Ok(Payload::GardenQuery {
                query: query_str,
                variables,
            })
        }

        envelope_capnp::payload::GardenEmergencyPause(()) => Ok(Payload::GardenEmergencyPause),

        envelope_capnp::payload::GardenCreateRegion(region) => {
            let region = region?;
            let behavior_type = region.get_behavior_type()?.to_str()?.to_string();
            let content_id = region.get_content_id()?.to_str()?.to_string();

            Ok(Payload::GardenCreateRegion {
                position: region.get_position(),
                duration: region.get_duration(),
                behavior_type,
                content_id,
            })
        }

        envelope_capnp::payload::GardenDeleteRegion(region) => {
            let region = region?;
            Ok(Payload::GardenDeleteRegion {
                region_id: region.get_region_id()?.to_str()?.to_string(),
            })
        }

        envelope_capnp::payload::GardenMoveRegion(region) => {
            let region = region?;
            Ok(Payload::GardenMoveRegion {
                region_id: region.get_region_id()?.to_str()?.to_string(),
                new_position: region.get_new_position(),
            })
        }

        envelope_capnp::payload::GardenGetRegions(regions) => {
            let regions = regions?;
            let start = regions.get_start();
            let end = regions.get_end();

            Ok(Payload::GardenGetRegions {
                start: if start == 0.0 { None } else { Some(start) },
                end: if end == 0.0 { None } else { Some(end) },
            })
        }

        // === Transport ===
        envelope_capnp::payload::TransportPlay(()) => Ok(Payload::TransportPlay),
        envelope_capnp::payload::TransportStop(()) => Ok(Payload::TransportStop),
        envelope_capnp::payload::TransportStatus(()) => Ok(Payload::TransportStatus),

        envelope_capnp::payload::TransportSeek(seek) => {
            let seek = seek?;
            Ok(Payload::TransportSeek {
                position_beats: seek.get_position_beats(),
            })
        }

        // === Timeline ===
        envelope_capnp::payload::TimelineQuery(query) => {
            let query = query?;
            Ok(Payload::TimelineQuery {
                from_beats: Some(query.get_from_beats()),
                to_beats: Some(query.get_to_beats()),
            })
        }

        envelope_capnp::payload::TimelineAddMarker(marker) => {
            let marker = marker?;
            let metadata_str = marker.get_metadata()?.to_str()?;
            let metadata = serde_json::from_str(metadata_str).unwrap_or_default();

            Ok(Payload::TimelineAddMarker {
                position_beats: marker.get_position_beats(),
                marker_type: marker.get_marker_type()?.to_str()?.to_string(),
                metadata,
            })
        }

        // === Responses (shouldn't receive these, but handle gracefully) ===
        envelope_capnp::payload::Success(success) => {
            let success = success?;
            let result_str = success.get_result()?.to_str()?;
            let result = serde_json::from_str(result_str).unwrap_or_default();
            Ok(Payload::Success { result })
        }

        envelope_capnp::payload::Error(error) => {
            let error = error?;
            Ok(Payload::Error {
                code: error.get_code()?.to_str()?.to_string(),
                message: error.get_message()?.to_str()?.to_string(),
                details: None,
            })
        }

        envelope_capnp::payload::ToolList(tool_list) => {
            let tool_list = tool_list?;
            let tools_reader = tool_list.get_tools()?;
            let mut tools = Vec::new();

            for i in 0..tools_reader.len() {
                let tool = tools_reader.get(i);
                tools.push(crate::ToolInfo {
                    name: tool.get_name()?.to_str()?.to_string(),
                    description: tool.get_description()?.to_str()?.to_string(),
                    input_schema: serde_json::from_str(tool.get_input_schema()?.to_str()?).unwrap_or_default(),
                });
            }

            Ok(Payload::ToolList { tools })
        }

        // === Stream Capture ===
        envelope_capnp::payload::StreamStart(stream) => {
            let stream = stream?;
            let def = stream.get_definition()?;
            let format = def.get_format()?;

            let stream_format = match format.which()? {
                crate::streams_capnp::stream_format::Audio(audio) => {
                    let audio = audio?;
                    let sample_format_enum = audio.get_sample_format()?;
                    let sample_format = match sample_format_enum {
                        crate::streams_capnp::SampleFormat::F32 => crate::SampleFormat::F32,
                        crate::streams_capnp::SampleFormat::I16 => crate::SampleFormat::I16,
                        crate::streams_capnp::SampleFormat::I24 => crate::SampleFormat::I24,
                    };

                    crate::StreamFormat::Audio {
                        sample_rate: audio.get_sample_rate(),
                        channels: audio.get_channels(),
                        sample_format,
                    }
                }
                crate::streams_capnp::stream_format::Midi(()) => {
                    crate::StreamFormat::Midi
                }
            };

            Ok(Payload::StreamStart {
                uri: stream.get_uri()?.to_str()?.to_string(),
                definition: crate::StreamDefinition {
                    uri: def.get_uri()?.to_str()?.to_string(),
                    device_identity: def.get_device_identity()?.to_str()?.to_string(),
                    format: stream_format,
                    chunk_size_bytes: def.get_chunk_size_bytes(),
                },
                chunk_path: stream.get_chunk_path()?.to_str()?.to_string(),
            })
        }

        envelope_capnp::payload::StreamSwitchChunk(stream) => {
            let stream = stream?;
            Ok(Payload::StreamSwitchChunk {
                uri: stream.get_uri()?.to_str()?.to_string(),
                new_chunk_path: stream.get_new_chunk_path()?.to_str()?.to_string(),
            })
        }

        envelope_capnp::payload::StreamStop(stream) => {
            let stream = stream?;
            Ok(Payload::StreamStop {
                uri: stream.get_uri()?.to_str()?.to_string(),
            })
        }

        // Generic tool call - name + JSON args
        envelope_capnp::payload::ToolCall(tool_call) => {
            let tool_call = tool_call?;
            let name = tool_call.get_name()?.to_str()?.to_string();
            let args_str = tool_call.get_args()?.to_str()?;
            let args = if args_str.is_empty() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(args_str).unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()))
            };
            Ok(Payload::ToolCall { name, args })
        }

        envelope_capnp::payload::Pong(pong_reader) => {
            let pong = pong_reader?;
            let worker_id_reader = pong.get_worker_id()?;
            let low = worker_id_reader.get_low();
            let high = worker_id_reader.get_high();
            let mut bytes = [0u8; 16];
            bytes[0..8].copy_from_slice(&low.to_le_bytes());
            bytes[8..16].copy_from_slice(&high.to_le_bytes());
            let worker_id = uuid::Uuid::from_bytes(bytes);
            let uptime_secs = pong.get_uptime_secs();
            Ok(Payload::Pong { worker_id, uptime_secs })
        }

        // Variants not yet implemented
        envelope_capnp::payload::Register(_) |
        envelope_capnp::payload::TimelineEvent(_) => {
            Err(capnp::Error::failed("Payload variant not yet implemented for capnp conversion".to_string()))
        }
    }
}

/// Convert a Cap'n Proto ToolRequest to Payload
fn capnp_tool_request_to_payload(
    reader: tools_capnp::tool_request::Reader,
) -> capnp::Result<Payload> {
    match reader.which()? {
        // === CAS Tools ===
        tools_capnp::tool_request::CasStore(cas) => {
            let cas = cas?;
            Ok(Payload::CasStore {
                data: cas.get_data()?.to_vec(),
                mime_type: cas.get_mime_type()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::CasInspect(cas) => {
            let cas = cas?;
            Ok(Payload::CasInspect {
                hash: cas.get_hash()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::CasGet(cas) => {
            let cas = cas?;
            Ok(Payload::CasGet {
                hash: cas.get_hash()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::CasUploadFile(cas) => {
            let cas = cas?;
            Ok(Payload::CasUploadFile {
                file_path: cas.get_file_path()?.to_str()?.to_string(),
                mime_type: cas.get_mime_type()?.to_str()?.to_string(),
            })
        }

        // === Orpheus Tools ===
        tools_capnp::tool_request::OrpheusGenerate(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;

            Ok(Payload::OrpheusGenerate {
                model: Some(orpheus.get_model()?.to_str()?.to_string()),
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                num_variations: Some(orpheus.get_num_variations()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusGenerateSeeded(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;

            Ok(Payload::OrpheusGenerateSeeded {
                seed_hash: orpheus.get_seed_hash()?.to_str()?.to_string(),
                model: Some(orpheus.get_model()?.to_str()?.to_string()),
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                num_variations: Some(orpheus.get_num_variations()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusContinue(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;

            Ok(Payload::OrpheusContinue {
                input_hash: orpheus.get_input_hash()?.to_str()?.to_string(),
                model: Some(orpheus.get_model()?.to_str()?.to_string()),
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                num_variations: Some(orpheus.get_num_variations()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusBridge(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;
            let section_b = orpheus.get_section_b_hash()?.to_str()?;

            Ok(Payload::OrpheusBridge {
                section_a_hash: orpheus.get_section_a_hash()?.to_str()?.to_string(),
                section_b_hash: if section_b.is_empty() { None } else { Some(section_b.to_string()) },
                model: Some(orpheus.get_model()?.to_str()?.to_string()),
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusLoops(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;
            let seed_hash = orpheus.get_seed_hash()?.to_str()?;

            Ok(Payload::OrpheusLoops {
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                num_variations: Some(orpheus.get_num_variations()),
                seed_hash: if seed_hash.is_empty() { None } else { Some(seed_hash.to_string()) },
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusClassify(orpheus) => {
            let orpheus = orpheus?;
            Ok(Payload::OrpheusClassify {
                midi_hash: orpheus.get_midi_hash()?.to_str()?.to_string(),
            })
        }

        // === ABC Notation Tools ===
        tools_capnp::tool_request::AbcParse(abc) => {
            let abc = abc?;
            Ok(Payload::AbcParse {
                abc: abc.get_abc()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::AbcToMidi(abc) => {
            let abc = abc?;
            let metadata = abc.get_metadata()?;

            Ok(Payload::AbcToMidi {
                abc: abc.get_abc()?.to_str()?.to_string(),
                tempo_override: Some(abc.get_tempo_override()),
                transpose: Some(abc.get_transpose()),
                velocity: Some(abc.get_velocity()),
                channel: Some(abc.get_channel()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::AbcValidate(abc) => {
            let abc = abc?;
            Ok(Payload::AbcValidate {
                abc: abc.get_abc()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::AbcTranspose(abc) => {
            let abc = abc?;
            let target_key = abc.get_target_key()?.to_str()?;

            Ok(Payload::AbcTranspose {
                abc: abc.get_abc()?.to_str()?.to_string(),
                semitones: Some(abc.get_semitones()),
                target_key: if target_key.is_empty() { None } else { Some(target_key.to_string()) },
            })
        }

        // === MIDI/Audio Tools ===
        tools_capnp::tool_request::ConvertMidiToWav(convert) => {
            let convert = convert?;
            let metadata = convert.get_metadata()?;

            Ok(Payload::ConvertMidiToWav {
                input_hash: convert.get_input_hash()?.to_str()?.to_string(),
                soundfont_hash: convert.get_soundfont_hash()?.to_str()?.to_string(),
                sample_rate: Some(convert.get_sample_rate()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::SoundfontInspect(sf) => {
            let sf = sf?;
            Ok(Payload::SoundfontInspect {
                soundfont_hash: sf.get_soundfont_hash()?.to_str()?.to_string(),
                include_drum_map: sf.get_include_drum_map(),
            })
        }

        tools_capnp::tool_request::SoundfontPresetInspect(sf) => {
            let sf = sf?;
            Ok(Payload::SoundfontPresetInspect {
                soundfont_hash: sf.get_soundfont_hash()?.to_str()?.to_string(),
                bank: sf.get_bank(),
                program: sf.get_program(),
            })
        }

        // === Analysis Tools ===
        tools_capnp::tool_request::BeatthisAnalyze(beat) => {
            let beat = beat?;
            let audio_hash = beat.get_audio_hash()?.to_str()?;
            let audio_path = beat.get_audio_path()?.to_str()?;

            Ok(Payload::BeatthisAnalyze {
                audio_hash: if audio_hash.is_empty() { None } else { Some(audio_hash.to_string()) },
                audio_path: if audio_path.is_empty() { None } else { Some(audio_path.to_string()) },
                include_frames: beat.get_include_frames(),
            })
        }

        tools_capnp::tool_request::ClapAnalyze(clap) => {
            let clap = clap?;
            let audio_b_hash = clap.get_audio_b_hash()?.to_str()?;
            let tasks_reader = clap.get_tasks()?;
            let text_reader = clap.get_text_candidates()?;

            let parent_id = clap.get_parent_id()?.to_str()?;
            let creator = clap.get_creator()?.to_str()?;

            Ok(Payload::ClapAnalyze {
                audio_hash: clap.get_audio_hash()?.to_str()?.to_string(),
                tasks: capnp_string_list(tasks_reader),
                audio_b_hash: if audio_b_hash.is_empty() { None } else { Some(audio_b_hash.to_string()) },
                text_candidates: capnp_string_list(text_reader),
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
                parent_id: if parent_id.is_empty() { None } else { Some(parent_id.to_string()) },
            })
        }

        // === Generation Tools ===
        tools_capnp::tool_request::MusicgenGenerate(mg) => {
            let mg = mg?;
            let metadata = mg.get_metadata()?;
            let prompt = mg.get_prompt()?.to_str()?;

            Ok(Payload::MusicgenGenerate {
                prompt: if prompt.is_empty() { None } else { Some(prompt.to_string()) },
                duration: Some(mg.get_duration()),
                temperature: Some(mg.get_temperature()),
                top_k: Some(mg.get_top_k()),
                top_p: Some(mg.get_top_p()),
                guidance_scale: Some(mg.get_guidance_scale()),
                do_sample: Some(mg.get_do_sample()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::YueGenerate(yue) => {
            let yue = yue?;
            let metadata = yue.get_metadata()?;
            let genre = yue.get_genre()?.to_str()?;

            Ok(Payload::YueGenerate {
                lyrics: yue.get_lyrics()?.to_str()?.to_string(),
                genre: if genre.is_empty() { None } else { Some(genre.to_string()) },
                max_new_tokens: Some(yue.get_max_new_tokens()),
                run_n_segments: Some(yue.get_run_n_segments()),
                seed: Some(yue.get_seed()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        // === Artifact Tools ===
        tools_capnp::tool_request::ArtifactUpload(artifact) => {
            let artifact = artifact?;
            let metadata = artifact.get_metadata()?;

            Ok(Payload::ArtifactUpload {
                file_path: artifact.get_file_path()?.to_str()?.to_string(),
                mime_type: artifact.get_mime_type()?.to_str()?.to_string(),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::ArtifactGet(artifact) => {
            let artifact = artifact?;
            Ok(Payload::ArtifactGet {
                id: artifact.get_id()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::ArtifactList(artifact) => {
            let artifact = artifact?;
            let tag = artifact.get_tag()?.to_str()?;
            let creator = artifact.get_creator()?.to_str()?;

            Ok(Payload::ArtifactList {
                tag: if tag.is_empty() { None } else { Some(tag.to_string()) },
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
            })
        }

        tools_capnp::tool_request::ArtifactCreate(artifact) => {
            let artifact = artifact?;
            let metadata_str = artifact.get_metadata()?.to_str()?;
            let metadata = serde_json::from_str(metadata_str).unwrap_or_default();
            let creator = artifact.get_creator()?.to_str()?;

            Ok(Payload::ArtifactCreate {
                cas_hash: artifact.get_cas_hash()?.to_str()?.to_string(),
                tags: capnp_string_list(artifact.get_tags()?),
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
                metadata,
            })
        }

        // === Graph Tools ===
        tools_capnp::tool_request::GraphQuery(query) => {
            let query = query?;
            let variables_str = query.get_variables()?.to_str()?;
            let variables = serde_json::from_str(variables_str).unwrap_or_default();

            Ok(Payload::GraphQuery {
                query: query.get_query()?.to_str()?.to_string(),
                limit: Some(query.get_limit() as usize),
                variables,
            })
        }

        tools_capnp::tool_request::GraphBind(bind) => {
            let bind = bind?;
            let hints_reader = bind.get_hints()?;
            let mut hints = Vec::new();

            for i in 0..hints_reader.len() {
                let hint = hints_reader.get(i);
                hints.push(crate::GraphHint {
                    kind: hint.get_kind()?.to_str()?.to_string(),
                    value: hint.get_value()?.to_str()?.to_string(),
                    confidence: hint.get_confidence(),
                });
            }

            Ok(Payload::GraphBind {
                id: bind.get_id()?.to_str()?.to_string(),
                name: bind.get_name()?.to_str()?.to_string(),
                hints,
            })
        }

        tools_capnp::tool_request::GraphTag(tag) => {
            let tag = tag?;
            Ok(Payload::GraphTag {
                identity_id: tag.get_identity_id()?.to_str()?.to_string(),
                namespace: tag.get_namespace()?.to_str()?.to_string(),
                value: tag.get_value()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::GraphConnect(connect) => {
            let connect = connect?;
            let transport = connect.get_transport()?.to_str()?;

            Ok(Payload::GraphConnect {
                from_identity: connect.get_from_identity()?.to_str()?.to_string(),
                from_port: connect.get_from_port()?.to_str()?.to_string(),
                to_identity: connect.get_to_identity()?.to_str()?.to_string(),
                to_port: connect.get_to_port()?.to_str()?.to_string(),
                transport: if transport.is_empty() { None } else { Some(transport.to_string()) },
            })
        }

        tools_capnp::tool_request::GraphFind(find) => {
            let find = find?;
            let name = find.get_name()?.to_str()?;
            let tag_namespace = find.get_tag_namespace()?.to_str()?;
            let tag_value = find.get_tag_value()?.to_str()?;

            Ok(Payload::GraphFind {
                name: if name.is_empty() { None } else { Some(name.to_string()) },
                tag_namespace: if tag_namespace.is_empty() { None } else { Some(tag_namespace.to_string()) },
                tag_value: if tag_value.is_empty() { None } else { Some(tag_value.to_string()) },
            })
        }

        tools_capnp::tool_request::GraphContext(context) => {
            let context = context?;
            let tag = context.get_tag()?.to_str()?;
            let creator = context.get_creator()?.to_str()?;
            let vibe_search = context.get_vibe_search()?.to_str()?;

            Ok(Payload::GraphContext {
                limit: Some(context.get_limit() as usize),
                tag: if tag.is_empty() { None } else { Some(tag.to_string()) },
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
                vibe_search: if vibe_search.is_empty() { None } else { Some(vibe_search.to_string()) },
                include_metadata: context.get_include_metadata(),
                include_annotations: context.get_include_annotations(),
            })
        }

        tools_capnp::tool_request::AddAnnotation(annotation) => {
            let annotation = annotation?;
            let source = annotation.get_source()?.to_str()?;
            let vibe = annotation.get_vibe()?.to_str()?;

            Ok(Payload::AddAnnotation {
                artifact_id: annotation.get_artifact_id()?.to_str()?.to_string(),
                message: annotation.get_message()?.to_str()?.to_string(),
                source: if source.is_empty() { None } else { Some(source.to_string()) },
                vibe: if vibe.is_empty() { None } else { Some(vibe.to_string()) },
            })
        }

        // === Config Tools ===
        tools_capnp::tool_request::ConfigGet(config) => {
            let config = config?;
            let section = config.get_section()?.to_str()?;
            let key = config.get_key()?.to_str()?;

            Ok(Payload::ConfigGet {
                section: if section.is_empty() { None } else { Some(section.to_string()) },
                key: if key.is_empty() { None } else { Some(key.to_string()) },
            })
        }

        // === Lua Tools ===
        tools_capnp::tool_request::LuaEval(lua) => {
            let lua = lua?;
            let params_str = lua.get_params()?.to_str()?;
            let params = if params_str.is_empty() {
                None
            } else {
                serde_json::from_str(params_str).ok()
            };

            Ok(Payload::LuaEval {
                code: lua.get_code()?.to_str()?.to_string(),
                params,
            })
        }

        tools_capnp::tool_request::LuaDescribe(lua) => {
            let lua = lua?;
            Ok(Payload::LuaDescribe {
                script_hash: lua.get_script_hash()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::ScriptStore(script) => {
            let script = script?;
            let tags_reader = script.get_tags()?;
            let creator = script.get_creator()?.to_str()?;

            Ok(Payload::ScriptStore {
                content: script.get_content()?.to_str()?.to_string(),
                tags: Some(capnp_string_list(tags_reader)),
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
            })
        }

        tools_capnp::tool_request::ScriptSearch(script) => {
            let script = script?;
            let tag = script.get_tag()?.to_str()?;
            let creator = script.get_creator()?.to_str()?;
            let vibe = script.get_vibe()?.to_str()?;

            Ok(Payload::ScriptSearch {
                tag: if tag.is_empty() { None } else { Some(tag.to_string()) },
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
                vibe: if vibe.is_empty() { None } else { Some(vibe.to_string()) },
            })
        }

        // === Job Tools ===
        tools_capnp::tool_request::JobExecute(job) => {
            let job = job?;
            let params_str = job.get_params()?.to_str()?;
            let params = serde_json::from_str(params_str).unwrap_or_default();
            let tags_reader = job.get_tags()?;

            Ok(Payload::JobExecute {
                script_hash: job.get_script_hash()?.to_str()?.to_string(),
                params,
                tags: Some(capnp_string_list(tags_reader)),
            })
        }

        tools_capnp::tool_request::JobStatus(job) => {
            let job = job?;
            Ok(Payload::JobStatus {
                job_id: job.get_job_id()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::JobPoll(job) => {
            let job = job?;
            let job_ids_reader = job.get_job_ids()?;
            let mode_enum = job.get_mode()?;
            let mode = match mode_enum {
                crate::common_capnp::PollMode::All => PollMode::All,
                crate::common_capnp::PollMode::Any => PollMode::Any,
            };

            Ok(Payload::JobPoll {
                job_ids: capnp_string_list(job_ids_reader),
                timeout_ms: job.get_timeout_ms(),
                mode,
            })
        }

        tools_capnp::tool_request::JobCancel(job) => {
            let job = job?;
            Ok(Payload::JobCancel {
                job_id: job.get_job_id()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::JobList(job) => {
            let job = job?;
            let status = job.get_status()?.to_str()?;

            Ok(Payload::JobList {
                status: if status.is_empty() { None } else { Some(status.to_string()) },
            })
        }

        tools_capnp::tool_request::JobSleep(job) => {
            let job = job?;
            Ok(Payload::JobSleep {
                milliseconds: job.get_milliseconds(),
            })
        }

        // === Resource Tools ===
        tools_capnp::tool_request::ReadResource(resource) => {
            let resource = resource?;
            Ok(Payload::ReadResource {
                uri: resource.get_uri()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::ListResources(()) => {
            Ok(Payload::ListResources)
        }

        // === Completion Tools ===
        tools_capnp::tool_request::Complete(complete) => {
            let complete = complete?;

            Ok(Payload::Complete {
                context: complete.get_context()?.to_str()?.to_string(),
                partial: complete.get_partial()?.to_str()?.to_string(),
            })
        }

        // === Misc Tools ===
        tools_capnp::tool_request::SampleLlm(llm) => {
            let llm = llm?;
            let system_prompt = llm.get_system_prompt()?.to_str()?;

            Ok(Payload::SampleLlm {
                prompt: llm.get_prompt()?.to_str()?.to_string(),
                max_tokens: Some(llm.get_max_tokens()),
                temperature: Some(llm.get_temperature()),
                system_prompt: if system_prompt.is_empty() { None } else { Some(system_prompt.to_string()) },
            })
        }

        tools_capnp::tool_request::ListTools(()) => {
            Ok(Payload::ListTools)
        }
    }
}

/// Helper: Convert capnp text list to Vec<String>
fn capnp_string_list(reader: capnp::text_list::Reader) -> Vec<String> {
    let mut result = Vec::new();
    for i in 0..reader.len() {
        if let Ok(s) = reader.get(i) {
            if let Ok(s_str) = s.to_str() {
                result.push(s_str.to_string());
            }
        }
    }
    result
}

/// Helper: Convert optional capnp text to Option<String>
fn capnp_optional_string(text: capnp::text::Reader) -> Option<String> {
    match text.to_str() {
        Ok(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

/// Helper: Convert WorkerType to capnp enum
fn worker_type_to_capnp(wt: &WorkerType) -> common_capnp::WorkerType {
    match wt {
        WorkerType::Luanette => common_capnp::WorkerType::Luanette,
        WorkerType::Hootenanny => common_capnp::WorkerType::Hootenanny,
        WorkerType::Chaosgarden => common_capnp::WorkerType::Chaosgarden,
    }
}

/// Helper: Convert PollMode to capnp enum
fn poll_mode_to_capnp(mode: &PollMode) -> common_capnp::PollMode {
    match mode {
        PollMode::Any => common_capnp::PollMode::Any,
        PollMode::All => common_capnp::PollMode::All,
    }
}

/// Helper: Convert TimelineEventType to capnp enum
fn timeline_event_type_to_capnp(et: &TimelineEventType) -> common_capnp::TimelineEventType {
    match et {
        TimelineEventType::SectionChange => common_capnp::TimelineEventType::SectionChange,
        TimelineEventType::BeatMarker => common_capnp::TimelineEventType::BeatMarker,
        TimelineEventType::CuePoint => common_capnp::TimelineEventType::CuePoint,
        TimelineEventType::GenerateTransition => common_capnp::TimelineEventType::GenerateTransition,
    }
}

/// Helper: Set artifact metadata on a capnp builder
fn set_artifact_metadata(
    builder: &mut common_capnp::artifact_metadata::Builder,
    variation_set_id: &Option<String>,
    parent_id: &Option<String>,
    tags: &[String],
    creator: &Option<String>,
) {
    builder.set_variation_set_id(variation_set_id.as_deref().unwrap_or(""));
    builder.set_parent_id(parent_id.as_deref().unwrap_or(""));
    {
        let mut tags_builder = builder.reborrow().init_tags(tags.len() as u32);
        for (i, tag) in tags.iter().enumerate() {
            tags_builder.set(i as u32, tag);
        }
    }
    builder.set_creator(creator.as_deref().unwrap_or(""));
}

/// Helper: Set StreamDefinition on a capnp builder
fn set_stream_definition(
    builder: &mut streams_capnp::stream_definition::Builder,
    def: &StreamDefinition,
) {
    builder.set_uri(&def.uri);
    builder.set_device_identity(&def.device_identity);
    builder.set_chunk_size_bytes(def.chunk_size_bytes);

    let mut format = builder.reborrow().init_format();
    match &def.format {
        StreamFormat::Audio { sample_rate, channels, sample_format } => {
            let mut audio = format.init_audio();
            audio.set_sample_rate(*sample_rate);
            audio.set_channels(*channels);
            audio.set_sample_format(match sample_format {
                SampleFormat::F32 => streams_capnp::SampleFormat::F32,
                SampleFormat::I16 => streams_capnp::SampleFormat::I16,
                SampleFormat::I24 => streams_capnp::SampleFormat::I24,
            });
        }
        StreamFormat::Midi => {
            format.set_midi(());
        }
    }
}

/// Convert a Payload response to Cap'n Proto Envelope
pub fn payload_to_capnp_envelope(
    request_id: uuid::Uuid,
    payload: &Payload,
) -> capnp::Result<capnp::message::Builder<capnp::message::HeapAllocator>> {
    let mut message = capnp::message::Builder::new_default();

    {
        let mut envelope = message.init_root::<envelope_capnp::envelope::Builder>();

        // Set request ID
        let mut id = envelope.reborrow().init_id();
        let bytes = request_id.as_bytes();
        id.set_low(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]));
        id.set_high(u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]));

        // Empty traceparent for now
        envelope.reborrow().set_traceparent("");

        // Set payload
        let mut payload_builder = envelope.init_payload();
        payload_to_capnp_payload(&mut payload_builder, payload)?;
    }

    Ok(message)
}

/// Convert Payload to Cap'n Proto Payload builder
fn payload_to_capnp_payload(
    builder: &mut envelope_capnp::payload::Builder,
    payload: &Payload,
) -> capnp::Result<()> {
    match payload {
        Payload::Success { result } => {
            let mut success = builder.reborrow().init_success();
            success.set_result(serde_json::to_string(result).unwrap_or_default());
        }

        Payload::Error { code, message, details } => {
            let mut error = builder.reborrow().init_error();
            error.set_code(code);
            error.set_message(message);
            if let Some(ref d) = details {
                error.set_details(serde_json::to_string(d).unwrap_or_default());
            } else {
                error.set_details("");
            }
        }

        Payload::ToolList { tools } => {
            let tool_list = builder.reborrow().init_tool_list();
            let mut tools_builder = tool_list.init_tools(tools.len() as u32);

            for (i, tool) in tools.iter().enumerate() {
                let mut tool_builder = tools_builder.reborrow().get(i as u32);
                tool_builder.set_name(&tool.name);
                tool_builder.set_description(&tool.description);
                tool_builder.set_input_schema(
                    serde_json::to_string(&tool.input_schema).unwrap_or_default(),
                );
            }
        }

        // Tool requests - serialize as toolRequest variant
        Payload::Ping => {
            builder.reborrow().set_ping(());
        }

        Payload::ListTools => {
            builder.reborrow().init_tool_request().set_list_tools(());
        }

        Payload::ListResources => {
            builder.reborrow().init_tool_request().set_list_resources(());
        }

        // Garden/Timeline payloads - direct envelope variants
        Payload::GardenStatus => {
            builder.reborrow().set_garden_status(());
        }

        Payload::GardenPlay => {
            builder.reborrow().set_garden_play(());
        }

        Payload::GardenPause => {
            builder.reborrow().set_garden_pause(());
        }

        Payload::GardenStop => {
            builder.reborrow().set_garden_stop(());
        }

        Payload::GardenSeek { beat } => {
            let mut seek = builder.reborrow().init_garden_seek();
            seek.set_beat(*beat);
        }

        Payload::GardenSetTempo { bpm } => {
            let mut tempo = builder.reborrow().init_garden_set_tempo();
            tempo.set_bpm(*bpm);
        }

        Payload::GardenQuery { query, variables } => {
            let mut q = builder.reborrow().init_garden_query();
            q.set_query(query);
            if let Some(ref vars) = variables {
                q.set_variables(serde_json::to_string(vars).unwrap_or_default());
            } else {
                q.set_variables("");
            }
        }

        Payload::GardenEmergencyPause => {
            builder.reborrow().set_garden_emergency_pause(());
        }

        Payload::GardenCreateRegion { position, duration, behavior_type, content_id } => {
            let mut region = builder.reborrow().init_garden_create_region();
            region.set_position(*position);
            region.set_duration(*duration);
            region.set_behavior_type(behavior_type);
            region.set_content_id(content_id);
        }

        Payload::GardenDeleteRegion { region_id } => {
            let mut region = builder.reborrow().init_garden_delete_region();
            region.set_region_id(region_id);
        }

        Payload::GardenMoveRegion { region_id, new_position } => {
            let mut region = builder.reborrow().init_garden_move_region();
            region.set_region_id(region_id);
            region.set_new_position(*new_position);
        }

        Payload::GardenGetRegions { start, end } => {
            let mut regions = builder.reborrow().init_garden_get_regions();
            regions.set_start(start.unwrap_or(0.0));
            regions.set_end(end.unwrap_or(0.0));
        }

        // === Transport Commands (Direct envelope) ===
        Payload::TransportPlay => {
            builder.reborrow().set_transport_play(());
        }

        Payload::TransportStop => {
            builder.reborrow().set_transport_stop(());
        }

        Payload::TransportSeek { position_beats } => {
            let mut seek = builder.reborrow().init_transport_seek();
            seek.set_position_beats(*position_beats);
        }

        Payload::TransportStatus => {
            builder.reborrow().set_transport_status(());
        }

        // === Timeline Commands (Direct envelope) ===
        Payload::TimelineQuery { from_beats, to_beats } => {
            let mut q = builder.reborrow().init_timeline_query();
            q.set_from_beats(from_beats.unwrap_or(0.0));
            q.set_to_beats(to_beats.unwrap_or(0.0));
        }

        Payload::TimelineAddMarker { position_beats, marker_type, metadata } => {
            let mut marker = builder.reborrow().init_timeline_add_marker();
            marker.set_position_beats(*position_beats);
            marker.set_marker_type(marker_type);
            marker.set_metadata(serde_json::to_string(metadata).unwrap_or_default());
        }

        Payload::TimelineEvent { event_type, position_beats, tempo, metadata } => {
            let mut event = builder.reborrow().init_timeline_event();
            event.set_event_type(timeline_event_type_to_capnp(event_type));
            event.set_position_beats(*position_beats);
            event.set_tempo(*tempo);
            event.set_metadata(serde_json::to_string(metadata).unwrap_or_default());
        }

        // === Stream Commands (Direct envelope) ===
        Payload::StreamStart { uri, definition, chunk_path } => {
            let mut start = builder.reborrow().init_stream_start();
            start.set_uri(uri);
            set_stream_definition(&mut start.reborrow().init_definition(), definition);
            start.set_chunk_path(chunk_path);
        }

        Payload::StreamSwitchChunk { uri, new_chunk_path } => {
            let mut switch = builder.reborrow().init_stream_switch_chunk();
            switch.set_uri(uri);
            switch.set_new_chunk_path(new_chunk_path);
        }

        Payload::StreamStop { uri } => {
            let mut stop = builder.reborrow().init_stream_stop();
            stop.set_uri(uri);
        }

        // === CAS Tools (ToolRequest) ===
        Payload::CasStore { data, mime_type } => {
            let mut req = builder.reborrow().init_tool_request().init_cas_store();
            req.set_data(data);
            req.set_mime_type(mime_type);
        }

        Payload::CasInspect { hash } => {
            let mut req = builder.reborrow().init_tool_request().init_cas_inspect();
            req.set_hash(hash);
        }

        Payload::CasGet { hash } => {
            let mut req = builder.reborrow().init_tool_request().init_cas_get();
            req.set_hash(hash);
        }

        Payload::CasUploadFile { file_path, mime_type } => {
            let mut req = builder.reborrow().init_tool_request().init_cas_upload_file();
            req.set_file_path(file_path);
            req.set_mime_type(mime_type);
        }

        // === Orpheus Tools (ToolRequest) ===
        Payload::OrpheusGenerate {
            model, temperature, top_p, max_tokens, num_variations,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_orpheus_generate();
            req.set_model(model.as_deref().unwrap_or(""));
            req.set_temperature(temperature.unwrap_or(1.0));
            req.set_top_p(top_p.unwrap_or(0.95));
            req.set_max_tokens(max_tokens.unwrap_or(1024));
            req.set_num_variations(num_variations.unwrap_or(1));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::OrpheusGenerateSeeded {
            seed_hash, model, temperature, top_p, max_tokens, num_variations,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_orpheus_generate_seeded();
            req.set_seed_hash(seed_hash);
            req.set_model(model.as_deref().unwrap_or(""));
            req.set_temperature(temperature.unwrap_or(1.0));
            req.set_top_p(top_p.unwrap_or(0.95));
            req.set_max_tokens(max_tokens.unwrap_or(1024));
            req.set_num_variations(num_variations.unwrap_or(1));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::OrpheusContinue {
            input_hash, model, temperature, top_p, max_tokens, num_variations,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_orpheus_continue();
            req.set_input_hash(input_hash);
            req.set_model(model.as_deref().unwrap_or(""));
            req.set_temperature(temperature.unwrap_or(1.0));
            req.set_top_p(top_p.unwrap_or(0.95));
            req.set_max_tokens(max_tokens.unwrap_or(1024));
            req.set_num_variations(num_variations.unwrap_or(1));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::OrpheusBridge {
            section_a_hash, section_b_hash, model, temperature, top_p, max_tokens,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_orpheus_bridge();
            req.set_section_a_hash(section_a_hash);
            req.set_section_b_hash(section_b_hash.as_deref().unwrap_or(""));
            req.set_model(model.as_deref().unwrap_or(""));
            req.set_temperature(temperature.unwrap_or(1.0));
            req.set_top_p(top_p.unwrap_or(0.95));
            req.set_max_tokens(max_tokens.unwrap_or(1024));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::OrpheusLoops {
            temperature, top_p, max_tokens, num_variations, seed_hash,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_orpheus_loops();
            req.set_temperature(temperature.unwrap_or(1.0));
            req.set_top_p(top_p.unwrap_or(0.95));
            req.set_max_tokens(max_tokens.unwrap_or(1024));
            req.set_num_variations(num_variations.unwrap_or(1));
            req.set_seed_hash(seed_hash.as_deref().unwrap_or(""));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::OrpheusClassify { midi_hash } => {
            let mut req = builder.reborrow().init_tool_request().init_orpheus_classify();
            req.set_midi_hash(midi_hash);
        }

        // === ABC Tools (ToolRequest) ===
        Payload::AbcParse { abc } => {
            let mut req = builder.reborrow().init_tool_request().init_abc_parse();
            req.set_abc(abc);
        }

        Payload::AbcToMidi {
            abc, tempo_override, transpose, velocity, channel,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_abc_to_midi();
            req.set_abc(abc);
            req.set_tempo_override(tempo_override.unwrap_or(0));
            req.set_transpose(transpose.unwrap_or(0));
            req.set_velocity(velocity.unwrap_or(80));
            req.set_channel(channel.unwrap_or(0));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::AbcValidate { abc } => {
            let mut req = builder.reborrow().init_tool_request().init_abc_validate();
            req.set_abc(abc);
        }

        Payload::AbcTranspose { abc, semitones, target_key } => {
            let mut req = builder.reborrow().init_tool_request().init_abc_transpose();
            req.set_abc(abc);
            req.set_semitones(semitones.unwrap_or(0));
            req.set_target_key(target_key.as_deref().unwrap_or(""));
        }

        // === MIDI/Audio Tools (ToolRequest) ===
        Payload::ConvertMidiToWav {
            input_hash, soundfont_hash, sample_rate,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_convert_midi_to_wav();
            req.set_input_hash(input_hash);
            req.set_soundfont_hash(soundfont_hash);
            req.set_sample_rate(sample_rate.unwrap_or(44100));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::SoundfontInspect { soundfont_hash, include_drum_map } => {
            let mut req = builder.reborrow().init_tool_request().init_soundfont_inspect();
            req.set_soundfont_hash(soundfont_hash);
            req.set_include_drum_map(*include_drum_map);
        }

        Payload::SoundfontPresetInspect { soundfont_hash, bank, program } => {
            let mut req = builder.reborrow().init_tool_request().init_soundfont_preset_inspect();
            req.set_soundfont_hash(soundfont_hash);
            req.set_bank(*bank);
            req.set_program(*program);
        }

        // === Analysis Tools (ToolRequest) ===
        Payload::BeatthisAnalyze { audio_path, audio_hash, include_frames } => {
            let mut req = builder.reborrow().init_tool_request().init_beatthis_analyze();
            req.set_audio_path(audio_path.as_deref().unwrap_or(""));
            req.set_audio_hash(audio_hash.as_deref().unwrap_or(""));
            req.set_include_frames(*include_frames);
        }

        Payload::ClapAnalyze {
            audio_hash, tasks, audio_b_hash, text_candidates, parent_id, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_clap_analyze();
            req.set_audio_hash(audio_hash);
            {
                let mut tasks_builder = req.reborrow().init_tasks(tasks.len() as u32);
                for (i, task) in tasks.iter().enumerate() {
                    tasks_builder.set(i as u32, task);
                }
            }
            req.set_audio_b_hash(audio_b_hash.as_deref().unwrap_or(""));
            {
                let mut candidates = req.reborrow().init_text_candidates(text_candidates.len() as u32);
                for (i, c) in text_candidates.iter().enumerate() {
                    candidates.set(i as u32, c);
                }
            }
            req.set_parent_id(parent_id.as_deref().unwrap_or(""));
            req.set_creator(creator.as_deref().unwrap_or(""));
        }

        // === Generation Tools (ToolRequest) ===
        Payload::MusicgenGenerate {
            prompt, duration, temperature, top_k, top_p, guidance_scale, do_sample,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_musicgen_generate();
            req.set_prompt(prompt.as_deref().unwrap_or(""));
            req.set_duration(duration.unwrap_or(10.0));
            req.set_temperature(temperature.unwrap_or(1.0));
            req.set_top_k(top_k.unwrap_or(250));
            req.set_top_p(top_p.unwrap_or(0.9));
            req.set_guidance_scale(guidance_scale.unwrap_or(3.0));
            req.set_do_sample(do_sample.unwrap_or(true));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::YueGenerate {
            lyrics, genre, max_new_tokens, run_n_segments, seed,
            variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_yue_generate();
            req.set_lyrics(lyrics);
            req.set_genre(genre.as_deref().unwrap_or("Pop"));
            req.set_max_new_tokens(max_new_tokens.unwrap_or(3000));
            req.set_run_n_segments(run_n_segments.unwrap_or(2));
            req.set_seed(seed.unwrap_or(42));
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        // === Artifact Tools (ToolRequest) ===
        Payload::ArtifactUpload {
            file_path, mime_type, variation_set_id, parent_id, tags, creator
        } => {
            let mut req = builder.reborrow().init_tool_request().init_artifact_upload();
            req.set_file_path(file_path);
            req.set_mime_type(mime_type);
            set_artifact_metadata(&mut req.reborrow().init_metadata(), variation_set_id, parent_id, tags, creator);
        }

        Payload::ArtifactGet { id } => {
            let mut req = builder.reborrow().init_tool_request().init_artifact_get();
            req.set_id(id);
        }

        Payload::ArtifactList { tag, creator } => {
            let mut req = builder.reborrow().init_tool_request().init_artifact_list();
            req.set_tag(tag.as_deref().unwrap_or(""));
            req.set_creator(creator.as_deref().unwrap_or(""));
        }

        Payload::ArtifactCreate { cas_hash, tags, creator, metadata } => {
            let mut req = builder.reborrow().init_tool_request().init_artifact_create();
            req.set_cas_hash(cas_hash);
            {
                let mut tags_builder = req.reborrow().init_tags(tags.len() as u32);
                for (i, tag) in tags.iter().enumerate() {
                    tags_builder.set(i as u32, tag);
                }
            }
            req.set_creator(creator.as_deref().unwrap_or(""));
            req.set_metadata(serde_json::to_string(metadata).unwrap_or_default());
        }

        // === Graph Tools (ToolRequest) ===
        Payload::GraphQuery { query, variables, limit } => {
            let mut req = builder.reborrow().init_tool_request().init_graph_query();
            req.set_query(query);
            req.set_variables(serde_json::to_string(variables).unwrap_or_default());
            req.set_limit(limit.unwrap_or(100) as u32);
        }

        Payload::GraphBind { id, name, hints } => {
            let mut req = builder.reborrow().init_tool_request().init_graph_bind();
            req.set_id(id);
            req.set_name(name);
            {
                let mut hints_builder = req.reborrow().init_hints(hints.len() as u32);
                for (i, hint) in hints.iter().enumerate() {
                    let mut h = hints_builder.reborrow().get(i as u32);
                    h.set_kind(&hint.kind);
                    h.set_value(&hint.value);
                    h.set_confidence(hint.confidence);
                }
            }
        }

        Payload::GraphTag { identity_id, namespace, value } => {
            let mut req = builder.reborrow().init_tool_request().init_graph_tag();
            req.set_identity_id(identity_id);
            req.set_namespace(namespace);
            req.set_value(value);
        }

        Payload::GraphConnect { from_identity, from_port, to_identity, to_port, transport } => {
            let mut req = builder.reborrow().init_tool_request().init_graph_connect();
            req.set_from_identity(from_identity);
            req.set_from_port(from_port);
            req.set_to_identity(to_identity);
            req.set_to_port(to_port);
            req.set_transport(transport.as_deref().unwrap_or(""));
        }

        Payload::GraphFind { name, tag_namespace, tag_value } => {
            let mut req = builder.reborrow().init_tool_request().init_graph_find();
            req.set_name(name.as_deref().unwrap_or(""));
            req.set_tag_namespace(tag_namespace.as_deref().unwrap_or(""));
            req.set_tag_value(tag_value.as_deref().unwrap_or(""));
        }

        Payload::GraphContext {
            tag, vibe_search, creator, limit, include_metadata, include_annotations
        } => {
            let mut req = builder.reborrow().init_tool_request().init_graph_context();
            req.set_tag(tag.as_deref().unwrap_or(""));
            req.set_vibe_search(vibe_search.as_deref().unwrap_or(""));
            req.set_creator(creator.as_deref().unwrap_or(""));
            req.set_limit(limit.unwrap_or(20) as u32);
            req.set_include_metadata(*include_metadata);
            req.set_include_annotations(*include_annotations);
        }

        Payload::AddAnnotation { artifact_id, message, vibe, source } => {
            let mut req = builder.reborrow().init_tool_request().init_add_annotation();
            req.set_artifact_id(artifact_id);
            req.set_message(message);
            req.set_vibe(vibe.as_deref().unwrap_or(""));
            req.set_source(source.as_deref().unwrap_or(""));
        }

        // === Config Tools (ToolRequest) ===
        Payload::ConfigGet { section, key } => {
            let mut req = builder.reborrow().init_tool_request().init_config_get();
            req.set_section(section.as_deref().unwrap_or(""));
            req.set_key(key.as_deref().unwrap_or(""));
        }

        // === Lua Tools (ToolRequest) ===
        Payload::LuaEval { code, params } => {
            let mut req = builder.reborrow().init_tool_request().init_lua_eval();
            req.set_code(code);
            req.set_params(
                params
                    .as_ref()
                    .map(|p| serde_json::to_string(p).unwrap_or_default())
                    .unwrap_or_default(),
            );
        }

        Payload::LuaDescribe { script_hash } => {
            let mut req = builder.reborrow().init_tool_request().init_lua_describe();
            req.set_script_hash(script_hash);
        }

        Payload::ScriptStore { content, tags, creator } => {
            let mut req = builder.reborrow().init_tool_request().init_script_store();
            req.set_content(content);
            if let Some(ref tags_vec) = tags {
                let mut tags_builder = req.reborrow().init_tags(tags_vec.len() as u32);
                for (i, tag) in tags_vec.iter().enumerate() {
                    tags_builder.set(i as u32, tag);
                }
            }
            req.set_creator(creator.as_deref().unwrap_or(""));
        }

        Payload::ScriptSearch { tag, creator, vibe } => {
            let mut req = builder.reborrow().init_tool_request().init_script_search();
            req.set_tag(tag.as_deref().unwrap_or(""));
            req.set_creator(creator.as_deref().unwrap_or(""));
            req.set_vibe(vibe.as_deref().unwrap_or(""));
        }

        // === Job Tools (ToolRequest) ===
        Payload::JobExecute { script_hash, params, tags } => {
            let mut req = builder.reborrow().init_tool_request().init_job_execute();
            req.set_script_hash(script_hash);
            req.set_params(serde_json::to_string(params).unwrap_or_default());
            if let Some(ref tags_vec) = tags {
                let mut tags_builder = req.reborrow().init_tags(tags_vec.len() as u32);
                for (i, tag) in tags_vec.iter().enumerate() {
                    tags_builder.set(i as u32, tag);
                }
            }
        }

        Payload::JobStatus { job_id } => {
            let mut req = builder.reborrow().init_tool_request().init_job_status();
            req.set_job_id(job_id);
        }

        Payload::JobPoll { job_ids, timeout_ms, mode } => {
            let mut req = builder.reborrow().init_tool_request().init_job_poll();
            {
                let mut ids = req.reborrow().init_job_ids(job_ids.len() as u32);
                for (i, id) in job_ids.iter().enumerate() {
                    ids.set(i as u32, id);
                }
            }
            req.set_timeout_ms(*timeout_ms);
            req.set_mode(poll_mode_to_capnp(mode));
        }

        Payload::JobCancel { job_id } => {
            let mut req = builder.reborrow().init_tool_request().init_job_cancel();
            req.set_job_id(job_id);
        }

        Payload::JobList { status } => {
            let mut req = builder.reborrow().init_tool_request().init_job_list();
            req.set_status(status.as_deref().unwrap_or(""));
        }

        Payload::JobSleep { milliseconds } => {
            let mut req = builder.reborrow().init_tool_request().init_job_sleep();
            req.set_milliseconds(*milliseconds);
        }

        // === Resource Tools (ToolRequest) ===
        Payload::ReadResource { uri } => {
            let mut req = builder.reborrow().init_tool_request().init_read_resource();
            req.set_uri(uri);
        }

        Payload::Complete { context, partial } => {
            let mut req = builder.reborrow().init_tool_request().init_complete();
            req.set_context(context);
            req.set_partial(partial);
        }

        Payload::SampleLlm { prompt, max_tokens, temperature, system_prompt } => {
            let mut req = builder.reborrow().init_tool_request().init_sample_llm();
            req.set_prompt(prompt);
            req.set_max_tokens(max_tokens.unwrap_or(1024));
            req.set_temperature(temperature.unwrap_or(0.7));
            req.set_system_prompt(system_prompt.as_deref().unwrap_or(""));
        }

        // === Worker Management (Direct envelope) ===
        Payload::Register(registration) => {
            let mut reg = builder.reborrow().init_register();
            let mut worker_id = reg.reborrow().init_worker_id();
            let bytes = registration.worker_id.as_bytes();
            worker_id.set_low(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]));
            worker_id.set_high(u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]));
            reg.set_worker_type(worker_type_to_capnp(&registration.worker_type));
            reg.set_worker_name(&registration.worker_name);
            {
                let mut caps = reg.reborrow().init_capabilities(registration.capabilities.len() as u32);
                for (i, cap) in registration.capabilities.iter().enumerate() {
                    caps.set(i as u32, cap);
                }
            }
            reg.set_hostname(&registration.hostname);
            reg.set_version(&registration.version);
        }

        Payload::Pong { worker_id, uptime_secs } => {
            let mut pong = builder.reborrow().init_pong();
            let mut wid = pong.reborrow().init_worker_id();
            let bytes = worker_id.as_bytes();
            wid.set_low(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]));
            wid.set_high(u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]));
            pong.set_uptime_secs(*uptime_secs);
        }

        Payload::Shutdown { reason } => {
            let mut shutdown = builder.reborrow().init_shutdown();
            shutdown.set_reason(reason);
        }

        // Generic tool call - name + JSON args
        Payload::ToolCall { name, args } => {
            let mut tool_call = builder.reborrow().init_tool_call();
            tool_call.set_name(name);
            tool_call.set_args(serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abc_parse_conversion() {
        let payload = Payload::AbcParse {
            abc: "X:1\nT:Test\nK:C\nCDEF".to_string(),
        };

        let request = payload_to_request(&payload).unwrap();
        assert!(matches!(request, Some(ToolRequest::AbcParse(_))));

        if let Some(ToolRequest::AbcParse(req)) = request {
            assert_eq!(req.abc, "X:1\nT:Test\nK:C\nCDEF");
        }
    }

    #[test]
    fn test_garden_status_conversion() {
        let payload = Payload::GardenStatus;
        let request = payload_to_request(&payload).unwrap();
        assert!(matches!(request, Some(ToolRequest::GardenStatus)));
    }

    #[test]
    fn test_unsupported_returns_none() {
        let payload = Payload::OrpheusGenerate {
            max_tokens: Some(1024),
            num_variations: Some(1),
            temperature: None,
            top_p: None,
            model: None,
            tags: vec![],
            creator: None,
            parent_id: None,
            variation_set_id: None,
        };

        let request = payload_to_request(&payload).unwrap();
        assert!(request.is_none());
    }

    #[test]
    fn test_envelope_to_payload_ack() {
        let envelope = ResponseEnvelope::ack("test");
        let payload = envelope_to_payload(envelope);

        match payload {
            Payload::Success { result } => {
                assert_eq!(result["status"], "ok");
                assert_eq!(result["message"], "test");
            }
            _ => panic!("Expected Success payload"),
        }
    }
}
