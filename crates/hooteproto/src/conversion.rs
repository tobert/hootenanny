//! Typed protocol conversions for hooteproto
//!
//! Provides conversions between:
//! - Payload variants ↔ Typed ToolRequest structs (for dispatch)
//! - ResponseEnvelope ↔ Payload (for ZMQ transport)
//! - Payload ↔ Cap'n Proto (for wire serialization)

use crate::{
    AnalysisTask, Encoding, Payload, SampleFormat, StreamDefinition, StreamFormat,
    TimelineEventType,
};

// Cap'n Proto imports for reading requests
use crate::{common_capnp, envelope_capnp, streams_capnp, tools_capnp};

use crate::envelope::ResponseEnvelope;
use crate::request::*;
use crate::responses::ToolResponse;
use crate::ToolError;

/// Convert a Payload to a ToolRequest for typed dispatch.
pub fn payload_to_request(payload: &Payload) -> Result<Option<ToolRequest>, ToolError> {
    match payload {
        Payload::ToolRequest(request) => Ok(Some(request.clone())),
        _ => Ok(None),
    }
}

/// Convert a ResponseEnvelope back to Payload for ZMQ transport.
pub fn envelope_to_payload(envelope: ResponseEnvelope) -> Payload {
    match &envelope {
        ResponseEnvelope::Error(err) => Payload::Error {
            code: err.code().to_string(),
            message: err.message().to_string(),
            details: None,
        },
        _ => Payload::TypedResponse(envelope),
    }
}

/// Convert a Cap'n Proto Envelope reader to Payload
pub fn capnp_envelope_to_payload(
    reader: envelope_capnp::envelope::Reader,
) -> capnp::Result<Payload> {
    let payload_reader = reader.get_payload()?;

    match payload_reader.which()? {
        envelope_capnp::payload::Ping(()) => Ok(Payload::Ping),
        envelope_capnp::payload::Shutdown(shutdown) => {
            let reason = shutdown?.get_reason()?.to_str()?.to_string();
            Ok(Payload::Shutdown { reason })
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

        envelope_capnp::payload::ToolRequest(tool_req) => {
            let tool_req = tool_req?;
            let request = capnp_tool_request_to_request(tool_req)?;
            Ok(Payload::ToolRequest(request))
        }

        // Legacy/Fallback: Handle direct Garden commands from Envelope by converting to ToolRequest
        envelope_capnp::payload::GardenStatus(()) => Ok(Payload::ToolRequest(ToolRequest::GardenStatus)),
        envelope_capnp::payload::GardenPlay(()) => Ok(Payload::ToolRequest(ToolRequest::GardenPlay)),
        envelope_capnp::payload::GardenPause(()) => Ok(Payload::ToolRequest(ToolRequest::GardenPause)),
        envelope_capnp::payload::GardenStop(()) => Ok(Payload::ToolRequest(ToolRequest::GardenStop)),
        envelope_capnp::payload::GardenSeek(seek) => Ok(Payload::ToolRequest(ToolRequest::GardenSeek(GardenSeekRequest { beat: seek?.get_beat() }))),
        envelope_capnp::payload::GardenSetTempo(tempo) => Ok(Payload::ToolRequest(ToolRequest::GardenSetTempo(GardenSetTempoRequest { bpm: tempo?.get_bpm() }))),
        envelope_capnp::payload::GardenQuery(query) => {
            let query = query?;
            Ok(Payload::ToolRequest(ToolRequest::GardenQuery(GardenQueryRequest {
                query: query.get_query()?.to_str()?.to_string(),
                variables: serde_json::from_str(query.get_variables()?.to_str()?).ok(),
            })))
        }
        envelope_capnp::payload::GardenEmergencyPause(()) => Ok(Payload::ToolRequest(ToolRequest::GardenEmergencyPause)),
        envelope_capnp::payload::GardenCreateRegion(region) => {
            let region = region?;
            Ok(Payload::ToolRequest(ToolRequest::GardenCreateRegion(GardenCreateRegionRequest {
                position: region.get_position(),
                duration: region.get_duration(),
                behavior_type: region.get_behavior_type()?.to_str()?.to_string(),
                content_id: region.get_content_id()?.to_str()?.to_string(),
            })))
        }
        envelope_capnp::payload::GardenDeleteRegion(region) => Ok(Payload::ToolRequest(ToolRequest::GardenDeleteRegion(GardenDeleteRegionRequest { region_id: region?.get_region_id()?.to_str()?.to_string() }))),
        envelope_capnp::payload::GardenMoveRegion(region) => {
            let region = region?;
            Ok(Payload::ToolRequest(ToolRequest::GardenMoveRegion(GardenMoveRegionRequest {
                region_id: region.get_region_id()?.to_str()?.to_string(),
                new_position: region.get_new_position(),
            })))
        }
        envelope_capnp::payload::GardenGetRegions(regions) => {
            let regions = regions?;
            let start = regions.get_start();
            let end = regions.get_end();
            Ok(Payload::ToolRequest(ToolRequest::GardenGetRegions(GardenGetRegionsRequest {
                start: if start == 0.0 { None } else { Some(start) },
                end: if end == 0.0 { None } else { Some(end) },
            })))
        }

        // Direct Protocol Messages (Transport/Timeline/Stream)
        envelope_capnp::payload::TransportPlay(()) => Ok(Payload::TransportPlay),
        envelope_capnp::payload::TransportStop(()) => Ok(Payload::TransportStop),
        envelope_capnp::payload::TransportStatus(()) => Ok(Payload::TransportStatus),
        envelope_capnp::payload::TransportSeek(seek) => Ok(Payload::TransportSeek { position_beats: seek?.get_position_beats() }),
        envelope_capnp::payload::TimelineQuery(query) => {
            let query = query?;
            Ok(Payload::TimelineQuery { from_beats: Some(query.get_from_beats()), to_beats: Some(query.get_to_beats()) })
        }
        envelope_capnp::payload::TimelineAddMarker(marker) => {
            let marker = marker?;
            Ok(Payload::TimelineAddMarker {
                position_beats: marker.get_position_beats(),
                marker_type: marker.get_marker_type()?.to_str()?.to_string(),
                metadata: serde_json::from_str(marker.get_metadata()?.to_str()?).unwrap_or_default(),
            })
        }
        envelope_capnp::payload::TimelineEvent(event) => {
            let event = event?;
            Ok(Payload::TimelineEvent {
                event_type: match event.get_event_type()? {
                    common_capnp::TimelineEventType::SectionChange => TimelineEventType::SectionChange,
                    common_capnp::TimelineEventType::BeatMarker => TimelineEventType::BeatMarker,
                    common_capnp::TimelineEventType::CuePoint => TimelineEventType::CuePoint,
                    common_capnp::TimelineEventType::GenerateTransition => TimelineEventType::GenerateTransition,
                },
                position_beats: event.get_position_beats(),
                tempo: event.get_tempo(),
                metadata: serde_json::from_str(event.get_metadata()?.to_str()?).unwrap_or_default(),
            })
        }
        envelope_capnp::payload::StreamStart(stream) => {
            let stream = stream?;
            let def = stream.get_definition()?;
            let format = def.get_format()?;
            let stream_format = match format.which()? {
                streams_capnp::stream_format::Audio(audio) => {
                    let audio = audio?;
                    let sample_format = match audio.get_sample_format()? {
                        streams_capnp::SampleFormat::F32 => SampleFormat::F32,
                        streams_capnp::SampleFormat::I16 => SampleFormat::I16,
                        streams_capnp::SampleFormat::I24 => SampleFormat::I24,
                    };
                    StreamFormat::Audio {
                        sample_rate: audio.get_sample_rate(),
                        channels: audio.get_channels(),
                        sample_format,
                    }
                }
                streams_capnp::stream_format::Midi(()) => StreamFormat::Midi,
            };
            Ok(Payload::StreamStart {
                uri: stream.get_uri()?.to_str()?.to_string(),
                definition: StreamDefinition {
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
            Ok(Payload::StreamSwitchChunk { uri: stream.get_uri()?.to_str()?.to_string(), new_chunk_path: stream.get_new_chunk_path()?.to_str()?.to_string() })
        }
        envelope_capnp::payload::StreamStop(stream) => Ok(Payload::StreamStop { uri: stream?.get_uri()?.to_str()?.to_string() }),

        envelope_capnp::payload::Success(success) => {
            let result_str = success?.get_result()?.to_str()?;
            let response = serde_json::from_str::<ToolResponse>(result_str).map_err(|e| capnp::Error::failed(format!("Unknown response: {}", e)))?;
            Ok(Payload::TypedResponse(ResponseEnvelope::success(response)))
        }
        envelope_capnp::payload::Error(error) => {
            let error = error?;
            Ok(Payload::Error {
                code: error.get_code()?.to_str()?.to_string(),
                message: error.get_message()?.to_str()?.to_string(),
                details: None,
            })
        }
        envelope_capnp::payload::ToolList(list) => {
            let reader = list?.get_tools()?;
            let mut tools = Vec::new();
            for i in 0..reader.len() {
                let t = reader.get(i);
                tools.push(crate::ToolInfo {
                    name: t.get_name()?.to_str()?.to_string(),
                    description: t.get_description()?.to_str()?.to_string(),
                    input_schema: serde_json::from_str(t.get_input_schema()?.to_str()?).unwrap_or_default(),
                });
            }
            Ok(Payload::ToolList { tools })
        }

        envelope_capnp::payload::ToolCall(call) => Err(capnp::Error::failed(format!("ToolCall deprecated: {}", call?.get_name()?.to_str()?))),
        envelope_capnp::payload::Register(_) => Err(capnp::Error::failed("Register unimplemented".to_string())),
    }
}

pub fn payload_to_capnp_envelope(
    request_id: uuid::Uuid,
    payload: &Payload,
) -> capnp::Result<capnp::message::Builder<capnp::message::HeapAllocator>> {
    let mut message = capnp::message::Builder::new_default();
    {
        let mut envelope = message.init_root::<envelope_capnp::envelope::Builder>();
        let mut id = envelope.reborrow().init_id();
        let bytes = request_id.as_bytes();
        id.set_low(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]));
        id.set_high(u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]));
        envelope.reborrow().set_traceparent("");
        let mut payload_builder = envelope.init_payload();
        match payload {
            Payload::Ping => payload_builder.set_ping(()),
            Payload::Pong { worker_id, uptime_secs } => {
                let mut p = payload_builder.init_pong();
                let mut id = p.reborrow().init_worker_id();
                let bytes = worker_id.as_bytes();
                id.set_low(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]));
                id.set_high(u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]));
                p.set_uptime_secs(*uptime_secs);
            }
            Payload::Shutdown { reason } => payload_builder.init_shutdown().set_reason(reason),
            Payload::ToolRequest(tr) => match tr {
                // Ping is system but exists in ToolRequest for Holler convenience
                ToolRequest::Ping => payload_builder.set_ping(()),
                _ => {
                    let mut tr_builder = payload_builder.init_tool_request();
                    request_to_capnp_tool_request(&mut tr_builder, tr)?;
                }
            },
            
            // Direct messages
            Payload::TransportPlay => payload_builder.set_transport_play(()),
            Payload::TransportStop => payload_builder.set_transport_stop(()),
            Payload::TransportSeek { position_beats } => payload_builder.init_transport_seek().set_position_beats(*position_beats),
            Payload::TransportStatus => payload_builder.set_transport_status(()),
            Payload::TimelineQuery { from_beats, to_beats } => {
                let mut q = payload_builder.init_timeline_query();
                q.set_from_beats(from_beats.unwrap_or(0.0));
                q.set_to_beats(to_beats.unwrap_or(0.0));
            }
            Payload::TimelineAddMarker { position_beats, marker_type, metadata } => {
                let mut m = payload_builder.init_timeline_add_marker();
                m.set_position_beats(*position_beats);
                m.set_marker_type(marker_type);
                m.set_metadata(serde_json::to_string(metadata).unwrap_or_default());
            }
            Payload::TimelineEvent { event_type, position_beats, tempo, metadata } => {
                let mut e = payload_builder.init_timeline_event();
                e.set_event_type(timeline_event_type_to_capnp(event_type));
                e.set_position_beats(*position_beats);
                e.set_tempo(*tempo);
                e.set_metadata(serde_json::to_string(metadata).unwrap_or_default());
            }
            Payload::StreamStart { uri, definition, chunk_path } => {
                let mut s = payload_builder.init_stream_start();
                s.set_uri(uri);
                set_stream_definition(&mut s.reborrow().init_definition(), definition);
                s.set_chunk_path(chunk_path);
            }
            Payload::StreamSwitchChunk { uri, new_chunk_path } => {
                let mut s = payload_builder.init_stream_switch_chunk();
                s.set_uri(uri);
                s.set_new_chunk_path(new_chunk_path);
            }
            Payload::StreamStop { uri } => payload_builder.init_stream_stop().set_uri(uri),

            Payload::TypedResponse(envelope) => {
                let result = envelope.to_json();
                payload_builder.init_success().set_result(serde_json::to_string(&result).unwrap_or_default());
            }
            Payload::Error { code, message, details } => {
                let mut e = payload_builder.init_error();
                e.set_code(code);
                e.set_message(message);
                if let Some(ref d) = details {
                    e.set_details(serde_json::to_string(d).unwrap_or_default());
                } else {
                    e.set_details("");
                }
            }
            Payload::ToolList { tools } => {
                let mut l = payload_builder.init_tool_list().init_tools(tools.len() as u32);
                for (i, tool) in tools.iter().enumerate() {
                    let mut t = l.reborrow().get(i as u32);
                    t.set_name(&tool.name);
                    t.set_description(&tool.description);
                    t.set_input_schema(serde_json::to_string(&tool.input_schema).unwrap_or_default());
                }
            }
            _ => return Err(capnp::Error::failed("Unimplemented payload for serialization".to_string())),
        }
    }
    Ok(message)
}

fn request_to_capnp_tool_request(builder: &mut tools_capnp::tool_request::Builder, request: &ToolRequest) -> capnp::Result<()> {
    match request {
        ToolRequest::CasStore(req) => { let mut c = builder.reborrow().init_cas_store(); c.set_data(&req.data); c.set_mime_type(&req.mime_type); }
        ToolRequest::CasInspect(req) => builder.reborrow().init_cas_inspect().set_hash(&req.hash),
        ToolRequest::CasGet(req) => builder.reborrow().init_cas_get().set_hash(&req.hash),
        ToolRequest::CasUploadFile(req) => { let mut c = builder.reborrow().init_cas_upload_file(); c.set_file_path(&req.file_path); c.set_mime_type(&req.mime_type); }
        ToolRequest::OrpheusGenerate(req) => {
            let mut o = builder.reborrow().init_orpheus_generate();
            o.set_model(req.model.as_deref().unwrap_or(""));
            o.set_temperature(req.temperature.unwrap_or(1.0));
            o.set_top_p(req.top_p.unwrap_or(0.95));
            o.set_max_tokens(req.max_tokens.unwrap_or(1024));
            o.set_num_variations(req.num_variations.unwrap_or(1));
            set_artifact_metadata(&mut o.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::OrpheusGenerateSeeded(req) => {
            let mut o = builder.reborrow().init_orpheus_generate_seeded();
            o.set_seed_hash(&req.seed_hash);
            o.set_model(req.model.as_deref().unwrap_or(""));
            o.set_temperature(req.temperature.unwrap_or(1.0));
            o.set_top_p(req.top_p.unwrap_or(0.95));
            o.set_max_tokens(req.max_tokens.unwrap_or(1024));
            o.set_num_variations(req.num_variations.unwrap_or(1));
            set_artifact_metadata(&mut o.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::OrpheusContinue(req) => {
            let mut o = builder.reborrow().init_orpheus_continue();
            o.set_input_hash(&req.input_hash);
            o.set_model(req.model.as_deref().unwrap_or(""));
            o.set_temperature(req.temperature.unwrap_or(1.0));
            o.set_top_p(req.top_p.unwrap_or(0.95));
            o.set_max_tokens(req.max_tokens.unwrap_or(1024));
            o.set_num_variations(req.num_variations.unwrap_or(1));
            set_artifact_metadata(&mut o.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::OrpheusBridge(req) => {
            let mut o = builder.reborrow().init_orpheus_bridge();
            o.set_section_a_hash(&req.section_a_hash);
            o.set_section_b_hash(req.section_b_hash.as_deref().unwrap_or(""));
            o.set_model(req.model.as_deref().unwrap_or(""));
            o.set_temperature(req.temperature.unwrap_or(1.0));
            o.set_top_p(req.top_p.unwrap_or(0.95));
            o.set_max_tokens(req.max_tokens.unwrap_or(1024));
            set_artifact_metadata(&mut o.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::OrpheusLoops(req) => {
            let mut o = builder.reborrow().init_orpheus_loops();
            o.set_temperature(req.temperature.unwrap_or(1.0));
            o.set_top_p(req.top_p.unwrap_or(0.95));
            o.set_max_tokens(req.max_tokens.unwrap_or(1024));
            o.set_num_variations(req.num_variations.unwrap_or(1));
            o.set_seed_hash(req.seed_hash.as_deref().unwrap_or(""));
            set_artifact_metadata(&mut o.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::OrpheusClassify(req) => builder.reborrow().init_orpheus_classify().set_midi_hash(&req.midi_hash),
        ToolRequest::AbcParse(req) => builder.reborrow().init_abc_parse().set_abc(&req.abc),
        ToolRequest::AbcValidate(req) => builder.reborrow().init_abc_validate().set_abc(&req.abc),
        ToolRequest::AbcToMidi(req) => {
            let mut a = builder.reborrow().init_abc_to_midi();
            a.set_abc(&req.abc);
            a.set_tempo_override(req.tempo_override.unwrap_or(0));
            a.set_transpose(req.transpose.unwrap_or(0));
            a.set_velocity(req.velocity.unwrap_or(80));
            a.set_channel(req.channel.unwrap_or(0));
            set_artifact_metadata(&mut a.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::AbcTranspose(req) => {
            let mut a = builder.reborrow().init_abc_transpose();
            a.set_abc(&req.abc);
            a.set_semitones(req.semitones.unwrap_or(0));
            a.set_target_key(req.target_key.as_deref().unwrap_or(""));
        }
        ToolRequest::MidiToWav(req) => {
            let mut c = builder.reborrow().init_convert_midi_to_wav();
            c.set_input_hash(&req.input_hash);
            c.set_soundfont_hash(&req.soundfont_hash);
            c.set_sample_rate(req.sample_rate.unwrap_or(44100));
            set_artifact_metadata(&mut c.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::SoundfontInspect(req) => { let mut s = builder.reborrow().init_soundfont_inspect(); s.set_soundfont_hash(&req.soundfont_hash); s.set_include_drum_map(req.include_drum_map); }
        ToolRequest::SoundfontPresetInspect(req) => { let mut s = builder.reborrow().init_soundfont_preset_inspect(); s.set_soundfont_hash(&req.soundfont_hash); s.set_bank(req.bank as i32); s.set_program(req.program as i32); }
        ToolRequest::BeatthisAnalyze(req) => {
            let mut b = builder.reborrow().init_beatthis_analyze();
            b.set_audio_path(req.audio_path.as_deref().unwrap_or(""));
            b.set_audio_hash(req.audio_hash.as_deref().unwrap_or(""));
            b.set_include_frames(req.include_frames);
        }
        ToolRequest::ClapAnalyze(req) => {
            let mut c = builder.reborrow().init_clap_analyze();
            c.set_audio_hash(&req.audio_hash);
            c.set_audio_b_hash(req.audio_b_hash.as_deref().unwrap_or(""));
            c.set_creator(req.creator.as_deref().unwrap_or(""));
            c.set_parent_id(req.parent_id.as_deref().unwrap_or(""));
            {
                let mut t = c.reborrow().init_tasks(req.tasks.len() as u32);
                for (i, v) in req.tasks.iter().enumerate() { t.set(i as u32, v); }
            }
            {
                let mut tc = c.reborrow().init_text_candidates(req.text_candidates.len() as u32);
                for (i, v) in req.text_candidates.iter().enumerate() { tc.set(i as u32, v); }
            }
        }
        ToolRequest::MusicgenGenerate(req) => {
            let mut m = builder.reborrow().init_musicgen_generate();
            m.set_prompt(req.prompt.as_deref().unwrap_or(""));
            m.set_duration(req.duration.unwrap_or(10.0));
            m.set_temperature(req.temperature.unwrap_or(1.0));
            m.set_top_k(req.top_k.unwrap_or(250));
            m.set_top_p(req.top_p.unwrap_or(0.9));
            m.set_guidance_scale(req.guidance_scale.unwrap_or(3.0));
            m.set_do_sample(req.do_sample.unwrap_or(true));
            set_artifact_metadata(&mut m.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::YueGenerate(req) => {
            let mut y = builder.reborrow().init_yue_generate();
            y.set_lyrics(&req.lyrics);
            y.set_genre(req.genre.as_deref().unwrap_or("Pop"));
            y.set_max_new_tokens(req.max_new_tokens.unwrap_or(3000));
            y.set_run_n_segments(req.run_n_segments.unwrap_or(2));
            y.set_seed(req.seed.unwrap_or(42));
            set_artifact_metadata(&mut y.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::ArtifactUpload(req) => {
            let mut a = builder.reborrow().init_artifact_upload();
            a.set_file_path(&req.file_path);
            a.set_mime_type(&req.mime_type);
            set_artifact_metadata(&mut a.init_metadata(), &req.variation_set_id, &req.parent_id, &req.tags, &req.creator);
        }
        ToolRequest::ArtifactGet(req) => builder.reborrow().init_artifact_get().set_id(&req.id),
        ToolRequest::ArtifactList(req) => {
            let mut a = builder.reborrow().init_artifact_list();
            a.set_tag(req.tag.as_deref().unwrap_or(""));
            a.set_creator(req.creator.as_deref().unwrap_or(""));
        }
        ToolRequest::ArtifactCreate(req) => {
            let mut a = builder.reborrow().init_artifact_create();
            a.set_cas_hash(&req.cas_hash);
            a.set_creator(req.creator.as_deref().unwrap_or(""));
            a.set_metadata(serde_json::to_string(&req.metadata).unwrap_or_default());
            {
                let mut t = a.reborrow().init_tags(req.tags.len() as u32);
                for (i, v) in req.tags.iter().enumerate() { t.set(i as u32, v); }
            }
        }
        ToolRequest::GraphQuery(req) => {
            let mut q = builder.reborrow().init_graph_query();
            q.set_query(&req.query);
            q.set_limit(req.limit.unwrap_or(100) as u32);
            q.set_variables(serde_json::to_string(&req.variables).unwrap_or_default());
        }
        ToolRequest::GraphBind(req) => {
            let mut b = builder.reborrow().init_graph_bind();
            b.set_id(&req.id);
            b.set_name(&req.name);
            let mut h = b.init_hints(req.hints.len() as u32);
            for (i, hint) in req.hints.iter().enumerate() {
                let mut hi = h.reborrow().get(i as u32);
                hi.set_kind(&hint.kind);
                hi.set_value(&hint.value);
                hi.set_confidence(hint.confidence);
            }
        }
        ToolRequest::GraphTag(req) => {
            let mut t = builder.reborrow().init_graph_tag();
            t.set_identity_id(&req.identity_id);
            t.set_namespace(&req.namespace);
            t.set_value(&req.value);
        }
        ToolRequest::GraphConnect(req) => {
            let mut c = builder.reborrow().init_graph_connect();
            c.set_from_identity(&req.from_identity);
            c.set_from_port(&req.from_port);
            c.set_to_identity(&req.to_identity);
            c.set_to_port(&req.to_port);
            c.set_transport(req.transport.as_deref().unwrap_or(""));
        }
        ToolRequest::GraphFind(req) => {
            let mut f = builder.reborrow().init_graph_find();
            f.set_name(req.name.as_deref().unwrap_or(""));
            f.set_tag_namespace(req.tag_namespace.as_deref().unwrap_or(""));
            f.set_tag_value(req.tag_value.as_deref().unwrap_or(""));
        }
        ToolRequest::GraphContext(req) => {
            let mut c = builder.reborrow().init_graph_context();
            c.set_tag(req.tag.as_deref().unwrap_or(""));
            c.set_creator(req.creator.as_deref().unwrap_or(""));
            c.set_vibe_search(req.vibe_search.as_deref().unwrap_or(""));
            c.set_limit(req.limit.unwrap_or(20) as u32);
            c.set_include_metadata(req.include_metadata);
            c.set_include_annotations(req.include_annotations);
        }
        ToolRequest::AddAnnotation(req) => {
            let mut a = builder.reborrow().init_add_annotation();
            a.set_artifact_id(&req.artifact_id);
            a.set_message(&req.message);
            a.set_vibe(req.vibe.as_deref().unwrap_or(""));
            a.set_source(req.source.as_deref().unwrap_or(""));
        }
        ToolRequest::ConfigGet(req) => {
            let mut c = builder.reborrow().init_config_get();
            c.set_section(req.section.as_deref().unwrap_or(""));
            c.set_key(req.key.as_deref().unwrap_or(""));
        }
        ToolRequest::JobStatus(req) => builder.reborrow().init_job_status().set_job_id(&req.job_id),
        ToolRequest::JobPoll(req) => {
            let mut j = builder.reborrow().init_job_poll();
            j.set_timeout_ms(req.timeout_ms);
            let mut ids = j.reborrow().init_job_ids(req.job_ids.len() as u32);
            for (i, id) in req.job_ids.iter().enumerate() { ids.set(i as u32, id); }
            let mode = match req.mode.as_deref() {
                Some("all") => common_capnp::PollMode::All,
                _ => common_capnp::PollMode::Any,
            };
            j.set_mode(mode);
        }
        ToolRequest::JobCancel(req) => builder.reborrow().init_job_cancel().set_job_id(&req.job_id),
        ToolRequest::JobList(req) => builder.reborrow().init_job_list().set_status(req.status.as_deref().unwrap_or("")),
        ToolRequest::JobSleep(req) => builder.reborrow().init_job_sleep().set_milliseconds(req.milliseconds),
        ToolRequest::ReadResource(req) => builder.reborrow().init_read_resource().set_uri(&req.uri),
        ToolRequest::ListResources => builder.reborrow().set_list_resources(()),
        ToolRequest::Complete(req) => { let mut c = builder.reborrow().init_complete(); c.set_context(&req.context); c.set_partial(&req.partial); }
        ToolRequest::SampleLlm(req) => {
            let mut l = builder.reborrow().init_sample_llm();
            l.set_prompt(&req.prompt);
            l.set_max_tokens(req.max_tokens.unwrap_or(1024));
            l.set_temperature(req.temperature.unwrap_or(1.0));
            l.set_system_prompt(req.system_prompt.as_deref().unwrap_or(""));
        }
        ToolRequest::ListTools => builder.reborrow().set_list_tools(()),
        ToolRequest::WeaveEval(req) => builder.reborrow().init_weave_eval().set_code(&req.code),
        ToolRequest::WeaveSession => builder.reborrow().set_weave_session(()),
        ToolRequest::WeaveReset(req) => builder.reborrow().init_weave_reset().set_clear_session(req.clear_session),
        ToolRequest::WeaveHelp(req) => builder.reborrow().init_weave_help().set_topic(req.topic.as_deref().unwrap_or("")),
        
        // === New Modernized Mappings ===
        ToolRequest::GardenStatus => builder.reborrow().set_garden_status(()),
        ToolRequest::GardenPlay => builder.reborrow().set_garden_play(()),
        ToolRequest::GardenPause => builder.reborrow().set_garden_pause(()),
        ToolRequest::GardenStop => builder.reborrow().set_garden_stop(()),
        ToolRequest::GardenSeek(req) => builder.reborrow().init_garden_seek().set_beat(req.beat),
        ToolRequest::GardenSetTempo(req) => builder.reborrow().init_garden_set_tempo().set_bpm(req.bpm),
        ToolRequest::GardenQuery(req) => {
            let mut q = builder.reborrow().init_garden_query();
            q.set_query(&req.query);
            q.set_variables(serde_json::to_string(&req.variables).unwrap_or_default());
        }
        ToolRequest::GardenEmergencyPause => builder.reborrow().set_garden_emergency_pause(()),
        ToolRequest::GardenCreateRegion(req) => {
            let mut r = builder.reborrow().init_garden_create_region();
            r.set_position(req.position);
            r.set_duration(req.duration);
            r.set_behavior_type(&req.behavior_type);
            r.set_content_id(&req.content_id);
        }
        ToolRequest::GardenDeleteRegion(req) => builder.reborrow().init_garden_delete_region().set_region_id(&req.region_id),
        ToolRequest::GardenMoveRegion(req) => {
            let mut r = builder.reborrow().init_garden_move_region();
            r.set_region_id(&req.region_id);
            r.set_new_position(req.new_position);
        }
        ToolRequest::GardenGetRegions(req) => {
            let mut r = builder.reborrow().init_garden_get_regions();
            r.set_start(req.start.unwrap_or(0.0));
            r.set_end(req.end.unwrap_or(0.0));
        }
        ToolRequest::GardenAttachAudio(req) => {
            let mut a = builder.reborrow().init_garden_attach_audio();
            a.set_device_name(req.device_name.as_deref().unwrap_or(""));
            a.set_sample_rate(req.sample_rate.unwrap_or(0));
            a.set_latency_frames(req.latency_frames.unwrap_or(0));
        }
        ToolRequest::GardenDetachAudio => builder.reborrow().set_garden_detach_audio(()),
        ToolRequest::GardenAudioStatus => builder.reborrow().set_garden_audio_status(()),
        ToolRequest::GardenAttachInput(req) => {
            let mut a = builder.reborrow().init_garden_attach_input();
            a.set_device_name(req.device_name.as_deref().unwrap_or(""));
            a.set_sample_rate(req.sample_rate.unwrap_or(0));
        }
        ToolRequest::GardenDetachInput => builder.reborrow().set_garden_detach_input(()),
        ToolRequest::GardenInputStatus => builder.reborrow().set_garden_input_status(()),
        ToolRequest::GardenSetMonitor(req) => {
            let mut s = builder.reborrow().init_garden_set_monitor();
            s.set_enabled(req.enabled.unwrap_or(false));
            s.set_enabled_set(req.enabled.is_some());
            s.set_gain(req.gain.unwrap_or(1.0));
            s.set_gain_set(req.gain.is_some());
        }
        ToolRequest::GetToolHelp(req) => builder.reborrow().init_get_tool_help().set_topic(req.topic.as_deref().unwrap_or("")),
        ToolRequest::Schedule(req) => {
            let mut s = builder.reborrow().init_schedule();
            encoding_to_capnp(s.reborrow().init_encoding(), &req.encoding);
            s.set_at(req.at);
            s.set_duration(req.duration.unwrap_or(0.0));
            s.set_gain(req.gain.unwrap_or(1.0));
            s.set_rate(req.rate.unwrap_or(1.0));
        }
        ToolRequest::Analyze(req) => {
            let mut a = builder.reborrow().init_analyze();
            encoding_to_capnp(a.reborrow().init_encoding(), &req.encoding);
            let mut t = a.init_tasks(req.tasks.len() as u32);
            for (i, task) in req.tasks.iter().enumerate() {
                t.set(i as u32, analysis_task_to_capnp(task));
            }
        }
        
        ToolRequest::Ping => {
            // Should be handled by payload_to_capnp_payload
            return Err(capnp::Error::failed("ToolRequest::Ping passed to tool_request builder (should use envelope)".to_string()));
        }
    }
    Ok(())
}

fn capnp_tool_request_to_request(reader: tools_capnp::tool_request::Reader) -> capnp::Result<ToolRequest> {
    match reader.which()? {
        // ... (standard tools unchanged)
        tools_capnp::tool_request::CasStore(cas) => {
            let cas = cas?;
            Ok(ToolRequest::CasStore(CasStoreRequest {
                data: cas.get_data()?.to_vec(),
                mime_type: cas.get_mime_type()?.to_str()?.to_string(),
            }))
        }
        tools_capnp::tool_request::CasInspect(cas) => Ok(ToolRequest::CasInspect(CasInspectRequest { hash: cas?.get_hash()?.to_str()?.to_string() })),
        tools_capnp::tool_request::CasGet(cas) => Ok(ToolRequest::CasGet(CasGetRequest { hash: cas?.get_hash()?.to_str()?.to_string() })),
        tools_capnp::tool_request::CasUploadFile(cas) => {
            let cas = cas?;
            Ok(ToolRequest::CasUploadFile(CasUploadFileRequest { file_path: cas.get_file_path()?.to_str()?.to_string(), mime_type: cas.get_mime_type()?.to_str()?.to_string() }))
        }
        tools_capnp::tool_request::OrpheusGenerate(o) => {
            let o = o?; let m = o.get_metadata()?;
            Ok(ToolRequest::OrpheusGenerate(OrpheusGenerateRequest {
                model: Some(o.get_model()?.to_str()?.to_string()),
                temperature: Some(o.get_temperature()),
                top_p: Some(o.get_top_p()),
                max_tokens: Some(o.get_max_tokens()),
                num_variations: Some(o.get_num_variations()),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::OrpheusGenerateSeeded(o) => {
            let o = o?; let m = o.get_metadata()?;
            Ok(ToolRequest::OrpheusGenerateSeeded(OrpheusGenerateSeededRequest {
                seed_hash: o.get_seed_hash()?.to_str()?.to_string(),
                model: Some(o.get_model()?.to_str()?.to_string()),
                temperature: Some(o.get_temperature()),
                top_p: Some(o.get_top_p()),
                max_tokens: Some(o.get_max_tokens()),
                num_variations: Some(o.get_num_variations()),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::OrpheusContinue(o) => {
            let o = o?; let m = o.get_metadata()?;
            Ok(ToolRequest::OrpheusContinue(OrpheusContinueRequest {
                input_hash: o.get_input_hash()?.to_str()?.to_string(),
                model: Some(o.get_model()?.to_str()?.to_string()),
                temperature: Some(o.get_temperature()),
                top_p: Some(o.get_top_p()),
                max_tokens: Some(o.get_max_tokens()),
                num_variations: Some(o.get_num_variations()),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::OrpheusBridge(o) => {
            let o = o?; let m = o.get_metadata()?;
            Ok(ToolRequest::OrpheusBridge(OrpheusBridgeRequest {
                section_a_hash: o.get_section_a_hash()?.to_str()?.to_string(),
                section_b_hash: capnp_optional_string(o.get_section_b_hash()?),
                model: Some(o.get_model()?.to_str()?.to_string()),
                temperature: Some(o.get_temperature()),
                top_p: Some(o.get_top_p()),
                max_tokens: Some(o.get_max_tokens()),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::OrpheusLoops(o) => {
            let o = o?; let m = o.get_metadata()?;
            Ok(ToolRequest::OrpheusLoops(OrpheusLoopsRequest {
                temperature: Some(o.get_temperature()),
                top_p: Some(o.get_top_p()),
                max_tokens: Some(o.get_max_tokens()),
                num_variations: Some(o.get_num_variations()),
                seed_hash: capnp_optional_string(o.get_seed_hash()?),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::OrpheusClassify(o) => { let o = o?; Ok(ToolRequest::OrpheusClassify(OrpheusClassifyRequest { midi_hash: o.get_midi_hash()?.to_str()?.to_string() })) }
        tools_capnp::tool_request::AbcParse(a) => { let a = a?; Ok(ToolRequest::AbcParse(AbcParseRequest { abc: a.get_abc()?.to_str()?.to_string() })) }
        tools_capnp::tool_request::AbcValidate(a) => { let a = a?; Ok(ToolRequest::AbcValidate(AbcValidateRequest { abc: a.get_abc()?.to_str()?.to_string() })) }
        tools_capnp::tool_request::AbcToMidi(a) => {
            let a = a?; let m = a.get_metadata()?;
            Ok(ToolRequest::AbcToMidi(AbcToMidiRequest {
                abc: a.get_abc()?.to_str()?.to_string(),
                tempo_override: Some(a.get_tempo_override()),
                transpose: Some(a.get_transpose()),
                velocity: Some(a.get_velocity()),
                channel: Some(a.get_channel()),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::AbcTranspose(a) => {
            let a = a?;
            Ok(ToolRequest::AbcTranspose(AbcTransposeRequest {
                abc: a.get_abc()?.to_str()?.to_string(),
                semitones: Some(a.get_semitones()),
                target_key: capnp_optional_string(a.get_target_key()?),
            }))
        }
        tools_capnp::tool_request::ConvertMidiToWav(c) => {
            let c = c?; let m = c.get_metadata()?;
            Ok(ToolRequest::MidiToWav(MidiToWavRequest {
                input_hash: c.get_input_hash()?.to_str()?.to_string(),
                soundfont_hash: c.get_soundfont_hash()?.to_str()?.to_string(),
                sample_rate: Some(c.get_sample_rate()),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::SoundfontInspect(s) => {
            let s = s?;
            Ok(ToolRequest::SoundfontInspect(SoundfontInspectRequest {
                soundfont_hash: s.get_soundfont_hash()?.to_str()?.to_string(),
                include_drum_map: s.get_include_drum_map(),
            }))
        }
        tools_capnp::tool_request::SoundfontPresetInspect(s) => {
            let s = s?;
            Ok(ToolRequest::SoundfontPresetInspect(SoundfontPresetInspectRequest {
                soundfont_hash: s.get_soundfont_hash()?.to_str()?.to_string(),
                bank: s.get_bank() as u16,
                program: s.get_program() as u16,
            }))
        }
        tools_capnp::tool_request::BeatthisAnalyze(b) => {
            let b = b?;
            Ok(ToolRequest::BeatthisAnalyze(BeatthisAnalyzeRequest {
                audio_path: capnp_optional_string(b.get_audio_path()?),
                audio_hash: capnp_optional_string(b.get_audio_hash()?),
                include_frames: b.get_include_frames(),
            }))
        }
        tools_capnp::tool_request::ClapAnalyze(c) => {
            let c = c?;
            Ok(ToolRequest::ClapAnalyze(ClapAnalyzeRequest {
                audio_hash: c.get_audio_hash()?.to_str()?.to_string(),
                tasks: capnp_string_list(c.get_tasks()?),
                audio_b_hash: capnp_optional_string(c.get_audio_b_hash()?),
                text_candidates: capnp_string_list(c.get_text_candidates()?),
                parent_id: capnp_optional_string(c.get_parent_id()?),
                creator: capnp_optional_string(c.get_creator()?),
            }))
        }
        tools_capnp::tool_request::MusicgenGenerate(m) => {
            let g = m?; let meta = g.get_metadata()?;
            Ok(ToolRequest::MusicgenGenerate(MusicgenGenerateRequest {
                prompt: capnp_optional_string(g.get_prompt()?),
                duration: Some(g.get_duration()),
                temperature: Some(g.get_temperature()),
                top_k: Some(g.get_top_k()),
                top_p: Some(g.get_top_p()),
                guidance_scale: Some(g.get_guidance_scale()),
                do_sample: Some(g.get_do_sample()),
                tags: capnp_string_list(meta.get_tags()?),
                creator: Some(meta.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(meta.get_parent_id()?),
                variation_set_id: capnp_optional_string(meta.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::YueGenerate(y) => {
            let y = y?; let m = y.get_metadata()?;
            Ok(ToolRequest::YueGenerate(YueGenerateRequest {
                lyrics: y.get_lyrics()?.to_str()?.to_string(),
                genre: capnp_optional_string(y.get_genre()?),
                max_new_tokens: Some(y.get_max_new_tokens()),
                run_n_segments: Some(y.get_run_n_segments()),
                seed: Some(y.get_seed()),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::ArtifactUpload(a) => {
            let a = a?; let m = a.get_metadata()?;
            Ok(ToolRequest::ArtifactUpload(ArtifactUploadRequest {
                file_path: a.get_file_path()?.to_str()?.to_string(),
                mime_type: a.get_mime_type()?.to_str()?.to_string(),
                tags: capnp_string_list(m.get_tags()?),
                creator: Some(m.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(m.get_parent_id()?),
                variation_set_id: capnp_optional_string(m.get_variation_set_id()?),
            }))
        }
        tools_capnp::tool_request::ArtifactGet(a) => { let a = a?; Ok(ToolRequest::ArtifactGet(ArtifactGetRequest { id: a.get_id()?.to_str()?.to_string() })) }
        tools_capnp::tool_request::ArtifactList(a) => {
            let a = a?;
            Ok(ToolRequest::ArtifactList(ArtifactListRequest {
                tag: capnp_optional_string(a.get_tag()?),
                creator: capnp_optional_string(a.get_creator()?),
                limit: None,
            }))
        }
        tools_capnp::tool_request::ArtifactCreate(a) => {
            let a = a?;
            let metadata = serde_json::from_str(a.get_metadata()?.to_str()?).unwrap_or_default();
            Ok(ToolRequest::ArtifactCreate(crate::request::ArtifactCreateRequest {
                cas_hash: a.get_cas_hash()?.to_str()?.to_string(),
                tags: capnp_string_list(a.get_tags()?),
                creator: capnp_optional_string(a.get_creator()?),
                metadata,
            }))
        }
        tools_capnp::tool_request::GraphQuery(q) => {
            let q = q?;
            Ok(ToolRequest::GraphQuery(GraphQueryRequest {
                query: q.get_query()?.to_str()?.to_string(),
                variables: serde_json::from_str(q.get_variables()?.to_str()?).ok(),
                limit: Some(q.get_limit() as usize),
            }))
        }
        tools_capnp::tool_request::GraphBind(b) => {
            let b = b?;
            let mut hints = Vec::new();
            let hr = b.get_hints()?;
            for i in 0..hr.len() {
                let h = hr.get(i);
                hints.push(crate::request::GraphHint {
                    kind: h.get_kind()?.to_str()?.to_string(),
                    value: h.get_value()?.to_str()?.to_string(),
                    confidence: h.get_confidence(),
                });
            }
            Ok(ToolRequest::GraphBind(GraphBindRequest {
                id: b.get_id()?.to_str()?.to_string(),
                name: b.get_name()?.to_str()?.to_string(),
                hints,
            }))
        }
        tools_capnp::tool_request::GraphTag(t) => {
            let t = t?;
            Ok(ToolRequest::GraphTag(GraphTagRequest {
                identity_id: t.get_identity_id()?.to_str()?.to_string(),
                namespace: t.get_namespace()?.to_str()?.to_string(),
                value: t.get_value()?.to_str()?.to_string(),
            }))
        }
        tools_capnp::tool_request::GraphConnect(c) => {
            let c = c?;
            Ok(ToolRequest::GraphConnect(GraphConnectRequest {
                from_identity: c.get_from_identity()?.to_str()?.to_string(),
                from_port: c.get_from_port()?.to_str()?.to_string(),
                to_identity: c.get_to_identity()?.to_str()?.to_string(),
                to_port: c.get_to_port()?.to_str()?.to_string(),
                transport: capnp_optional_string(c.get_transport()?),
            }))
        }
        tools_capnp::tool_request::GraphFind(f) => {
            let f = f?;
            Ok(ToolRequest::GraphFind(GraphFindRequest {
                name: capnp_optional_string(f.get_name()?),
                tag_namespace: capnp_optional_string(f.get_tag_namespace()?),
                tag_value: capnp_optional_string(f.get_tag_value()?),
            }))
        }
        tools_capnp::tool_request::GraphContext(c) => {
            let c = c?;
            Ok(ToolRequest::GraphContext(GraphContextRequest {
                tag: capnp_optional_string(c.get_tag()?),
                vibe_search: capnp_optional_string(c.get_vibe_search()?),
                creator: capnp_optional_string(c.get_creator()?),
                limit: Some(c.get_limit() as usize),
                include_metadata: c.get_include_metadata(),
                include_annotations: c.get_include_annotations(),
                within_minutes: None,
            }))
        }
        tools_capnp::tool_request::AddAnnotation(a) => {
            let a = a?;
            Ok(ToolRequest::AddAnnotation(AddAnnotationRequest {
                artifact_id: a.get_artifact_id()?.to_str()?.to_string(),
                message: a.get_message()?.to_str()?.to_string(),
                vibe: capnp_optional_string(a.get_vibe()?),
                source: capnp_optional_string(a.get_source()?),
            }))
        }
        tools_capnp::tool_request::ConfigGet(c) => {
            let c = c?;
            Ok(ToolRequest::ConfigGet(ConfigGetRequest {
                section: capnp_optional_string(c.get_section()?),
                key: capnp_optional_string(c.get_key()?),
            }))
        }
        tools_capnp::tool_request::JobStatus(j) => { let j = j?; Ok(ToolRequest::JobStatus(JobStatusRequest { job_id: j.get_job_id()?.to_str()?.to_string() })) }
        tools_capnp::tool_request::JobPoll(j) => {
            let j = j?;
            let mode = match j.get_mode()? { crate::common_capnp::PollMode::Any => "any", crate::common_capnp::PollMode::All => "all" };
            Ok(ToolRequest::JobPoll(JobPollRequest {
                job_ids: capnp_string_list(j.get_job_ids()?),
                timeout_ms: j.get_timeout_ms(),
                mode: Some(mode.to_string()),
            }))
        }
        tools_capnp::tool_request::JobCancel(j) => { let j = j?; Ok(ToolRequest::JobCancel(JobCancelRequest { job_id: j.get_job_id()?.to_str()?.to_string() })) }
        tools_capnp::tool_request::JobList(j) => { let j = j?; Ok(ToolRequest::JobList(JobListRequest { status: capnp_optional_string(j.get_status()?) })) }
        tools_capnp::tool_request::JobSleep(j) => { let j = j?; Ok(ToolRequest::JobSleep(JobSleepRequest { milliseconds: j.get_milliseconds() })) }
        tools_capnp::tool_request::ReadResource(r) => { let r = r?; Ok(ToolRequest::ReadResource(ReadResourceRequest { uri: r.get_uri()?.to_str()?.to_string() })) }
        tools_capnp::tool_request::ListResources(()) => Ok(ToolRequest::ListResources),
        tools_capnp::tool_request::Complete(c) => {
            let c = c?;
            Ok(ToolRequest::Complete(CompleteRequest {
                context: c.get_context()?.to_str()?.to_string(),
                partial: c.get_partial()?.to_str()?.to_string(),
            }))
        }
        tools_capnp::tool_request::SampleLlm(l) => {
            let l = l?;
            Ok(ToolRequest::SampleLlm(SampleLlmRequest {
                prompt: l.get_prompt()?.to_str()?.to_string(),
                max_tokens: Some(l.get_max_tokens()),
                temperature: Some(l.get_temperature()),
                system_prompt: capnp_optional_string(l.get_system_prompt()?),
            }))
        }
        tools_capnp::tool_request::ListTools(()) => Ok(ToolRequest::ListTools),
        tools_capnp::tool_request::WeaveEval(w) => { let w = w?; Ok(ToolRequest::WeaveEval(WeaveEvalRequest { code: w.get_code()?.to_str()?.to_string() })) }
        tools_capnp::tool_request::WeaveSession(()) => Ok(ToolRequest::WeaveSession),
        tools_capnp::tool_request::WeaveReset(w) => { let w = w?; Ok(ToolRequest::WeaveReset(WeaveResetRequest { clear_session: w.get_clear_session() })) }
        tools_capnp::tool_request::WeaveHelp(w) => { let w = w?; Ok(ToolRequest::WeaveHelp(WeaveHelpRequest { topic: capnp_optional_string(w.get_topic()?) })) }
        
        // === New Modernized Mappings ===
        tools_capnp::tool_request::GardenStatus(()) => Ok(ToolRequest::GardenStatus),
        tools_capnp::tool_request::GardenPlay(()) => Ok(ToolRequest::GardenPlay),
        tools_capnp::tool_request::GardenPause(()) => Ok(ToolRequest::GardenPause),
        tools_capnp::tool_request::GardenStop(()) => Ok(ToolRequest::GardenStop),
        tools_capnp::tool_request::GardenSeek(s) => Ok(ToolRequest::GardenSeek(GardenSeekRequest { beat: s?.get_beat() })),
        tools_capnp::tool_request::GardenSetTempo(t) => Ok(ToolRequest::GardenSetTempo(GardenSetTempoRequest { bpm: t?.get_bpm() })),
        tools_capnp::tool_request::GardenQuery(q) => {
            let q = q?;
            Ok(ToolRequest::GardenQuery(GardenQueryRequest {
                query: q.get_query()?.to_str()?.to_string(),
                variables: serde_json::from_str(q.get_variables()?.to_str()?).ok(),
            }))
        }
        tools_capnp::tool_request::GardenEmergencyPause(()) => Ok(ToolRequest::GardenEmergencyPause),
        tools_capnp::tool_request::GardenCreateRegion(r) => {
            let r = r?;
            Ok(ToolRequest::GardenCreateRegion(GardenCreateRegionRequest {
                position: r.get_position(),
                duration: r.get_duration(),
                behavior_type: r.get_behavior_type()?.to_str()?.to_string(),
                content_id: r.get_content_id()?.to_str()?.to_string(),
            }))
        }
        tools_capnp::tool_request::GardenDeleteRegion(r) => Ok(ToolRequest::GardenDeleteRegion(GardenDeleteRegionRequest { region_id: r?.get_region_id()?.to_str()?.to_string() })),
        tools_capnp::tool_request::GardenMoveRegion(r) => {
            let r = r?;
            Ok(ToolRequest::GardenMoveRegion(GardenMoveRegionRequest {
                region_id: r.get_region_id()?.to_str()?.to_string(),
                new_position: r.get_new_position(),
            }))
        }
        tools_capnp::tool_request::GardenGetRegions(r) => {
            let r = r?;
            Ok(ToolRequest::GardenGetRegions(GardenGetRegionsRequest {
                start: if r.get_start() == 0.0 { None } else { Some(r.get_start()) },
                end: if r.get_end() == 0.0 { None } else { Some(r.get_end()) },
            }))
        }
        tools_capnp::tool_request::GardenAttachAudio(a) => {
            let a = a?;
            Ok(ToolRequest::GardenAttachAudio(GardenAttachAudioRequest {
                device_name: capnp_optional_string(a.get_device_name()?),
                sample_rate: if a.get_sample_rate() == 0 { None } else { Some(a.get_sample_rate()) },
                latency_frames: if a.get_latency_frames() == 0 { None } else { Some(a.get_latency_frames()) },
            }))
        }
        tools_capnp::tool_request::GardenDetachAudio(()) => Ok(ToolRequest::GardenDetachAudio),
        tools_capnp::tool_request::GardenAudioStatus(()) => Ok(ToolRequest::GardenAudioStatus),
        tools_capnp::tool_request::GardenAttachInput(a) => {
            let a = a?;
            Ok(ToolRequest::GardenAttachInput(GardenAttachInputRequest {
                device_name: capnp_optional_string(a.get_device_name()?),
                sample_rate: if a.get_sample_rate() == 0 { None } else { Some(a.get_sample_rate()) },
            }))
        }
        tools_capnp::tool_request::GardenDetachInput(()) => Ok(ToolRequest::GardenDetachInput),
        tools_capnp::tool_request::GardenInputStatus(()) => Ok(ToolRequest::GardenInputStatus),
        tools_capnp::tool_request::GardenSetMonitor(s) => {
            let s = s?;
            Ok(ToolRequest::GardenSetMonitor(GardenSetMonitorRequest {
                enabled: if s.get_enabled_set() { Some(s.get_enabled()) } else { None },
                gain: if s.get_gain_set() { Some(s.get_gain()) } else { None },
            }))
        }
        tools_capnp::tool_request::GetToolHelp(h) => Ok(ToolRequest::GetToolHelp(GetToolHelpRequest { topic: capnp_optional_string(h?.get_topic()?) })),
        tools_capnp::tool_request::Schedule(s) => {
            let s = s?;
            Ok(ToolRequest::Schedule(ScheduleRequest {
                encoding: capnp_to_encoding(s.get_encoding()?)?,
                at: s.get_at(),
                duration: if s.get_duration() == 0.0 { None } else { Some(s.get_duration()) },
                gain: if s.get_gain() == 1.0 { None } else { Some(s.get_gain()) },
                rate: if s.get_rate() == 1.0 { None } else { Some(s.get_rate()) },
            }))
        }
        tools_capnp::tool_request::Analyze(a) => {
            let a = a?;
            let mut tasks = Vec::new();
            let tr = a.get_tasks()?;
            for i in 0..tr.len() {
                tasks.push(capnp_to_analysis_task(tr.get(i)?));
            }
            Ok(ToolRequest::Analyze(AnalyzeRequest {
                encoding: capnp_to_encoding(a.get_encoding()?)?,
                tasks,
            }))
        }

        _ => Err(capnp::Error::failed("Unsupported ToolRequest variant".to_string())),
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

/// Helper: Convert capnp Encoding to Rust Encoding
fn capnp_to_encoding(reader: common_capnp::encoding::Reader) -> capnp::Result<Encoding> {
    match reader.which()? {
        common_capnp::encoding::Midi(id) => Ok(Encoding::Midi { artifact_id: id?.to_str()?.to_string() }),
        common_capnp::encoding::Audio(id) => Ok(Encoding::Audio { artifact_id: id?.to_str()?.to_string() }),
        common_capnp::encoding::Abc(abc) => Ok(Encoding::Abc { notation: abc?.to_str()?.to_string() }),
        common_capnp::encoding::Hash(hash) => {
            let h = hash;
            Ok(Encoding::Hash {
                content_hash: h.get_content_hash()?.to_str()?.to_string(),
                format: h.get_format()?.to_str()?.to_string(),
            })
        }
    }
}

/// Helper: Convert Rust Encoding to capnp Encoding
fn encoding_to_capnp(mut builder: common_capnp::encoding::Builder, encoding: &Encoding) {
    match encoding {
        Encoding::Midi { artifact_id } => builder.set_midi(artifact_id),
        Encoding::Audio { artifact_id } => builder.set_audio(artifact_id),
        Encoding::Abc { notation } => builder.set_abc(notation),
        Encoding::Hash { content_hash, format } => {
            let mut h = builder.init_hash();
            h.set_content_hash(content_hash);
            h.set_format(format);
        }
    }
}

/// Helper: Convert capnp AnalysisTask to Rust AnalysisTask
fn capnp_to_analysis_task(task: common_capnp::AnalysisTask) -> AnalysisTask {
    match task {
        common_capnp::AnalysisTask::Classify => AnalysisTask::Classify,
        common_capnp::AnalysisTask::Beats => AnalysisTask::Beats,
        common_capnp::AnalysisTask::Embeddings => AnalysisTask::Embeddings,
        common_capnp::AnalysisTask::Genre => AnalysisTask::Genre,
        common_capnp::AnalysisTask::Mood => AnalysisTask::Mood,
    }
}

/// Helper: Convert Rust AnalysisTask to capnp AnalysisTask
fn analysis_task_to_capnp(task: &AnalysisTask) -> common_capnp::AnalysisTask {
    match task {
        AnalysisTask::Classify => common_capnp::AnalysisTask::Classify,
        AnalysisTask::Beats => common_capnp::AnalysisTask::Beats,
        AnalysisTask::Embeddings => common_capnp::AnalysisTask::Embeddings,
        AnalysisTask::Genre => common_capnp::AnalysisTask::Genre,
        AnalysisTask::Mood => common_capnp::AnalysisTask::Mood,
    }
}