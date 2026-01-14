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
use crate::{common_capnp, envelope_capnp, responses_capnp, streams_capnp, tools_capnp};

use crate::envelope::ResponseEnvelope;
use crate::request::*;
use crate::responses::*;
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

        envelope_capnp::payload::ToolResponse(resp) => {
            let response = capnp_tool_response_to_response(resp?)?;
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
                match envelope {
                    ResponseEnvelope::Success { response } => {
                        let mut resp_builder = payload_builder.init_tool_response();
                        response_to_capnp_tool_response(&mut resp_builder, response)?;
                    }
                    ResponseEnvelope::Error(err) => {
                        let mut e = payload_builder.init_error();
                        e.set_code(err.code());
                        e.set_message(&err.message());
                        e.set_details("");
                    }
                    ResponseEnvelope::JobStarted { job_id, tool, .. } => {
                        let resp_builder = payload_builder.init_tool_response();
                        let mut js = resp_builder.init_job_started();
                        js.set_job_id(job_id);
                        js.set_tool(tool);
                    }
                    ResponseEnvelope::Ack { message } => {
                        let resp_builder = payload_builder.init_tool_response();
                        resp_builder.init_ack().set_message(message);
                    }
                }
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
        ToolRequest::CasStats => builder.reborrow().set_cas_stats(()),
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
        ToolRequest::MidiInfo(req) => {
            let mut m = builder.reborrow().init_midi_info();
            m.set_artifact_id(req.artifact_id.as_deref().unwrap_or(""));
            m.set_hash(req.hash.as_deref().unwrap_or(""));
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
        // RAVE tools
        ToolRequest::RaveEncode(req) => {
            let mut r = builder.reborrow().init_rave_encode();
            r.set_audio_hash(&req.audio_hash);
            r.set_model(req.model.as_deref().unwrap_or(""));
            set_artifact_metadata(&mut r.init_metadata(), &None, &None, &req.tags, &req.creator);
        }
        ToolRequest::RaveDecode(req) => {
            let mut r = builder.reborrow().init_rave_decode();
            r.set_latent_hash(&req.latent_hash);
            {
                let mut shape = r.reborrow().init_latent_shape(req.latent_shape.len() as u32);
                for (i, &v) in req.latent_shape.iter().enumerate() { shape.set(i as u32, v); }
            }
            r.set_model(req.model.as_deref().unwrap_or(""));
            set_artifact_metadata(&mut r.init_metadata(), &None, &None, &req.tags, &req.creator);
        }
        ToolRequest::RaveReconstruct(req) => {
            let mut r = builder.reborrow().init_rave_reconstruct();
            r.set_audio_hash(&req.audio_hash);
            r.set_model(req.model.as_deref().unwrap_or(""));
            set_artifact_metadata(&mut r.init_metadata(), &None, &None, &req.tags, &req.creator);
        }
        ToolRequest::RaveGenerate(req) => {
            let mut r = builder.reborrow().init_rave_generate();
            r.set_model(req.model.as_deref().unwrap_or(""));
            r.set_duration_seconds(req.duration_seconds.unwrap_or(4.0));
            r.set_temperature(req.temperature.unwrap_or(1.0));
            set_artifact_metadata(&mut r.init_metadata(), &None, &None, &req.tags, &req.creator);
        }
        ToolRequest::RaveStreamStart(req) => {
            let mut r = builder.reborrow().init_rave_stream_start();
            r.set_model(req.model.as_deref().unwrap_or(""));
            r.set_input_identity(&req.input_identity);
            r.set_output_identity(&req.output_identity);
            r.set_buffer_size(req.buffer_size.unwrap_or(2048));
        }
        ToolRequest::RaveStreamStop(req) => {
            builder.reborrow().init_rave_stream_stop().set_stream_id(&req.stream_id);
        }
        ToolRequest::RaveStreamStatus(req) => {
            builder.reborrow().init_rave_stream_status().set_stream_id(&req.stream_id);
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
        ToolRequest::EventPoll(_) => {
            // EventPoll is MCP-only, not sent over ZMQ
            unimplemented!("EventPoll is MCP-only, use JSON serialization")
        }
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
        ToolRequest::GardenGetAudioSnapshot(req) => {
            let mut s = builder.reborrow().init_garden_get_audio_snapshot();
            s.set_frames(req.frames);
        }
        ToolRequest::GardenClearRegions => builder.reborrow().set_garden_clear_regions(()),
        ToolRequest::GetToolHelp(req) => builder.reborrow().init_get_tool_help().set_topic(req.topic.as_deref().unwrap_or("")),

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
        tools_capnp::tool_request::CasStats(()) => Ok(ToolRequest::CasStats),
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
        tools_capnp::tool_request::MidiInfo(m) => {
            let m = m?;
            Ok(ToolRequest::MidiInfo(MidiInfoRequest {
                artifact_id: capnp_optional_string(m.get_artifact_id()?),
                hash: capnp_optional_string(m.get_hash()?),
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
        tools_capnp::tool_request::GardenGetAudioSnapshot(s) => {
            let s = s?;
            Ok(ToolRequest::GardenGetAudioSnapshot(GardenGetAudioSnapshotRequest {
                frames: s.get_frames(),
            }))
        }
        tools_capnp::tool_request::GardenClearRegions(()) => Ok(ToolRequest::GardenClearRegions),
        tools_capnp::tool_request::GetToolHelp(h) => Ok(ToolRequest::GetToolHelp(GetToolHelpRequest { topic: capnp_optional_string(h?.get_topic()?) })),

        // RAVE tools
        tools_capnp::tool_request::RaveEncode(r) => {
            let r = r?;
            let m = r.get_metadata()?;
            Ok(ToolRequest::RaveEncode(RaveEncodeRequest {
                audio_hash: r.get_audio_hash()?.to_str()?.to_string(),
                model: capnp_optional_string(r.get_model()?),
                tags: capnp_string_list(m.get_tags()?),
                creator: capnp_optional_string(m.get_creator()?),
            }))
        }
        tools_capnp::tool_request::RaveDecode(r) => {
            let r = r?;
            let m = r.get_metadata()?;
            Ok(ToolRequest::RaveDecode(RaveDecodeRequest {
                latent_hash: r.get_latent_hash()?.to_str()?.to_string(),
                latent_shape: r.get_latent_shape()?.iter().collect(),
                model: capnp_optional_string(r.get_model()?),
                tags: capnp_string_list(m.get_tags()?),
                creator: capnp_optional_string(m.get_creator()?),
            }))
        }
        tools_capnp::tool_request::RaveReconstruct(r) => {
            let r = r?;
            let m = r.get_metadata()?;
            Ok(ToolRequest::RaveReconstruct(RaveReconstructRequest {
                audio_hash: r.get_audio_hash()?.to_str()?.to_string(),
                model: capnp_optional_string(r.get_model()?),
                tags: capnp_string_list(m.get_tags()?),
                creator: capnp_optional_string(m.get_creator()?),
            }))
        }
        tools_capnp::tool_request::RaveGenerate(r) => {
            let r = r?;
            let m = r.get_metadata()?;
            Ok(ToolRequest::RaveGenerate(RaveGenerateRequest {
                model: capnp_optional_string(r.get_model()?),
                duration_seconds: if r.get_duration_seconds() > 0.0 { Some(r.get_duration_seconds()) } else { None },
                temperature: if r.get_temperature() > 0.0 { Some(r.get_temperature()) } else { None },
                tags: capnp_string_list(m.get_tags()?),
                creator: capnp_optional_string(m.get_creator()?),
            }))
        }
        tools_capnp::tool_request::RaveStreamStart(r) => {
            let r = r?;
            Ok(ToolRequest::RaveStreamStart(RaveStreamStartRequest {
                model: capnp_optional_string(r.get_model()?),
                input_identity: r.get_input_identity()?.to_str()?.to_string(),
                output_identity: r.get_output_identity()?.to_str()?.to_string(),
                buffer_size: if r.get_buffer_size() > 0 { Some(r.get_buffer_size()) } else { None },
            }))
        }
        tools_capnp::tool_request::RaveStreamStop(r) => {
            let r = r?;
            Ok(ToolRequest::RaveStreamStop(RaveStreamStopRequest {
                stream_id: r.get_stream_id()?.to_str()?.to_string(),
            }))
        }
        tools_capnp::tool_request::RaveStreamStatus(r) => {
            let r = r?;
            Ok(ToolRequest::RaveStreamStatus(RaveStreamStatusRequest {
                stream_id: r.get_stream_id()?.to_str()?.to_string(),
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
/// Note: ZeroShot labels must be populated separately from zeroShotLabels field
fn capnp_to_analysis_task(task: common_capnp::AnalysisTask, zero_shot_labels: Vec<String>) -> AnalysisTask {
    match task {
        common_capnp::AnalysisTask::Classify => AnalysisTask::Classify,
        common_capnp::AnalysisTask::Beats => AnalysisTask::Beats,
        common_capnp::AnalysisTask::Embeddings => AnalysisTask::Embeddings,
        common_capnp::AnalysisTask::Genre => AnalysisTask::Genre,
        common_capnp::AnalysisTask::Mood => AnalysisTask::Mood,
        common_capnp::AnalysisTask::ZeroShot => AnalysisTask::ZeroShot { labels: zero_shot_labels },
    }
}

/// Helper: Convert Rust AnalysisTask to capnp AnalysisTask
/// Note: ZeroShot labels are handled separately via zeroShotLabels field
fn analysis_task_to_capnp(task: &AnalysisTask) -> common_capnp::AnalysisTask {
    match task {
        AnalysisTask::Classify => common_capnp::AnalysisTask::Classify,
        AnalysisTask::Beats => common_capnp::AnalysisTask::Beats,
        AnalysisTask::Embeddings => common_capnp::AnalysisTask::Embeddings,
        AnalysisTask::Genre => common_capnp::AnalysisTask::Genre,
        AnalysisTask::Mood => common_capnp::AnalysisTask::Mood,
        AnalysisTask::ZeroShot { .. } => common_capnp::AnalysisTask::ZeroShot,
    }
}

// =============================================================================
// DAW Tool Helper Functions
// =============================================================================

/// Helper: Convert Rust Space to capnp Space
fn space_to_capnp(space: &crate::Space) -> common_capnp::Space {
    match space {
        crate::Space::Orpheus => common_capnp::Space::Orpheus,
        crate::Space::OrpheusChildren => common_capnp::Space::OrpheusChildren,
        crate::Space::OrpheusMonoMelodies => common_capnp::Space::OrpheusMonoMelodies,
        crate::Space::OrpheusLoops => common_capnp::Space::OrpheusLoops,
        crate::Space::OrpheusBridge => common_capnp::Space::OrpheusBridge,
        crate::Space::MusicGen => common_capnp::Space::MusicGen,
        crate::Space::Yue => common_capnp::Space::Yue,
        crate::Space::Abc => common_capnp::Space::Abc,
    }
}

/// Helper: Convert capnp Space to Rust Space
fn capnp_to_space(space: common_capnp::Space) -> crate::Space {
    match space {
        common_capnp::Space::Orpheus => crate::Space::Orpheus,
        common_capnp::Space::OrpheusChildren => crate::Space::OrpheusChildren,
        common_capnp::Space::OrpheusMonoMelodies => crate::Space::OrpheusMonoMelodies,
        common_capnp::Space::OrpheusLoops => crate::Space::OrpheusLoops,
        common_capnp::Space::OrpheusBridge => crate::Space::OrpheusBridge,
        common_capnp::Space::MusicGen => crate::Space::MusicGen,
        common_capnp::Space::Yue => crate::Space::Yue,
        common_capnp::Space::Abc => crate::Space::Abc,
    }
}

/// Helper: Convert Rust InferenceContext to capnp InferenceContext
fn inference_to_capnp(mut builder: common_capnp::inference_context::Builder, inference: &crate::InferenceContext) {
    builder.set_temperature(inference.temperature.unwrap_or(0.0));
    builder.set_top_p(inference.top_p.unwrap_or(0.0));
    builder.set_top_k(inference.top_k.unwrap_or(0));
    builder.set_seed(inference.seed.unwrap_or(0));
    builder.set_max_tokens(inference.max_tokens.unwrap_or(0));
    builder.set_duration_seconds(inference.duration_seconds.unwrap_or(0.0));
    builder.set_guidance_scale(inference.guidance_scale.unwrap_or(0.0));
    builder.set_variant(inference.variant.as_deref().unwrap_or(""));
}

/// Helper: Convert capnp InferenceContext to Rust InferenceContext
fn capnp_to_inference(reader: common_capnp::inference_context::Reader) -> capnp::Result<crate::InferenceContext> {
    Ok(crate::InferenceContext {
        temperature: Some(reader.get_temperature()).filter(|&v| v != 0.0),
        top_p: Some(reader.get_top_p()).filter(|&v| v != 0.0),
        top_k: Some(reader.get_top_k()).filter(|&v| v != 0),
        seed: Some(reader.get_seed()).filter(|&v| v != 0),
        max_tokens: Some(reader.get_max_tokens()).filter(|&v| v != 0),
        duration_seconds: Some(reader.get_duration_seconds()).filter(|&v| v != 0.0),
        guidance_scale: Some(reader.get_guidance_scale()).filter(|&v| v != 0.0),
        variant: {
            let v = reader.get_variant()?.to_str()?;
            if v.is_empty() { None } else { Some(v.to_string()) }
        },
    })
}

/// Helper: Convert Rust ProjectionTarget to capnp ProjectionTarget
fn projection_target_to_capnp(builder: common_capnp::projection_target::Builder, target: &crate::ProjectionTarget) {
    match target {
        crate::ProjectionTarget::Audio { soundfont_hash, sample_rate } => {
            let mut a = builder.init_audio();
            a.set_soundfont_hash(soundfont_hash);
            a.set_sample_rate(sample_rate.unwrap_or(44100));
        }
        crate::ProjectionTarget::Midi { channel, velocity, program } => {
            let mut m = builder.init_midi();
            m.set_channel(channel.unwrap_or(0));
            m.set_velocity(velocity.unwrap_or(80));
            m.set_program(program.unwrap_or(0));
        }
    }
}

/// Helper: Convert capnp ProjectionTarget to Rust ProjectionTarget
fn capnp_to_projection_target(reader: common_capnp::projection_target::Reader) -> capnp::Result<crate::ProjectionTarget> {
    match reader.which()? {
        common_capnp::projection_target::Audio(a) => {
            let a = a;
            Ok(crate::ProjectionTarget::Audio {
                soundfont_hash: a.get_soundfont_hash()?.to_str()?.to_string(),
                sample_rate: Some(a.get_sample_rate()).filter(|&v| v != 0),
            })
        }
        common_capnp::projection_target::Midi(m) => {
            let m = m;
            Ok(crate::ProjectionTarget::Midi {
                channel: Some(m.get_channel()).filter(|&v| v != 0),
                velocity: Some(m.get_velocity()).filter(|&v| v != 0),
                program: Some(m.get_program()).filter(|&v| v != 0),
            })
        }
    }
}

// =============================================================================
// Response Serialization (Rust → Cap'n Proto)
// =============================================================================

fn response_to_capnp_tool_response(
    builder: &mut responses_capnp::tool_response::Builder,
    response: &ToolResponse,
) -> capnp::Result<()> {
    match response {
        // CAS Operations
        ToolResponse::CasStored(r) => {
            let mut b = builder.reborrow().init_cas_stored();
            b.set_hash(&r.hash);
            b.set_size(r.size as u64);
            b.set_mime_type(&r.mime_type);
        }
        ToolResponse::CasContent(r) => {
            let mut b = builder.reborrow().init_cas_content();
            b.set_hash(&r.hash);
            b.set_size(r.size as u64);
            b.set_data(&r.data);
        }
        ToolResponse::CasInspected(r) => {
            let mut b = builder.reborrow().init_cas_inspected();
            b.set_hash(&r.hash);
            b.set_exists(r.exists);
            b.set_size(r.size.unwrap_or(0) as u64);
            b.set_preview(r.preview.as_deref().unwrap_or(""));
        }
        ToolResponse::CasStats(r) => {
            let mut b = builder.reborrow().init_cas_stats();
            b.set_total_items(r.total_items);
            b.set_total_bytes(r.total_bytes);
            b.set_cas_dir(&r.cas_dir);
        }

        // Artifacts
        ToolResponse::ArtifactCreated(r) => {
            let mut b = builder.reborrow().init_artifact_created();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            let mut tags = b.reborrow().init_tags(r.tags.len() as u32);
            for (i, tag) in r.tags.iter().enumerate() {
                tags.set(i as u32, tag);
            }
            b.set_creator(&r.creator);
        }
        ToolResponse::ArtifactInfo(r) => {
            let mut b = builder.reborrow().init_artifact_info();
            b.set_id(&r.id);
            b.set_content_hash(&r.content_hash);
            b.set_mime_type(&r.mime_type);
            let mut tags = b.reborrow().init_tags(r.tags.len() as u32);
            for (i, tag) in r.tags.iter().enumerate() {
                tags.set(i as u32, tag);
            }
            b.set_creator(&r.creator);
            b.set_created_at(r.created_at);
            b.set_parent_id(r.parent_id.as_deref().unwrap_or(""));
            b.set_variation_set_id(r.variation_set_id.as_deref().unwrap_or(""));
            if let Some(ref meta) = r.metadata {
                let mut m = b.reborrow().init_metadata();
                m.set_duration_seconds(meta.duration_seconds.unwrap_or(0.0));
                m.set_sample_rate(meta.sample_rate.unwrap_or(0));
                m.set_channels(meta.channels.unwrap_or(0));
                if let Some(ref midi) = meta.midi_info {
                    let mut mi = m.init_midi_info();
                    mi.set_tracks(midi.tracks);
                    mi.set_ticks_per_quarter(midi.ticks_per_quarter);
                    mi.set_duration_ticks(midi.duration_ticks);
                }
            }
        }
        ToolResponse::ArtifactList(r) => {
            let mut b = builder.reborrow().init_artifact_list();
            let mut arts = b.reborrow().init_artifacts(r.artifacts.len() as u32);
            for (i, art) in r.artifacts.iter().enumerate() {
                let mut a = arts.reborrow().get(i as u32);
                a.set_id(&art.id);
                a.set_content_hash(&art.content_hash);
                a.set_mime_type(&art.mime_type);
                let mut tags = a.reborrow().init_tags(art.tags.len() as u32);
                for (j, tag) in art.tags.iter().enumerate() {
                    tags.set(j as u32, tag);
                }
                a.set_creator(&art.creator);
                a.set_created_at(art.created_at);
                a.set_parent_id(art.parent_id.as_deref().unwrap_or(""));
                a.set_variation_set_id(art.variation_set_id.as_deref().unwrap_or(""));
            }
            b.set_count(r.count as u64);
        }

        // Jobs
        ToolResponse::JobStarted(r) => {
            let mut b = builder.reborrow().init_job_started();
            b.set_job_id(&r.job_id);
            b.set_tool(&r.tool);
        }
        ToolResponse::JobStatus(r) => {
            let mut b = builder.reborrow().init_job_status();
            b.set_job_id(&r.job_id);
            b.set_status(job_state_to_capnp(&r.status));
            b.set_source(&r.source);
            if let Some(ref result) = r.result {
                let mut nested = b.reborrow().init_result();
                response_to_capnp_tool_response(&mut nested, result)?;
            }
            b.set_error(r.error.as_deref().unwrap_or(""));
            b.set_created_at(r.created_at);
            b.set_started_at(r.started_at.unwrap_or(0));
            b.set_completed_at(r.completed_at.unwrap_or(0));
        }
        ToolResponse::JobList(r) => {
            let mut b = builder.reborrow().init_job_list();
            let mut jobs = b.reborrow().init_jobs(r.jobs.len() as u32);
            for (i, job) in r.jobs.iter().enumerate() {
                let mut j = jobs.reborrow().get(i as u32);
                j.set_job_id(&job.job_id);
                j.set_status(job_state_to_capnp(&job.status));
                j.set_source(&job.source);
                j.set_error(job.error.as_deref().unwrap_or(""));
                j.set_created_at(job.created_at);
                j.set_started_at(job.started_at.unwrap_or(0));
                j.set_completed_at(job.completed_at.unwrap_or(0));
            }
            b.set_total(r.total as u64);
            let mut counts = b.reborrow().init_by_status();
            counts.set_pending(r.by_status.pending as u64);
            counts.set_running(r.by_status.running as u64);
            counts.set_complete(r.by_status.complete as u64);
            counts.set_failed(r.by_status.failed as u64);
            counts.set_cancelled(r.by_status.cancelled as u64);
        }
        ToolResponse::JobPollResult(r) => {
            let mut b = builder.reborrow().init_job_poll_result();
            let mut completed = b.reborrow().init_completed(r.completed.len() as u32);
            for (i, id) in r.completed.iter().enumerate() {
                completed.set(i as u32, id);
            }
            let mut failed = b.reborrow().init_failed(r.failed.len() as u32);
            for (i, id) in r.failed.iter().enumerate() {
                failed.set(i as u32, id);
            }
            let mut pending = b.reborrow().init_pending(r.pending.len() as u32);
            for (i, id) in r.pending.iter().enumerate() {
                pending.set(i as u32, id);
            }
            b.set_timed_out(r.timed_out);
        }
        ToolResponse::JobPoll(r) => {
            let mut b = builder.reborrow().init_job_poll();
            let mut completed = b.reborrow().init_completed(r.completed.len() as u32);
            for (i, id) in r.completed.iter().enumerate() {
                completed.set(i as u32, id);
            }
            let mut failed = b.reborrow().init_failed(r.failed.len() as u32);
            for (i, id) in r.failed.iter().enumerate() {
                failed.set(i as u32, id);
            }
            let mut pending = b.reborrow().init_pending(r.pending.len() as u32);
            for (i, id) in r.pending.iter().enumerate() {
                pending.set(i as u32, id);
            }
            b.set_reason(&r.reason);
            b.set_elapsed_ms(r.elapsed_ms);
        }
        ToolResponse::JobCancel(r) => {
            let mut b = builder.reborrow().init_job_cancel();
            b.set_job_id(&r.job_id);
            b.set_cancelled(r.cancelled);
        }
        // Event Polling (MCP-only, not serialized over ZMQ)
        ToolResponse::EventPoll(_) => {
            // EventPoll is handled directly by holler/dispatcher, not sent over ZMQ
            unimplemented!("EventPoll is MCP-only, use JSON serialization")
        }

        // ABC Notation
        ToolResponse::AbcParsed(r) => {
            let mut b = builder.reborrow().init_abc_parsed();
            b.set_valid(r.valid);
            b.set_title(r.title.as_deref().unwrap_or(""));
            b.set_key(r.key.as_deref().unwrap_or(""));
            b.set_meter(r.meter.as_deref().unwrap_or(""));
            b.set_tempo(r.tempo.unwrap_or(0));
            b.set_notes_count(r.notes_count as u64);
        }
        ToolResponse::AbcValidated(r) => {
            let mut b = builder.reborrow().init_abc_validated();
            b.set_valid(r.valid);
            let mut errors = b.reborrow().init_errors(r.errors.len() as u32);
            for (i, err) in r.errors.iter().enumerate() {
                let mut e = errors.reborrow().get(i as u32);
                e.set_line(err.line as u64);
                e.set_column(err.column as u64);
                e.set_message(&err.message);
            }
            let mut warnings = b.reborrow().init_warnings(r.warnings.len() as u32);
            for (i, w) in r.warnings.iter().enumerate() {
                warnings.set(i as u32, w);
            }
        }
        ToolResponse::AbcTransposed(r) => {
            let mut b = builder.reborrow().init_abc_transposed();
            b.set_abc(&r.abc);
            b.set_original_key(r.original_key.as_deref().unwrap_or(""));
            b.set_new_key(r.new_key.as_deref().unwrap_or(""));
            b.set_semitones(r.semitones);
        }
        ToolResponse::AbcConverted(r) => {
            let mut b = builder.reborrow().init_abc_converted();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            b.set_duration_seconds(r.duration_seconds);
            b.set_notes_count(r.notes_count as u64);
        }
        ToolResponse::AbcToMidi(r) => {
            let mut b = builder.reborrow().init_abc_to_midi();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
        }
        ToolResponse::MidiToWav(r) => {
            let mut b = builder.reborrow().init_midi_to_wav();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            b.set_sample_rate(r.sample_rate);
            b.set_duration_secs(r.duration_secs.unwrap_or(0.0));
        }

        // SoundFont
        ToolResponse::SoundfontInfo(r) => {
            let mut b = builder.reborrow().init_soundfont_info();
            b.set_name(&r.name);
            let mut presets = b.reborrow().init_presets(r.presets.len() as u32);
            for (i, p) in r.presets.iter().enumerate() {
                let mut preset = presets.reborrow().get(i as u32);
                preset.set_bank(p.bank);
                preset.set_program(p.program);
                preset.set_name(&p.name);
            }
            b.set_preset_count(r.preset_count as u64);
        }
        ToolResponse::SoundfontPresetInfo(r) => {
            let mut b = builder.reborrow().init_soundfont_preset_info();
            b.set_bank(r.bank);
            b.set_program(r.program);
            b.set_name(&r.name);
            let mut regions = b.reborrow().init_regions(r.regions.len() as u32);
            for (i, reg) in r.regions.iter().enumerate() {
                let mut region = regions.reborrow().get(i as u32);
                region.set_key_low(reg.key_low);
                region.set_key_high(reg.key_high);
                region.set_velocity_low(reg.velocity_low);
                region.set_velocity_high(reg.velocity_high);
                region.set_sample_name(reg.sample_name.as_deref().unwrap_or(""));
            }
        }

        // Orpheus MIDI Generation
        ToolResponse::OrpheusGenerated(r) => {
            let mut b = builder.reborrow().init_orpheus_generated();
            let mut hashes = b.reborrow().init_output_hashes(r.output_hashes.len() as u32);
            for (i, h) in r.output_hashes.iter().enumerate() {
                hashes.set(i as u32, h);
            }
            let mut aids = b.reborrow().init_artifact_ids(r.artifact_ids.len() as u32);
            for (i, a) in r.artifact_ids.iter().enumerate() {
                aids.set(i as u32, a);
            }
            let mut tokens = b.reborrow().init_tokens_per_variation(r.tokens_per_variation.len() as u32);
            for (i, t) in r.tokens_per_variation.iter().enumerate() {
                tokens.set(i as u32, *t);
            }
            b.set_total_tokens(r.total_tokens);
            b.set_variation_set_id(r.variation_set_id.as_deref().unwrap_or(""));
            b.set_summary(&r.summary);
        }
        ToolResponse::OrpheusClassified(r) => {
            let b = builder.reborrow().init_orpheus_classified();
            let mut cls = b.init_classifications(r.classifications.len() as u32);
            for (i, c) in r.classifications.iter().enumerate() {
                let mut classification = cls.reborrow().get(i as u32);
                classification.set_label(&c.label);
                classification.set_confidence(c.confidence);
            }
        }

        // Audio Generation
        ToolResponse::AudioGenerated(r) => {
            let mut b = builder.reborrow().init_audio_generated();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            b.set_duration_seconds(r.duration_seconds);
            b.set_sample_rate(r.sample_rate);
            b.set_format(audio_format_to_capnp(&r.format));
            b.set_genre(r.genre.as_deref().unwrap_or(""));
        }

        // Audio Analysis
        ToolResponse::BeatsAnalyzed(r) => {
            let mut b = builder.reborrow().init_beats_analyzed();
            let mut beats = b.reborrow().init_beats(r.beats.len() as u32);
            for (i, beat) in r.beats.iter().enumerate() {
                beats.set(i as u32, *beat);
            }
            let mut downbeats = b.reborrow().init_downbeats(r.downbeats.len() as u32);
            for (i, db) in r.downbeats.iter().enumerate() {
                downbeats.set(i as u32, *db);
            }
            b.set_estimated_bpm(r.estimated_bpm);
            b.set_confidence(r.confidence);
        }
        ToolResponse::ClapAnalyzed(r) => {
            let mut b = builder.reborrow().init_clap_analyzed();
            if let Some(ref emb) = r.embeddings {
                let mut embeddings = b.reborrow().init_embeddings(emb.len() as u32);
                for (i, e) in emb.iter().enumerate() {
                    embeddings.set(i as u32, *e);
                }
            }
            if let Some(ref genre) = r.genre {
                let mut genres = b.reborrow().init_genre(genre.len() as u32);
                for (i, g) in genre.iter().enumerate() {
                    let mut classification = genres.reborrow().get(i as u32);
                    classification.set_label(&g.label);
                    classification.set_score(g.score);
                }
            }
            if let Some(ref mood) = r.mood {
                let mut moods = b.reborrow().init_mood(mood.len() as u32);
                for (i, m) in mood.iter().enumerate() {
                    let mut classification = moods.reborrow().get(i as u32);
                    classification.set_label(&m.label);
                    classification.set_score(m.score);
                }
            }
            if let Some(ref zs) = r.zero_shot {
                let mut zero_shots = b.reborrow().init_zero_shot(zs.len() as u32);
                for (i, z) in zs.iter().enumerate() {
                    let mut classification = zero_shots.reborrow().get(i as u32);
                    classification.set_label(&z.label);
                    classification.set_score(z.score);
                }
            }
            b.set_similarity(r.similarity.unwrap_or(0.0));
        }
        ToolResponse::MidiInfo(r) => {
            let mut b = builder.reborrow().init_midi_info();
            if let Some(tempo) = r.tempo_bpm {
                b.set_tempo_bpm(tempo);
                b.set_has_tempo_bpm(true);
            } else {
                b.set_has_tempo_bpm(false);
            }
            {
                let mut changes = b.reborrow().init_tempo_changes(r.tempo_changes.len() as u32);
                for (i, tc) in r.tempo_changes.iter().enumerate() {
                    let mut c = changes.reborrow().get(i as u32);
                    c.set_tick(tc.tick);
                    c.set_bpm(tc.bpm);
                }
            }
            if let Some((num, denom)) = r.time_signature {
                b.set_time_sig_num(num);
                b.set_time_sig_denom(denom);
                b.set_has_time_sig(true);
            } else {
                b.set_has_time_sig(false);
            }
            b.set_duration_seconds(r.duration_seconds);
            b.set_track_count(r.track_count as u16);
            b.set_ppq(r.ppq);
            b.set_note_count(r.note_count as u32);
            b.set_format(r.format);
        }

        // Garden/Transport
        ToolResponse::GardenStatus(r) => {
            let mut b = builder.reborrow().init_garden_status();
            b.set_state(transport_state_to_capnp(&r.state));
            b.set_position_beats(r.position_beats);
            b.set_tempo_bpm(r.tempo_bpm);
            b.set_region_count(r.region_count as u64);
        }
        ToolResponse::GardenRegions(r) => {
            let mut b = builder.reborrow().init_garden_regions();
            let mut regions = b.reborrow().init_regions(r.regions.len() as u32);
            for (i, reg) in r.regions.iter().enumerate() {
                let mut region = regions.reborrow().get(i as u32);
                region.set_region_id(&reg.region_id);
                region.set_position(reg.position);
                region.set_duration(reg.duration);
                region.set_behavior_type(&reg.behavior_type);
                region.set_content_id(&reg.content_id);
            }
            b.set_count(r.count as u64);
        }
        ToolResponse::GardenRegionCreated(r) => {
            let mut b = builder.reborrow().init_garden_region_created();
            b.set_region_id(&r.region_id);
            b.set_position(r.position);
            b.set_duration(r.duration);
        }
        ToolResponse::GardenQueryResult(r) => {
            let mut b = builder.reborrow().init_garden_query_result();
            b.set_results(&serde_json::to_string(&r.results).unwrap_or_default());
            b.set_count(r.count as u64);
        }
        ToolResponse::GardenAudioStatus(r) => {
            let mut b = builder.reborrow().init_garden_audio_status();
            b.set_attached(r.attached);
            b.set_device_name(r.device_name.as_deref().unwrap_or(""));
            b.set_sample_rate(r.sample_rate.unwrap_or(0));
            b.set_latency_frames(r.latency_frames.unwrap_or(0));
            b.set_buffer_underruns(r.underruns);
            b.set_callbacks(r.callbacks);
            b.set_samples_written(r.samples_written);
            b.set_monitor_reads(r.monitor_reads);
            b.set_monitor_samples(r.monitor_samples);
        }
        ToolResponse::GardenInputStatus(r) => {
            let mut b = builder.reborrow().init_garden_input_status();
            b.set_attached(r.attached);
            b.set_device_name(r.device_name.as_deref().unwrap_or(""));
            b.set_sample_rate(r.sample_rate.unwrap_or(0));
            b.set_channels(r.channels.unwrap_or(0));
            b.set_monitor_enabled(r.monitor_enabled);
            b.set_monitor_gain(r.monitor_gain);
            b.set_callbacks(r.callbacks);
            b.set_samples_captured(r.samples_captured);
            b.set_overruns(r.overruns);
        }
        ToolResponse::GardenMonitorStatus(r) => {
            let mut b = builder.reborrow().init_garden_monitor_status();
            b.set_enabled(r.enabled);
            b.set_gain(r.gain);
        }
        ToolResponse::GardenAudioSnapshot(r) => {
            let mut b = builder.reborrow().init_garden_audio_snapshot();
            b.set_sample_rate(r.sample_rate);
            b.set_channels(r.channels);
            b.set_format(r.format);
            let mut samples = b.reborrow().init_samples(r.samples.len() as u32);
            for (i, &sample) in r.samples.iter().enumerate() {
                samples.set(i as u32, sample);
            }
        }

        // Graph
        ToolResponse::GraphIdentity(r) => {
            let mut b = builder.reborrow().init_graph_identity();
            b.set_id(&r.id);
            b.set_name(&r.name);
            b.set_created_at(r.created_at);
        }
        ToolResponse::GraphIdentities(r) => {
            let mut b = builder.reborrow().init_graph_identities();
            let mut ids = b.reborrow().init_identities(r.identities.len() as u32);
            for (i, id) in r.identities.iter().enumerate() {
                let mut identity = ids.reborrow().get(i as u32);
                identity.set_id(&id.id);
                identity.set_name(&id.name);
                let mut tags = identity.reborrow().init_tags(id.tags.len() as u32);
                for (j, tag) in id.tags.iter().enumerate() {
                    tags.set(j as u32, tag);
                }
            }
            b.set_count(r.count as u64);
        }
        ToolResponse::GraphConnection(r) => {
            let mut b = builder.reborrow().init_graph_connection();
            b.set_connection_id(&r.connection_id);
            b.set_from_identity(&r.from_identity);
            b.set_from_port(&r.from_port);
            b.set_to_identity(&r.to_identity);
            b.set_to_port(&r.to_port);
            b.set_transport(r.transport.as_deref().unwrap_or(""));
        }
        ToolResponse::GraphTags(r) => {
            let mut b = builder.reborrow().init_graph_tags();
            b.set_identity_id(&r.identity_id);
            let mut tags = b.reborrow().init_tags(r.tags.len() as u32);
            for (i, tag) in r.tags.iter().enumerate() {
                let mut t = tags.reborrow().get(i as u32);
                t.set_namespace(&tag.namespace);
                t.set_value(&tag.value);
            }
        }
        ToolResponse::GraphContext(r) => {
            let mut b = builder.reborrow().init_graph_context();
            b.set_context(&r.context);
            b.set_artifact_count(r.artifact_count as u64);
            b.set_identity_count(r.identity_count as u64);
        }
        ToolResponse::GraphQueryResult(r) => {
            let mut b = builder.reborrow().init_graph_query_result();
            b.set_results(&serde_json::to_string(&r.results).unwrap_or_default());
            b.set_count(r.count as u64);
        }
        ToolResponse::GraphBind(r) => {
            let mut b = builder.reborrow().init_graph_bind();
            b.set_identity_id(&r.identity_id);
            b.set_name(&r.name);
            b.set_hints_count(r.hints_count as u32);
        }
        ToolResponse::GraphTag(r) => {
            let mut b = builder.reborrow().init_graph_tag();
            b.set_identity_id(&r.identity_id);
            b.set_tag(&r.tag);
        }
        ToolResponse::GraphConnect(r) => {
            let mut b = builder.reborrow().init_graph_connect();
            b.set_from_identity(&r.from_identity);
            b.set_from_port(&r.from_port);
            b.set_to_identity(&r.to_identity);
            b.set_to_port(&r.to_port);
        }

        // Config
        ToolResponse::ConfigValue(r) => {
            let mut b = builder.reborrow().init_config_value();
            b.set_section(r.section.as_deref().unwrap_or(""));
            b.set_key(r.key.as_deref().unwrap_or(""));
            b.set_value(&serde_json::to_string(&r.value).unwrap_or_default());
        }

        // Admin
        ToolResponse::ToolsList(r) => {
            let mut b = builder.reborrow().init_tools_list();
            let mut tools = b.reborrow().init_tools(r.tools.len() as u32);
            for (i, tool) in r.tools.iter().enumerate() {
                let mut t = tools.reborrow().get(i as u32);
                t.set_name(&tool.name);
                t.set_description(&tool.description);
                t.set_input_schema(&serde_json::to_string(&tool.input_schema).unwrap_or_default());
            }
            b.set_count(r.count as u64);
        }

        // Simple
        ToolResponse::Ack(r) => {
            builder.reborrow().init_ack().set_message(&r.message);
        }
        ToolResponse::AnnotationAdded(r) => {
            let mut b = builder.reborrow().init_annotation_added();
            b.set_artifact_id(&r.artifact_id);
            b.set_annotation_id(&r.annotation_id);
        }

        // Vibeweaver
        ToolResponse::WeaveEval(r) => {
            let mut b = builder.reborrow().init_weave_eval();
            b.set_output_type(weave_output_type_to_capnp(&r.output_type));
            b.set_result(r.result.as_deref().unwrap_or(""));
            b.set_stdout(r.stdout.as_deref().unwrap_or(""));
            b.set_stderr(r.stderr.as_deref().unwrap_or(""));
        }
        ToolResponse::WeaveSession(r) => {
            let mut b = builder.reborrow().init_weave_session();
            if let Some(ref session) = r.session {
                let mut s = b.reborrow().init_session();
                s.set_id(&session.id);
                s.set_name(&session.name);
                s.set_vibe(session.vibe.as_deref().unwrap_or(""));
            }
            b.set_message(r.message.as_deref().unwrap_or(""));
        }
        ToolResponse::WeaveReset(r) => {
            let mut b = builder.reborrow().init_weave_reset();
            b.set_reset(r.reset);
            b.set_message(&r.message);
        }
        ToolResponse::WeaveHelp(r) => {
            let mut b = builder.reborrow().init_weave_help();
            b.set_help(&r.help);
            b.set_topic(r.topic.as_deref().unwrap_or(""));
        }
        ToolResponse::ToolHelp(r) => {
            let mut b = builder.reborrow().init_tool_help();
            b.set_help(&r.help);
            b.set_topic(r.topic.as_deref().unwrap_or(""));
        }
        ToolResponse::Scheduled(r) => {
            let mut b = builder.reborrow().init_schedule_result();
            b.set_success(r.success);
            b.set_message(&r.message);
            b.set_region_id(&r.region_id);
            b.set_position(r.position);
            b.set_duration(r.duration);
            b.set_artifact_id(&r.artifact_id);
        }

        // DAW tool responses
        ToolResponse::AnalyzeResult(r) => {
            let mut b = builder.reborrow().init_analyze_result();
            b.set_content_hash(&r.content_hash);
            b.set_results(&serde_json::to_string(&r.results).unwrap_or_default());
            b.set_summary(&r.summary);
            b.set_artifact_id(r.artifact_id.as_deref().unwrap_or(""));
        }
        ToolResponse::ProjectResult(r) => {
            let mut b = builder.reborrow().init_project_result();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            b.set_projection_type(&r.projection_type);
            b.set_duration_seconds(r.duration_seconds.unwrap_or(0.0));
            b.set_sample_rate(r.sample_rate.unwrap_or(0));
        }

        // RAVE responses
        ToolResponse::RaveEncoded(r) => {
            let mut b = builder.reborrow().init_rave_encoded();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            let mut shape = b.reborrow().init_latent_shape(r.latent_shape.len() as u32);
            for (i, dim) in r.latent_shape.iter().enumerate() {
                shape.set(i as u32, *dim);
            }
            b.set_latent_dim(r.latent_dim);
            b.set_model(&r.model);
            b.set_sample_rate(r.sample_rate);
        }
        ToolResponse::RaveDecoded(r) => {
            let mut b = builder.reborrow().init_rave_decoded();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            b.set_duration_seconds(r.duration_seconds);
            b.set_sample_rate(r.sample_rate);
            b.set_model(&r.model);
        }
        ToolResponse::RaveReconstructed(r) => {
            let mut b = builder.reborrow().init_rave_reconstructed();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            b.set_duration_seconds(r.duration_seconds);
            b.set_sample_rate(r.sample_rate);
            b.set_model(&r.model);
        }
        ToolResponse::RaveGenerated(r) => {
            let mut b = builder.reborrow().init_rave_generated();
            b.set_artifact_id(&r.artifact_id);
            b.set_content_hash(&r.content_hash);
            b.set_duration_seconds(r.duration_seconds);
            b.set_sample_rate(r.sample_rate);
            b.set_model(&r.model);
            b.set_temperature(r.temperature);
        }
        ToolResponse::RaveStreamStarted(r) => {
            let mut b = builder.reborrow().init_rave_stream_started();
            b.set_stream_id(&r.stream_id);
            b.set_model(&r.model);
            b.set_input_identity(&r.input_identity);
            b.set_output_identity(&r.output_identity);
            b.set_latency_ms(r.latency_ms);
        }
        ToolResponse::RaveStreamStopped(r) => {
            let mut b = builder.reborrow().init_rave_stream_stopped();
            b.set_stream_id(&r.stream_id);
            b.set_duration_seconds(r.duration_seconds);
        }
        ToolResponse::RaveStreamStatus(r) => {
            let mut b = builder.reborrow().init_rave_stream_status();
            b.set_stream_id(&r.stream_id);
            b.set_running(r.running);
            b.set_model(&r.model);
            b.set_input_identity(&r.input_identity);
            b.set_output_identity(&r.output_identity);
            b.set_frames_processed(r.frames_processed);
            b.set_latency_ms(r.latency_ms);
        }
    }
    Ok(())
}

fn job_state_to_capnp(state: &JobState) -> responses_capnp::JobState {
    match state {
        JobState::Pending => responses_capnp::JobState::Pending,
        JobState::Running => responses_capnp::JobState::Running,
        JobState::Complete => responses_capnp::JobState::Complete,
        JobState::Failed => responses_capnp::JobState::Failed,
        JobState::Cancelled => responses_capnp::JobState::Cancelled,
    }
}

fn audio_format_to_capnp(format: &AudioFormat) -> responses_capnp::AudioFormat {
    match format {
        AudioFormat::Wav => responses_capnp::AudioFormat::Wav,
        AudioFormat::Mp3 => responses_capnp::AudioFormat::Mp3,
        AudioFormat::Flac => responses_capnp::AudioFormat::Flac,
    }
}

fn transport_state_to_capnp(state: &TransportState) -> responses_capnp::TransportState {
    match state {
        TransportState::Stopped => responses_capnp::TransportState::Stopped,
        TransportState::Playing => responses_capnp::TransportState::Playing,
        TransportState::Paused => responses_capnp::TransportState::Paused,
    }
}

fn weave_output_type_to_capnp(output_type: &WeaveOutputType) -> responses_capnp::WeaveOutputType {
    match output_type {
        WeaveOutputType::Expression => responses_capnp::WeaveOutputType::Expression,
        WeaveOutputType::Statement => responses_capnp::WeaveOutputType::Statement,
    }
}

// =============================================================================
// Response Deserialization (Cap'n Proto → Rust)
// =============================================================================

fn capnp_tool_response_to_response(
    reader: responses_capnp::tool_response::Reader,
) -> capnp::Result<ToolResponse> {
    use responses_capnp::tool_response::Which;

    match reader.which()? {
        // CAS Operations
        Which::CasStored(r) => {
            let r = r?;
            Ok(ToolResponse::CasStored(CasStoredResponse {
                hash: r.get_hash()?.to_string()?,
                size: r.get_size() as usize,
                mime_type: r.get_mime_type()?.to_string()?,
            }))
        }
        Which::CasContent(r) => {
            let r = r?;
            Ok(ToolResponse::CasContent(CasContentResponse {
                hash: r.get_hash()?.to_string()?,
                size: r.get_size() as usize,
                data: r.get_data()?.to_vec(),
            }))
        }
        Which::CasInspected(r) => {
            let r = r?;
            let size = r.get_size();
            let preview = r.get_preview()?.to_string()?;
            Ok(ToolResponse::CasInspected(CasInspectedResponse {
                hash: r.get_hash()?.to_string()?,
                exists: r.get_exists(),
                size: if size > 0 { Some(size as usize) } else { None },
                preview: if preview.is_empty() { None } else { Some(preview) },
            }))
        }
        Which::CasStats(r) => {
            let r = r?;
            Ok(ToolResponse::CasStats(CasStatsResponse {
                total_items: r.get_total_items(),
                total_bytes: r.get_total_bytes(),
                cas_dir: r.get_cas_dir()?.to_string()?,
            }))
        }

        // Artifacts
        Which::ArtifactCreated(r) => {
            let r = r?;
            let tags: Vec<String> = r.get_tags()?.iter()
                .filter_map(|t| t.ok().and_then(|s| s.to_string().ok()))
                .collect();
            Ok(ToolResponse::ArtifactCreated(ArtifactCreatedResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                tags,
                creator: r.get_creator()?.to_string()?,
            }))
        }
        Which::ArtifactInfo(r) => {
            let r = r?;
            let tags: Vec<String> = r.get_tags()?.iter()
                .filter_map(|t| t.ok().and_then(|s| s.to_string().ok()))
                .collect();
            let parent_id = r.get_parent_id()?.to_string()?;
            let variation_set_id = r.get_variation_set_id()?.to_string()?;
            let metadata = if r.has_metadata() {
                let m = r.get_metadata()?;
                Some(ArtifactMetadata {
                    duration_seconds: Some(m.get_duration_seconds()),
                    sample_rate: Some(m.get_sample_rate()),
                    channels: Some(m.get_channels()),
                    midi_info: if m.has_midi_info() {
                        let mi = m.get_midi_info()?;
                        Some(MidiMetadata {
                            tracks: mi.get_tracks(),
                            ticks_per_quarter: mi.get_ticks_per_quarter(),
                            duration_ticks: mi.get_duration_ticks(),
                        })
                    } else {
                        None
                    },
                })
            } else {
                None
            };
            Ok(ToolResponse::ArtifactInfo(ArtifactInfoResponse {
                id: r.get_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                mime_type: r.get_mime_type()?.to_string()?,
                tags,
                creator: r.get_creator()?.to_string()?,
                created_at: r.get_created_at(),
                parent_id: if parent_id.is_empty() { None } else { Some(parent_id) },
                variation_set_id: if variation_set_id.is_empty() { None } else { Some(variation_set_id) },
                metadata,
            }))
        }
        Which::ArtifactList(r) => {
            let r = r?;
            let mut artifacts = Vec::new();
            for art in r.get_artifacts()?.iter() {
                let tags: Vec<String> = art.get_tags()?.iter()
                    .filter_map(|t| t.ok().and_then(|s| s.to_string().ok()))
                    .collect();
                let parent_id = art.get_parent_id()?.to_string()?;
                let variation_set_id = art.get_variation_set_id()?.to_string()?;
                artifacts.push(ArtifactInfoResponse {
                    id: art.get_id()?.to_string()?,
                    content_hash: art.get_content_hash()?.to_string()?,
                    mime_type: art.get_mime_type()?.to_string()?,
                    tags,
                    creator: art.get_creator()?.to_string()?,
                    created_at: art.get_created_at(),
                    parent_id: if parent_id.is_empty() { None } else { Some(parent_id) },
                    variation_set_id: if variation_set_id.is_empty() { None } else { Some(variation_set_id) },
                    metadata: None,
                });
            }
            Ok(ToolResponse::ArtifactList(ArtifactListResponse {
                artifacts,
                count: r.get_count() as usize,
            }))
        }

        // Jobs
        Which::JobStarted(r) => {
            let r = r?;
            Ok(ToolResponse::JobStarted(JobStartedResponse {
                job_id: r.get_job_id()?.to_string()?,
                tool: r.get_tool()?.to_string()?,
            }))
        }
        Which::JobStatus(r) => {
            let r = r?;
            let result = if r.has_result() {
                Some(Box::new(capnp_tool_response_to_response(r.get_result()?)?))
            } else {
                None
            };
            let error = r.get_error()?.to_string()?;
            let started_at = r.get_started_at();
            let completed_at = r.get_completed_at();
            Ok(ToolResponse::JobStatus(JobStatusResponse {
                job_id: r.get_job_id()?.to_string()?,
                status: capnp_to_job_state(r.get_status()?),
                source: r.get_source()?.to_string()?,
                result,
                error: if error.is_empty() { None } else { Some(error) },
                created_at: r.get_created_at(),
                started_at: if started_at > 0 { Some(started_at) } else { None },
                completed_at: if completed_at > 0 { Some(completed_at) } else { None },
            }))
        }
        Which::JobList(r) => {
            let r = r?;
            let mut jobs = Vec::new();
            for job in r.get_jobs()?.iter() {
                let error = job.get_error()?.to_string()?;
                let started_at = job.get_started_at();
                let completed_at = job.get_completed_at();
                jobs.push(JobStatusResponse {
                    job_id: job.get_job_id()?.to_string()?,
                    status: capnp_to_job_state(job.get_status()?),
                    source: job.get_source()?.to_string()?,
                    result: None,
                    error: if error.is_empty() { None } else { Some(error) },
                    created_at: job.get_created_at(),
                    started_at: if started_at > 0 { Some(started_at) } else { None },
                    completed_at: if completed_at > 0 { Some(completed_at) } else { None },
                });
            }
            let counts = r.get_by_status()?;
            Ok(ToolResponse::JobList(JobListResponse {
                jobs,
                total: r.get_total() as usize,
                by_status: JobCounts {
                    pending: counts.get_pending() as usize,
                    running: counts.get_running() as usize,
                    complete: counts.get_complete() as usize,
                    failed: counts.get_failed() as usize,
                    cancelled: counts.get_cancelled() as usize,
                },
            }))
        }
        Which::JobPollResult(r) => {
            let r = r?;
            let completed: Vec<String> = r.get_completed()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            let failed: Vec<String> = r.get_failed()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            let pending: Vec<String> = r.get_pending()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            Ok(ToolResponse::JobPollResult(JobPollResultResponse {
                completed,
                failed,
                pending,
                timed_out: r.get_timed_out(),
            }))
        }

        // ABC Notation
        Which::AbcParsed(r) => {
            let r = r?;
            let title = r.get_title()?.to_string()?;
            let key = r.get_key()?.to_string()?;
            let meter = r.get_meter()?.to_string()?;
            let tempo = r.get_tempo();
            Ok(ToolResponse::AbcParsed(AbcParsedResponse {
                valid: r.get_valid(),
                title: if title.is_empty() { None } else { Some(title) },
                key: if key.is_empty() { None } else { Some(key) },
                meter: if meter.is_empty() { None } else { Some(meter) },
                tempo: if tempo > 0 { Some(tempo) } else { None },
                notes_count: r.get_notes_count() as usize,
            }))
        }
        Which::AbcValidated(r) => {
            let r = r?;
            let mut errors = Vec::new();
            for err in r.get_errors()?.iter() {
                errors.push(AbcValidationError {
                    line: err.get_line() as usize,
                    column: err.get_column() as usize,
                    message: err.get_message()?.to_string()?,
                });
            }
            let warnings: Vec<String> = r.get_warnings()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            Ok(ToolResponse::AbcValidated(AbcValidatedResponse {
                valid: r.get_valid(),
                errors,
                warnings,
            }))
        }
        Which::AbcTransposed(r) => {
            let r = r?;
            let original_key = r.get_original_key()?.to_string()?;
            let new_key = r.get_new_key()?.to_string()?;
            Ok(ToolResponse::AbcTransposed(AbcTransposedResponse {
                abc: r.get_abc()?.to_string()?,
                original_key: if original_key.is_empty() { None } else { Some(original_key) },
                new_key: if new_key.is_empty() { None } else { Some(new_key) },
                semitones: r.get_semitones(),
            }))
        }
        Which::AbcConverted(r) => {
            let r = r?;
            Ok(ToolResponse::AbcConverted(AbcConvertedResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                duration_seconds: r.get_duration_seconds(),
                notes_count: r.get_notes_count() as usize,
            }))
        }

        // SoundFont
        Which::SoundfontInfo(r) => {
            let r = r?;
            let mut presets = Vec::new();
            for p in r.get_presets()?.iter() {
                presets.push(SoundfontPreset {
                    bank: p.get_bank(),
                    program: p.get_program(),
                    name: p.get_name()?.to_string()?,
                });
            }
            Ok(ToolResponse::SoundfontInfo(SoundfontInfoResponse {
                name: r.get_name()?.to_string()?,
                presets,
                preset_count: r.get_preset_count() as usize,
            }))
        }
        Which::SoundfontPresetInfo(r) => {
            let r = r?;
            let mut regions = Vec::new();
            for reg in r.get_regions()?.iter() {
                let sample_name = reg.get_sample_name()?.to_string()?;
                regions.push(SoundfontRegion {
                    key_low: reg.get_key_low(),
                    key_high: reg.get_key_high(),
                    velocity_low: reg.get_velocity_low(),
                    velocity_high: reg.get_velocity_high(),
                    sample_name: if sample_name.is_empty() { None } else { Some(sample_name) },
                });
            }
            Ok(ToolResponse::SoundfontPresetInfo(SoundfontPresetInfoResponse {
                bank: r.get_bank(),
                program: r.get_program(),
                name: r.get_name()?.to_string()?,
                regions,
            }))
        }

        // Orpheus
        Which::OrpheusGenerated(r) => {
            let r = r?;
            let output_hashes: Vec<String> = r.get_output_hashes()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            let artifact_ids: Vec<String> = r.get_artifact_ids()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            let tokens_per_variation: Vec<u64> = r.get_tokens_per_variation()?.iter().collect();
            let variation_set_id = r.get_variation_set_id()?.to_string()?;
            Ok(ToolResponse::OrpheusGenerated(OrpheusGeneratedResponse {
                output_hashes,
                artifact_ids,
                tokens_per_variation,
                total_tokens: r.get_total_tokens(),
                variation_set_id: if variation_set_id.is_empty() { None } else { Some(variation_set_id) },
                summary: r.get_summary()?.to_string()?,
            }))
        }
        Which::OrpheusClassified(r) => {
            let r = r?;
            let mut classifications = Vec::new();
            for c in r.get_classifications()?.iter() {
                classifications.push(MidiClassification {
                    label: c.get_label()?.to_string()?,
                    confidence: c.get_confidence(),
                });
            }
            Ok(ToolResponse::OrpheusClassified(OrpheusClassifiedResponse {
                classifications,
            }))
        }

        // Audio Generation
        Which::AudioGenerated(r) => {
            let r = r?;
            let genre = r.get_genre()?.to_string()?;
            Ok(ToolResponse::AudioGenerated(AudioGeneratedResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                duration_seconds: r.get_duration_seconds(),
                sample_rate: r.get_sample_rate(),
                format: capnp_to_audio_format(r.get_format()?),
                genre: if genre.is_empty() { None } else { Some(genre) },
            }))
        }

        // Audio Analysis
        Which::BeatsAnalyzed(r) => {
            let r = r?;
            let beats: Vec<f64> = r.get_beats()?.iter().collect();
            let downbeats: Vec<f64> = r.get_downbeats()?.iter().collect();
            Ok(ToolResponse::BeatsAnalyzed(BeatsAnalyzedResponse {
                beats,
                downbeats,
                estimated_bpm: r.get_estimated_bpm(),
                confidence: r.get_confidence(),
            }))
        }
        Which::ClapAnalyzed(r) => {
            let r = r?;
            let embeddings: Vec<f32> = r.get_embeddings()?.iter().collect();
            let mut genre = Vec::new();
            for g in r.get_genre()?.iter() {
                genre.push(ClapClassification {
                    label: g.get_label()?.to_string()?,
                    score: g.get_score(),
                });
            }
            let mut mood = Vec::new();
            for m in r.get_mood()?.iter() {
                mood.push(ClapClassification {
                    label: m.get_label()?.to_string()?,
                    score: m.get_score(),
                });
            }
            let mut zero_shot = Vec::new();
            for z in r.get_zero_shot()?.iter() {
                zero_shot.push(ClapClassification {
                    label: z.get_label()?.to_string()?,
                    score: z.get_score(),
                });
            }
            let similarity = r.get_similarity();
            Ok(ToolResponse::ClapAnalyzed(ClapAnalyzedResponse {
                embeddings: if embeddings.is_empty() { None } else { Some(embeddings) },
                genre: if genre.is_empty() { None } else { Some(genre) },
                mood: if mood.is_empty() { None } else { Some(mood) },
                zero_shot: if zero_shot.is_empty() { None } else { Some(zero_shot) },
                similarity: if similarity == 0.0 { None } else { Some(similarity) },
            }))
        }
        Which::MidiInfo(r) => {
            let r = r?;
            let tempo_bpm = if r.get_has_tempo_bpm() {
                Some(r.get_tempo_bpm())
            } else {
                None
            };
            let mut tempo_changes = Vec::new();
            for tc in r.get_tempo_changes()?.iter() {
                tempo_changes.push(MidiTempoChange {
                    tick: tc.get_tick(),
                    bpm: tc.get_bpm(),
                });
            }
            let time_signature = if r.get_has_time_sig() {
                Some((r.get_time_sig_num(), r.get_time_sig_denom()))
            } else {
                None
            };
            Ok(ToolResponse::MidiInfo(MidiInfoResponse {
                tempo_bpm,
                tempo_changes,
                time_signature,
                duration_seconds: r.get_duration_seconds(),
                track_count: r.get_track_count() as usize,
                ppq: r.get_ppq(),
                note_count: r.get_note_count() as usize,
                format: r.get_format(),
            }))
        }

        // Garden/Transport
        Which::GardenStatus(r) => {
            let r = r?;
            Ok(ToolResponse::GardenStatus(GardenStatusResponse {
                state: capnp_to_transport_state(r.get_state()?),
                position_beats: r.get_position_beats(),
                tempo_bpm: r.get_tempo_bpm(),
                region_count: r.get_region_count() as usize,
            }))
        }
        Which::GardenRegions(r) => {
            let r = r?;
            let mut regions = Vec::new();
            for reg in r.get_regions()?.iter() {
                regions.push(GardenRegionInfo {
                    region_id: reg.get_region_id()?.to_string()?,
                    position: reg.get_position(),
                    duration: reg.get_duration(),
                    behavior_type: reg.get_behavior_type()?.to_string()?,
                    content_id: reg.get_content_id()?.to_string()?,
                });
            }
            Ok(ToolResponse::GardenRegions(GardenRegionsResponse {
                regions,
                count: r.get_count() as usize,
            }))
        }
        Which::GardenRegionCreated(r) => {
            let r = r?;
            Ok(ToolResponse::GardenRegionCreated(GardenRegionCreatedResponse {
                region_id: r.get_region_id()?.to_string()?,
                position: r.get_position(),
                duration: r.get_duration(),
            }))
        }
        Which::GardenQueryResult(r) => {
            let r = r?;
            let results_str = r.get_results()?.to_string()?;
            let results: Vec<serde_json::Value> = serde_json::from_str(&results_str).unwrap_or_default();
            Ok(ToolResponse::GardenQueryResult(GardenQueryResultResponse {
                results,
                count: r.get_count() as usize,
            }))
        }
        Which::GardenAudioStatus(r) => {
            let r = r?;
            let device_name = r.get_device_name()?.to_string()?;
            Ok(ToolResponse::GardenAudioStatus(GardenAudioStatusResponse {
                attached: r.get_attached(),
                device_name: if device_name.is_empty() { None } else { Some(device_name) },
                sample_rate: Some(r.get_sample_rate()),
                latency_frames: Some(r.get_latency_frames()),
                callbacks: r.get_callbacks(),
                samples_written: r.get_samples_written(),
                underruns: r.get_buffer_underruns(),
                monitor_reads: r.get_monitor_reads(),
                monitor_samples: r.get_monitor_samples(),
            }))
        }
        Which::GardenInputStatus(r) => {
            let r = r?;
            let device_name = r.get_device_name()?.to_string()?;
            let channels = r.get_channels();
            Ok(ToolResponse::GardenInputStatus(GardenInputStatusResponse {
                attached: r.get_attached(),
                device_name: if device_name.is_empty() { None } else { Some(device_name) },
                sample_rate: Some(r.get_sample_rate()),
                channels: if channels == 0 { None } else { Some(channels) },
                monitor_enabled: r.get_monitor_enabled(),
                monitor_gain: r.get_monitor_gain(),
                callbacks: r.get_callbacks(),
                samples_captured: r.get_samples_captured(),
                overruns: r.get_overruns(),
            }))
        }
        Which::GardenMonitorStatus(r) => {
            let r = r?;
            Ok(ToolResponse::GardenMonitorStatus(GardenMonitorStatusResponse {
                enabled: r.get_enabled(),
                gain: r.get_gain(),
            }))
        }
        Which::GardenAudioSnapshot(r) => {
            let r = r?;
            let samples_reader = r.get_samples()?;
            let mut samples = Vec::with_capacity(samples_reader.len() as usize);
            for i in 0..samples_reader.len() {
                samples.push(samples_reader.get(i));
            }
            Ok(ToolResponse::GardenAudioSnapshot(GardenAudioSnapshotResponse {
                sample_rate: r.get_sample_rate(),
                channels: r.get_channels(),
                format: r.get_format(),
                samples,
            }))
        }

        // Graph
        Which::GraphIdentity(r) => {
            let r = r?;
            Ok(ToolResponse::GraphIdentity(GraphIdentityResponse {
                id: r.get_id()?.to_string()?,
                name: r.get_name()?.to_string()?,
                created_at: r.get_created_at(),
            }))
        }
        Which::GraphIdentities(r) => {
            let r = r?;
            let mut identities = Vec::new();
            for id in r.get_identities()?.iter() {
                let tags: Vec<String> = id.get_tags()?.iter()
                    .filter_map(|t| t.ok().and_then(|s| s.to_string().ok()))
                    .collect();
                identities.push(GraphIdentityInfo {
                    id: id.get_id()?.to_string()?,
                    name: id.get_name()?.to_string()?,
                    tags,
                });
            }
            Ok(ToolResponse::GraphIdentities(GraphIdentitiesResponse {
                identities,
                count: r.get_count() as usize,
            }))
        }
        Which::GraphConnection(r) => {
            let r = r?;
            let transport = r.get_transport()?.to_string()?;
            Ok(ToolResponse::GraphConnection(GraphConnectionResponse {
                connection_id: r.get_connection_id()?.to_string()?,
                from_identity: r.get_from_identity()?.to_string()?,
                from_port: r.get_from_port()?.to_string()?,
                to_identity: r.get_to_identity()?.to_string()?,
                to_port: r.get_to_port()?.to_string()?,
                transport: if transport.is_empty() { None } else { Some(transport) },
            }))
        }
        Which::GraphTags(r) => {
            let r = r?;
            let mut tags = Vec::new();
            for t in r.get_tags()?.iter() {
                tags.push(GraphTagInfo {
                    namespace: t.get_namespace()?.to_string()?,
                    value: t.get_value()?.to_string()?,
                });
            }
            Ok(ToolResponse::GraphTags(GraphTagsResponse {
                identity_id: r.get_identity_id()?.to_string()?,
                tags,
            }))
        }
        Which::GraphContext(r) => {
            let r = r?;
            Ok(ToolResponse::GraphContext(GraphContextResponse {
                context: r.get_context()?.to_string()?,
                artifact_count: r.get_artifact_count() as usize,
                identity_count: r.get_identity_count() as usize,
            }))
        }
        Which::GraphQueryResult(r) => {
            let r = r?;
            let results_str = r.get_results()?.to_string()?;
            let results: Vec<serde_json::Value> = serde_json::from_str(&results_str).unwrap_or_default();
            Ok(ToolResponse::GraphQueryResult(GraphQueryResultResponse {
                results,
                count: r.get_count() as usize,
            }))
        }

        // Config
        Which::ConfigValue(r) => {
            let r = r?;
            let section = r.get_section()?.to_string()?;
            let key = r.get_key()?.to_string()?;
            let value_str = r.get_value()?.to_string()?;
            let value: ConfigValue = serde_json::from_str(&value_str).unwrap_or(ConfigValue::Null);
            Ok(ToolResponse::ConfigValue(ConfigValueResponse {
                section: if section.is_empty() { None } else { Some(section) },
                key: if key.is_empty() { None } else { Some(key) },
                value,
            }))
        }

        // Admin
        Which::ToolsList(r) => {
            let r = r?;
            let mut tools = Vec::new();
            for t in r.get_tools()?.iter() {
                let schema_str = t.get_input_schema()?.to_string()?;
                let input_schema: serde_json::Value = serde_json::from_str(&schema_str).unwrap_or(serde_json::Value::Null);
                tools.push(crate::ToolInfo {
                    name: t.get_name()?.to_string()?,
                    description: t.get_description()?.to_string()?,
                    input_schema,
                });
            }
            Ok(ToolResponse::ToolsList(ToolsListResponse {
                tools,
                count: r.get_count() as usize,
            }))
        }

        // Simple
        Which::Ack(r) => {
            let r = r?;
            Ok(ToolResponse::Ack(AckResponse {
                message: r.get_message()?.to_string()?,
            }))
        }
        Which::AnnotationAdded(r) => {
            let r = r?;
            Ok(ToolResponse::AnnotationAdded(AnnotationAddedResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                annotation_id: r.get_annotation_id()?.to_string()?,
            }))
        }

        // Vibeweaver
        Which::WeaveEval(r) => {
            let r = r?;
            let result = r.get_result()?.to_string()?;
            let stdout = r.get_stdout()?.to_string()?;
            let stderr = r.get_stderr()?.to_string()?;
            Ok(ToolResponse::WeaveEval(WeaveEvalResponse {
                output_type: capnp_to_weave_output_type(r.get_output_type()?),
                result: if result.is_empty() { None } else { Some(result) },
                stdout: if stdout.is_empty() { None } else { Some(stdout) },
                stderr: if stderr.is_empty() { None } else { Some(stderr) },
            }))
        }
        Which::WeaveSession(r) => {
            let r = r?;
            let session = if r.has_session() {
                let s = r.get_session()?;
                let vibe = s.get_vibe()?.to_string()?;
                Some(WeaveSessionInfo {
                    id: s.get_id()?.to_string()?,
                    name: s.get_name()?.to_string()?,
                    vibe: if vibe.is_empty() { None } else { Some(vibe) },
                })
            } else {
                None
            };
            let message = r.get_message()?.to_string()?;
            Ok(ToolResponse::WeaveSession(WeaveSessionResponse {
                session,
                message: if message.is_empty() { None } else { Some(message) },
            }))
        }
        Which::WeaveReset(r) => {
            let r = r?;
            Ok(ToolResponse::WeaveReset(WeaveResetResponse {
                reset: r.get_reset(),
                message: r.get_message()?.to_string()?,
            }))
        }
        Which::WeaveHelp(r) => {
            let r = r?;
            let topic = r.get_topic()?.to_string()?;
            Ok(ToolResponse::WeaveHelp(WeaveHelpResponse {
                help: r.get_help()?.to_string()?,
                topic: if topic.is_empty() { None } else { Some(topic) },
            }))
        }

        Which::ScheduleResult(r) => {
            let r = r?;
            Ok(ToolResponse::Scheduled(ScheduledResponse {
                success: r.get_success(),
                message: r.get_message()?.to_string()?,
                region_id: r.get_region_id()?.to_string()?,
                position: r.get_position(),
                duration: r.get_duration(),
                artifact_id: r.get_artifact_id()?.to_string()?,
            }))
        }

        Which::ToolHelp(r) => {
            let r = r?;
            let topic = r.get_topic()?.to_string()?;
            Ok(ToolResponse::ToolHelp(ToolHelpResponse {
                help: r.get_help()?.to_string()?,
                topic: if topic.is_empty() { None } else { Some(topic) },
            }))
        }

        Which::AnalyzeResult(r) => {
            let r = r?;
            let artifact_id = r.get_artifact_id()?.to_string()?;
            Ok(ToolResponse::AnalyzeResult(AnalyzeResultResponse {
                content_hash: r.get_content_hash()?.to_string()?,
                results: serde_json::from_str(r.get_results()?.to_str()?).unwrap_or_default(),
                summary: r.get_summary()?.to_string()?,
                artifact_id: if artifact_id.is_empty() { None } else { Some(artifact_id) },
            }))
        }

        Which::ProjectResult(r) => {
            let r = r?;
            let duration = r.get_duration_seconds();
            let sample_rate = r.get_sample_rate();
            Ok(ToolResponse::ProjectResult(ProjectResultResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                projection_type: r.get_projection_type()?.to_string()?,
                duration_seconds: if duration > 0.0 { Some(duration) } else { None },
                sample_rate: if sample_rate > 0 { Some(sample_rate) } else { None },
            }))
        }

        Which::GraphBind(r) => {
            let r = r?;
            Ok(ToolResponse::GraphBind(GraphBindResponse {
                identity_id: r.get_identity_id()?.to_string()?,
                name: r.get_name()?.to_string()?,
                hints_count: r.get_hints_count() as usize,
            }))
        }

        Which::GraphTag(r) => {
            let r = r?;
            Ok(ToolResponse::GraphTag(GraphTagResponse {
                identity_id: r.get_identity_id()?.to_string()?,
                tag: r.get_tag()?.to_string()?,
            }))
        }

        Which::GraphConnect(r) => {
            let r = r?;
            Ok(ToolResponse::GraphConnect(GraphConnectResponse {
                from_identity: r.get_from_identity()?.to_string()?,
                from_port: r.get_from_port()?.to_string()?,
                to_identity: r.get_to_identity()?.to_string()?,
                to_port: r.get_to_port()?.to_string()?,
            }))
        }

        // Extended Job responses
        Which::JobPoll(r) => {
            let r = r?;
            let completed: Vec<String> = r.get_completed()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            let failed: Vec<String> = r.get_failed()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            let pending: Vec<String> = r.get_pending()?.iter()
                .filter_map(|s| s.ok().and_then(|t| t.to_string().ok()))
                .collect();
            Ok(ToolResponse::JobPoll(JobPollResponse {
                completed,
                failed,
                pending,
                reason: r.get_reason()?.to_string()?,
                elapsed_ms: r.get_elapsed_ms(),
            }))
        }
        Which::JobCancel(r) => {
            let r = r?;
            Ok(ToolResponse::JobCancel(JobCancelResponse {
                job_id: r.get_job_id()?.to_string()?,
                cancelled: r.get_cancelled(),
            }))
        }
        // Audio Conversion responses
        Which::AbcToMidi(r) => {
            let r = r?;
            Ok(ToolResponse::AbcToMidi(AbcToMidiResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
            }))
        }
        Which::MidiToWav(r) => {
            let r = r?;
            let duration_secs = r.get_duration_secs();
            Ok(ToolResponse::MidiToWav(MidiToWavResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                sample_rate: r.get_sample_rate(),
                duration_secs: if duration_secs > 0.0 { Some(duration_secs) } else { None },
            }))
        }

        // RAVE responses
        Which::RaveEncoded(r) => {
            let r = r?;
            let latent_shape: Vec<u32> = r.get_latent_shape()?.iter().collect();
            Ok(ToolResponse::RaveEncoded(RaveEncodedResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                latent_shape,
                latent_dim: r.get_latent_dim(),
                model: r.get_model()?.to_string()?,
                sample_rate: r.get_sample_rate(),
            }))
        }
        Which::RaveDecoded(r) => {
            let r = r?;
            Ok(ToolResponse::RaveDecoded(RaveDecodedResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                duration_seconds: r.get_duration_seconds(),
                sample_rate: r.get_sample_rate(),
                model: r.get_model()?.to_string()?,
            }))
        }
        Which::RaveReconstructed(r) => {
            let r = r?;
            Ok(ToolResponse::RaveReconstructed(RaveReconstructedResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                duration_seconds: r.get_duration_seconds(),
                sample_rate: r.get_sample_rate(),
                model: r.get_model()?.to_string()?,
            }))
        }
        Which::RaveGenerated(r) => {
            let r = r?;
            Ok(ToolResponse::RaveGenerated(RaveGeneratedResponse {
                artifact_id: r.get_artifact_id()?.to_string()?,
                content_hash: r.get_content_hash()?.to_string()?,
                duration_seconds: r.get_duration_seconds(),
                sample_rate: r.get_sample_rate(),
                model: r.get_model()?.to_string()?,
                temperature: r.get_temperature(),
            }))
        }
        Which::RaveStreamStarted(r) => {
            let r = r?;
            Ok(ToolResponse::RaveStreamStarted(RaveStreamStartedResponse {
                stream_id: r.get_stream_id()?.to_string()?,
                model: r.get_model()?.to_string()?,
                input_identity: r.get_input_identity()?.to_string()?,
                output_identity: r.get_output_identity()?.to_string()?,
                latency_ms: r.get_latency_ms(),
            }))
        }
        Which::RaveStreamStopped(r) => {
            let r = r?;
            Ok(ToolResponse::RaveStreamStopped(RaveStreamStoppedResponse {
                stream_id: r.get_stream_id()?.to_string()?,
                duration_seconds: r.get_duration_seconds(),
            }))
        }
        Which::RaveStreamStatus(r) => {
            let r = r?;
            Ok(ToolResponse::RaveStreamStatus(RaveStreamStatusResponse {
                stream_id: r.get_stream_id()?.to_string()?,
                running: r.get_running(),
                model: r.get_model()?.to_string()?,
                input_identity: r.get_input_identity()?.to_string()?,
                output_identity: r.get_output_identity()?.to_string()?,
                frames_processed: r.get_frames_processed(),
                latency_ms: r.get_latency_ms(),
            }))
        }
    }
}

fn capnp_to_job_state(state: responses_capnp::JobState) -> JobState {
    match state {
        responses_capnp::JobState::Pending => JobState::Pending,
        responses_capnp::JobState::Running => JobState::Running,
        responses_capnp::JobState::Complete => JobState::Complete,
        responses_capnp::JobState::Failed => JobState::Failed,
        responses_capnp::JobState::Cancelled => JobState::Cancelled,
    }
}

fn capnp_to_audio_format(format: responses_capnp::AudioFormat) -> AudioFormat {
    match format {
        responses_capnp::AudioFormat::Wav => AudioFormat::Wav,
        responses_capnp::AudioFormat::Mp3 => AudioFormat::Mp3,
        responses_capnp::AudioFormat::Flac => AudioFormat::Flac,
    }
}

fn capnp_to_transport_state(state: responses_capnp::TransportState) -> TransportState {
    match state {
        responses_capnp::TransportState::Stopped => TransportState::Stopped,
        responses_capnp::TransportState::Playing => TransportState::Playing,
        responses_capnp::TransportState::Paused => TransportState::Paused,
    }
}

fn capnp_to_weave_output_type(output_type: responses_capnp::WeaveOutputType) -> WeaveOutputType {
    match output_type {
        responses_capnp::WeaveOutputType::Expression => WeaveOutputType::Expression,
        responses_capnp::WeaveOutputType::Statement => WeaveOutputType::Statement,
    }
}
