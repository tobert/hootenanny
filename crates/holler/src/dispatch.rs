//! JSON → Typed Payload conversion
//!
//! This is the JSON boundary. MCP sends us tool name + JSON args,
//! we parse to typed Payload variants for ZMQ transport.
//!
//! hooteproto should have NO serde_json::Value in Payload variants.
//! All JSON parsing happens here in holler.

use anyhow::{Context, Result};
use hooteproto::Payload;
use hooteproto::request::{self, ToolRequest};
use serde::Deserialize;
use serde_json::Value;

/// Convert MCP tool call (name + JSON args) to typed Payload.
pub fn json_to_payload(name: &str, args: Value) -> Result<Payload> {
    match name {
        // === ABC Tools ===
        "abc_parse" => {
            let p: AbcParseArgs = serde_json::from_value(args).context("Invalid abc_parse arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AbcParse(request::AbcParseRequest { abc: p.abc })))
        }
        "abc_validate" => {
            let p: AbcValidateArgs = serde_json::from_value(args).context("Invalid abc_validate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AbcValidate(request::AbcValidateRequest { abc: p.abc })))
        }
        "abc_to_midi" => {
            let p: AbcToMidiArgs = serde_json::from_value(args).context("Invalid abc_to_midi arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AbcToMidi(request::AbcToMidiRequest {
                abc: p.abc,
                tempo_override: p.tempo_override,
                transpose: p.transpose,
                velocity: p.velocity,
                channel: p.channel,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "abc_transpose" => {
            let p: AbcTransposeArgs = serde_json::from_value(args).context("Invalid abc_transpose arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AbcTranspose(request::AbcTransposeRequest {
                abc: p.abc,
                semitones: p.semitones,
                target_key: p.target_key,
            })))
        }

        // === System Tools ===
        "storage_stats" => Ok(Payload::ToolRequest(ToolRequest::CasStats)),

        // === Playback Tools ===
        "status" => Ok(Payload::ToolRequest(ToolRequest::GardenStatus)),
        "play" => Ok(Payload::ToolRequest(ToolRequest::GardenPlay)),
        "pause" => Ok(Payload::ToolRequest(ToolRequest::GardenPause)),
        "stop" => Ok(Payload::ToolRequest(ToolRequest::GardenStop)),
        "seek" => {
            let p: GardenSeekArgs = serde_json::from_value(args).context("Invalid seek arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenSeek(request::GardenSeekRequest { beat: p.beat })))
        }
        "tempo" => {
            let p: GardenSetTempoArgs = serde_json::from_value(args).context("Invalid tempo arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenSetTempo(request::GardenSetTempoRequest { bpm: p.bpm })))
        }
        "garden_query" => {
            let p: GardenQueryArgs = serde_json::from_value(args).context("Invalid garden_query arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenQuery(request::GardenQueryRequest {
                query: p.query,
                variables: p.variables,
            })))
        }

        // === Timeline Tools ===
        "timeline_region_create" => {
            let p: GardenCreateRegionArgs = serde_json::from_value(args).context("Invalid timeline_region_create arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenCreateRegion(request::GardenCreateRegionRequest {
                position: p.position,
                duration: p.duration,
                behavior_type: p.behavior_type,
                content_id: p.content_id,
            })))
        }
        "timeline_region_delete" => {
            let p: GardenDeleteRegionArgs = serde_json::from_value(args).context("Invalid timeline_region_delete arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenDeleteRegion(request::GardenDeleteRegionRequest {
                region_id: p.region_id,
            })))
        }
        "timeline_region_move" => {
            let p: GardenMoveRegionArgs = serde_json::from_value(args).context("Invalid timeline_region_move arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenMoveRegion(request::GardenMoveRegionRequest {
                region_id: p.region_id,
                new_position: p.new_position,
            })))
        }
        "timeline_clear" => Ok(Payload::ToolRequest(ToolRequest::GardenClearRegions)),
        "timeline_region_list" => {
            let p: GardenGetRegionsArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GardenGetRegions(request::GardenGetRegionsRequest {
                start: p.start,
                end: p.end,
            })))
        }

        // === Audio I/O Tools ===
        "audio_output_attach" => {
            let p: GardenAttachAudioArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GardenAttachAudio(request::GardenAttachAudioRequest {
                device_name: p.device_name,
                sample_rate: p.sample_rate,
                latency_frames: p.latency_frames,
            })))
        }
        "audio_output_detach" => Ok(Payload::ToolRequest(ToolRequest::GardenDetachAudio)),
        "audio_output_status" => Ok(Payload::ToolRequest(ToolRequest::GardenAudioStatus)),
        "audio_input_attach" => {
            let p: GardenAttachInputArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GardenAttachInput(request::GardenAttachInputRequest {
                device_name: p.device_name,
                sample_rate: p.sample_rate,
            })))
        }
        "audio_input_detach" => Ok(Payload::ToolRequest(ToolRequest::GardenDetachInput)),
        "audio_input_status" => Ok(Payload::ToolRequest(ToolRequest::GardenInputStatus)),
        "audio_monitor" => {
            let p: GardenSetMonitorArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GardenSetMonitor(request::GardenSetMonitorRequest {
                enabled: p.enabled,
                gain: p.gain,
            })))
        }
        "audio_capture" => {
            let p: AudioCaptureArgs = serde_json::from_value(args).context("Invalid audio_capture arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AudioCapture(request::AudioCaptureRequest {
                duration_seconds: p.duration_seconds.unwrap_or(5.0),
                source: p.source,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }

        // === MIDI I/O Tools (direct ALSA for low latency) ===
        "midi_list_ports" => Ok(Payload::ToolRequest(ToolRequest::MidiListPorts)),
        "midi_input_attach" => {
            let p: MidiAttachArgs = serde_json::from_value(args).context("Invalid midi_input_attach arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiInputAttach(request::MidiAttachRequest {
                port_pattern: p.port_pattern,
            })))
        }
        "midi_input_detach" => {
            let p: MidiDetachArgs = serde_json::from_value(args).context("Invalid midi_input_detach arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiInputDetach(request::MidiDetachRequest {
                port_pattern: p.port_pattern,
            })))
        }
        "midi_output_attach" => {
            let p: MidiAttachArgs = serde_json::from_value(args).context("Invalid midi_output_attach arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiOutputAttach(request::MidiAttachRequest {
                port_pattern: p.port_pattern,
            })))
        }
        "midi_output_detach" => {
            let p: MidiDetachArgs = serde_json::from_value(args).context("Invalid midi_output_detach arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiOutputDetach(request::MidiDetachRequest {
                port_pattern: p.port_pattern,
            })))
        }
        "midi_send" => {
            let p: MidiSendArgs = serde_json::from_value(args).context("Invalid midi_send arguments")?;
            let message: request::MidiMessageSpec = serde_json::from_value(p.message)
                .context("Invalid MIDI message specification")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiSend(request::MidiSendRequest {
                port_pattern: p.port_pattern,
                message,
            })))
        }
        "midi_status" => Ok(Payload::ToolRequest(ToolRequest::MidiStatus)),
        "midi_play" => {
            let p: MidiPlayArgs = serde_json::from_value(args).context("Invalid midi_play arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiPlay(request::MidiPlayRequest {
                artifact_id: p.artifact_id,
                port_pattern: p.port_pattern,
                start_beat: p.start_beat.unwrap_or(0.0),
            })))
        }
        "midi_stop" => {
            let p: MidiStopArgs = serde_json::from_value(args).context("Invalid midi_stop arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiStop(request::MidiStopRequest {
                region_id: p.region_id,
            })))
        }

        // === Job Tools ===
        "job_list" => {
            let p: JobListArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::JobList(request::JobListRequest { status: p.status })))
        }
        "job_poll" => {
            let p: JobPollArgs = serde_json::from_value(args).context("Invalid job_poll arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::JobPoll(request::JobPollRequest {
                job_ids: p.job_ids,
                timeout_ms: p.timeout_ms,
                mode: p.mode,
            })))
        }
        "job_cancel" => {
            let p: JobCancelArgs = serde_json::from_value(args).context("Invalid job_cancel arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::JobCancel(request::JobCancelRequest { job_id: p.job_id })))
        }
        "event_poll" => {
            let p: EventPollArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::EventPoll(request::EventPollRequest {
                cursor: p.cursor,
                since_ms: p.since_ms,
                types: p.types,
                timeout_ms: p.timeout_ms,
                limit: p.limit,
            })))
        }

        // === Orpheus Tools (model-specific) ===
        "orpheus_generate" => {
            let p: OrpheusGenerateArgs = serde_json::from_value(args).context("Invalid orpheus_generate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::OrpheusGenerate(request::OrpheusGenerateRequest {
                model: p.model,
                temperature: p.temperature,
                top_p: p.top_p,
                max_tokens: p.max_tokens,
                num_variations: p.num_variations,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "orpheus_continue" => {
            let p: OrpheusContinueArgs = serde_json::from_value(args).context("Invalid orpheus_continue arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::OrpheusContinue(request::OrpheusContinueRequest {
                input_hash: p.input_hash,
                model: p.model,
                temperature: p.temperature,
                top_p: p.top_p,
                max_tokens: p.max_tokens,
                num_variations: p.num_variations,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "orpheus_bridge" => {
            let p: OrpheusBridgeArgs = serde_json::from_value(args).context("Invalid orpheus_bridge arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::OrpheusBridge(request::OrpheusBridgeRequest {
                section_a_hash: p.section_a_hash,
                section_b_hash: p.section_b_hash,
                model: p.model,
                temperature: p.temperature,
                top_p: p.top_p,
                max_tokens: p.max_tokens,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "orpheus_loops" => {
            let p: OrpheusLoopsArgs = serde_json::from_value(args).context("Invalid orpheus_loops arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::OrpheusLoops(request::OrpheusLoopsRequest {
                temperature: p.temperature,
                top_p: p.top_p,
                max_tokens: p.max_tokens,
                num_variations: p.num_variations,
                seed_hash: p.seed_hash,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "orpheus_classify" | "midi_classify" => {
            let p: OrpheusClassifyArgs = serde_json::from_value(args).context("Invalid midi_classify arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::OrpheusClassify(request::OrpheusClassifyRequest { midi_hash: p.midi_hash })))
        }

        // === AsyncLong Tools (return job_id immediately) ===
        "musicgen_generate" => {
            let p: MusicgenGenerateArgs = serde_json::from_value(args).context("Invalid musicgen_generate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MusicgenGenerate(request::MusicgenGenerateRequest {
                prompt: p.prompt,
                duration: p.duration,
                temperature: p.temperature,
                top_k: p.top_k,
                top_p: p.top_p,
                guidance_scale: p.guidance_scale,
                do_sample: p.do_sample,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
                parent_id: p.parent_id,
                variation_set_id: p.variation_set_id,
            })))
        }
        "yue_generate" => {
            let p: YueGenerateArgs = serde_json::from_value(args).context("Invalid yue_generate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::YueGenerate(request::YueGenerateRequest {
                lyrics: p.lyrics,
                genre: p.genre,
                max_new_tokens: p.max_new_tokens,
                run_n_segments: p.run_n_segments,
                seed: p.seed,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
                parent_id: p.parent_id,
                variation_set_id: p.variation_set_id,
            })))
        }
        "beats_detect" => {
            let p: BeatthisAnalyzeArgs = serde_json::from_value(args).context("Invalid beats_detect arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::BeatthisAnalyze(request::BeatthisAnalyzeRequest {
                audio_hash: p.audio_hash,
                audio_path: p.audio_path,
                include_frames: p.include_frames.unwrap_or(false),
            })))
        }
        "audio_analyze" => {
            let p: ClapAnalyzeArgs = serde_json::from_value(args).context("Invalid audio_analyze arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::ClapAnalyze(request::ClapAnalyzeRequest {
                audio_hash: p.audio_hash,
                audio_b_hash: p.audio_b_hash,
                tasks: p.tasks.unwrap_or_else(|| vec!["classification".to_string()]),
                text_candidates: p.text_candidates.unwrap_or_default(),
                creator: p.creator,
                parent_id: p.parent_id,
            })))
        }
        "midi_info" => {
            let p: MidiInfoArgs = serde_json::from_value(args).context("Invalid midi_info arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiInfo(request::MidiInfoRequest {
                artifact_id: p.artifact_id,
                hash: p.hash,
            })))
        }
        "audio_info" => {
            let p: AudioInfoArgs = serde_json::from_value(args).context("Invalid audio_info arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AudioInfo(request::AudioInfoRequest {
                artifact_id: p.artifact_id,
                hash: p.hash,
            })))
        }

        // === AudioLDM2 Tools ===
        "audioldm2_generate" => {
            let p: Audioldm2GenerateArgs = serde_json::from_value(args).context("Invalid audioldm2_generate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::Audioldm2Generate(request::Audioldm2GenerateRequest {
                prompt: p.prompt,
                negative_prompt: p.negative_prompt,
                duration: p.duration,
                num_inference_steps: p.num_inference_steps,
                guidance_scale: p.guidance_scale,
                seed: p.seed,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
                parent_id: p.parent_id,
                variation_set_id: p.variation_set_id,
            })))
        }

        // === Anticipatory Music Transformer Tools ===
        "anticipatory_generate" => {
            let p: AnticipatoryGenerateArgs = serde_json::from_value(args).context("Invalid anticipatory_generate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AnticipatoryGenerate(request::AnticipatoryGenerateRequest {
                length_seconds: p.length_seconds,
                top_p: p.top_p,
                model_size: p.model_size,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
                parent_id: p.parent_id,
                variation_set_id: p.variation_set_id,
            })))
        }
        "anticipatory_continue" => {
            let p: AnticipatoryContinueArgs = serde_json::from_value(args).context("Invalid anticipatory_continue arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AnticipatoryContinue(request::AnticipatoryContinueRequest {
                input_hash: p.input_hash,
                length_seconds: p.length_seconds,
                prime_seconds: p.prime_seconds,
                top_p: p.top_p,
                model_size: p.model_size,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
                parent_id: p.parent_id,
                variation_set_id: p.variation_set_id,
            })))
        }
        "anticipatory_embed" => {
            let p: AnticipatoryEmbedArgs = serde_json::from_value(args).context("Invalid anticipatory_embed arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AnticipatoryEmbed(request::AnticipatoryEmbedRequest {
                input_hash: p.input_hash,
                model_size: p.model_size,
                embed_layer: p.embed_layer,
            })))
        }

        // === Demucs Tools ===
        "demucs_separate" => {
            let p: DemucsSeparateArgs = serde_json::from_value(args).context("Invalid demucs_separate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::DemucsSeparate(request::DemucsSeparateRequest {
                audio_hash: p.audio_hash,
                model: p.model,
                stems: p.stems.unwrap_or_default(),
                two_stems: p.two_stems,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
                parent_id: p.parent_id,
                variation_set_id: p.variation_set_id,
            })))
        }

        // === MIDI Analysis / Voice Separation ===
        "midi_analyze" => {
            let p: MidiAnalyzeArgs = serde_json::from_value(args).context("Invalid midi_analyze arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiAnalyze(request::MidiAnalyzeRequest {
                artifact_id: p.artifact_id,
                hash: p.hash,
                polyphony_threshold: p.polyphony_threshold,
                density_window_beats: p.density_window_beats,
            })))
        }
        "midi_voice_separate" => {
            let p: MidiVoiceSeparateArgs = serde_json::from_value(args).context("Invalid midi_voice_separate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiVoiceSeparate(request::MidiVoiceSeparateRequest {
                artifact_id: p.artifact_id,
                hash: p.hash,
                method: p.method,
                max_pitch_jump: p.max_pitch_jump,
                max_gap_beats: p.max_gap_beats,
                max_voices: p.max_voices,
                track_indices: p.track_indices.unwrap_or_default(),
            })))
        }
        "midi_stems_export" => {
            let p: MidiStemsExportArgs = serde_json::from_value(args).context("Invalid midi_stems_export arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiStemsExport(request::MidiStemsExportRequest {
                artifact_id: p.artifact_id,
                hash: p.hash,
                voice_data: p.voice_data,
                combined_file: p.combined_file.unwrap_or(false),
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
                parent_id: p.parent_id,
                variation_set_id: p.variation_set_id,
            })))
        }
        "midi_classify_voices" => {
            let p: MidiClassifyVoicesArgs = serde_json::from_value(args).context("Invalid midi_classify_voices arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiClassifyVoices(request::MidiClassifyVoicesRequest {
                artifact_id: p.artifact_id,
                hash: p.hash,
                voice_data: p.voice_data,
                use_ml: p.use_ml.unwrap_or(false),
            })))
        }

        // === Artifact Tools ===
        "artifact_upload" => {
            let p: ArtifactUploadArgs = serde_json::from_value(args).context("Invalid artifact_upload arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::ArtifactUpload(request::ArtifactUploadRequest {
                file_path: p.file_path,
                mime_type: p.mime_type,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "artifact_list" => {
            let p: ArtifactListArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::ArtifactList(request::ArtifactListRequest {
                tag: p.tag,
                creator: p.creator,
                limit: None,
            })))
        }
        "artifact_get" => {
            let p: ArtifactGetArgs = serde_json::from_value(args).context("Invalid artifact_get arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::ArtifactGet(request::ArtifactGetRequest { id: p.id })))
        }

        // === Graph Tools ===
        "graph_query" => {
            let p: GraphQueryArgs = serde_json::from_value(args).context("Invalid graph_query arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GraphQuery(request::GraphQueryRequest {
                query: p.query,
                variables: p.variables,
                limit: p.limit,
            })))
        }
        "graph_find" => {
            let p: GraphFindArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GraphFind(request::GraphFindRequest {
                name: p.name,
                tag_namespace: p.tag_namespace,
                tag_value: p.tag_value,
            })))
        }
        "graph_bind" => {
            let p: GraphBindArgs = serde_json::from_value(args).context("Invalid graph_bind arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GraphBind(request::GraphBindRequest {
                id: p.id,
                name: p.name,
                hints: p.hints.into_iter().map(|h| request::GraphHint {
                    kind: h.kind,
                    value: h.value,
                    confidence: h.confidence,
                }).collect(),
            })))
        }
        "graph_tag" => {
            let p: GraphTagArgs = serde_json::from_value(args).context("Invalid graph_tag arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GraphTag(request::GraphTagRequest {
                identity_id: p.identity_id,
                namespace: p.namespace,
                value: p.value,
            })))
        }
        "graph_connect" => {
            let p: GraphConnectArgs = serde_json::from_value(args).context("Invalid graph_connect arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GraphConnect(request::GraphConnectRequest {
                from_identity: p.from_identity,
                from_port: p.from_port,
                to_identity: p.to_identity,
                to_port: p.to_port,
                transport: p.transport,
            })))
        }
        "graph_context" => {
            let p: GraphContextArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GraphContext(request::GraphContextRequest {
                tag: p.tag,
                vibe_search: p.vibe_search,
                creator: p.creator,
                limit: p.limit,
                include_metadata: p.include_metadata,
                include_annotations: p.include_annotations,
                within_minutes: None,
            })))
        }
        "add_annotation" => {
            let p: AddAnnotationArgs = serde_json::from_value(args).context("Invalid add_annotation arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::AddAnnotation(request::AddAnnotationRequest {
                artifact_id: p.artifact_id,
                message: p.message,
                vibe: p.vibe,
                source: p.source,
            })))
        }

        // === Rendering Tools ===
        "convert_midi_to_wav" | "midi_render" => {
            let p: ConvertMidiToWavArgs = serde_json::from_value(args).context("Invalid midi_render arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::MidiToWav(request::MidiToWavRequest {
                input_hash: p.input_hash,
                soundfont_hash: p.soundfont_hash,
                sample_rate: p.sample_rate,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "soundfont_inspect" => {
            let p: SoundfontInspectArgs = serde_json::from_value(args).context("Invalid soundfont_inspect arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::SoundfontInspect(request::SoundfontInspectRequest {
                soundfont_hash: p.soundfont_hash,
                include_drum_map: p.include_drum_map.unwrap_or(false),
            })))
        }

        // === System Tools ===
        "config" => {
            let p: ConfigGetArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::ConfigGet(request::ConfigGetRequest {
                section: p.section,
                key: p.key,
            })))
        }

        // === Kernel Tools ===
        "kernel_eval" => {
            let p: WeaveEvalArgs = serde_json::from_value(args).context("Invalid kernel_eval arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::WeaveEval(request::WeaveEvalRequest {
                code: p.code,
            })))
        }
        "kernel_session" => {
            Ok(Payload::ToolRequest(ToolRequest::WeaveSession))
        }
        "kernel_reset" => {
            let p: WeaveResetArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::WeaveReset(request::WeaveResetRequest {
                clear_session: p.clear_session,
            })))
        }

        // === Tool Help ===
        "holler_help" | "get_tool_help" => {
            let p: GetToolHelpArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GetToolHelp(request::GetToolHelpRequest { topic: p.topic })))
        }

        // === RAVE Tools ===
        "rave_encode" => {
            let p: RaveEncodeArgs = serde_json::from_value(args).context("Invalid rave_encode arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::RaveEncode(request::RaveEncodeRequest {
                audio_hash: p.audio_hash,
                model: p.model,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "rave_decode" => {
            let p: RaveDecodeArgs = serde_json::from_value(args).context("Invalid rave_decode arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::RaveDecode(request::RaveDecodeRequest {
                latent_hash: p.latent_hash,
                latent_shape: p.latent_shape,
                model: p.model,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "rave_reconstruct" => {
            let p: RaveReconstructArgs = serde_json::from_value(args).context("Invalid rave_reconstruct arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::RaveReconstruct(request::RaveReconstructRequest {
                audio_hash: p.audio_hash,
                model: p.model,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "rave_generate" => {
            let p: RaveGenerateArgs = serde_json::from_value(args).context("Invalid rave_generate arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::RaveGenerate(request::RaveGenerateRequest {
                model: p.model,
                duration_seconds: p.duration_seconds,
                temperature: p.temperature,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })))
        }
        "rave_stream_start" => {
            let p: RaveStreamStartArgs = serde_json::from_value(args).context("Invalid rave_stream_start arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::RaveStreamStart(request::RaveStreamStartRequest {
                model: p.model,
                input_identity: p.input_identity,
                output_identity: p.output_identity,
                buffer_size: p.buffer_size,
            })))
        }
        "rave_stream_stop" => {
            let p: RaveStreamStopArgs = serde_json::from_value(args).context("Invalid rave_stream_stop arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::RaveStreamStop(request::RaveStreamStopRequest {
                stream_id: p.stream_id,
            })))
        }
        "rave_stream_status" => {
            let p: RaveStreamStatusArgs = serde_json::from_value(args).context("Invalid rave_stream_status arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::RaveStreamStatus(request::RaveStreamStatusRequest {
                stream_id: p.stream_id,
            })))
        }

        // === Fallback: Unknown tool ===
        _ => anyhow::bail!("Unknown tool: {}. All tools must have typed dispatch.", name),
    }
}

// ============================================================================
// Argument structs (MCP-shaped, JSON-friendly)
// These mirror hootenanny's api::schema types but live in holler.
// ============================================================================

#[derive(Debug, Deserialize)]
struct AbcParseArgs {
    abc: String,
}

#[derive(Debug, Deserialize)]
struct AbcValidateArgs {
    abc: String,
}

#[derive(Debug, Deserialize)]
struct AbcToMidiArgs {
    abc: String,
    tempo_override: Option<u16>,
    transpose: Option<i8>,
    velocity: Option<u8>,
    channel: Option<u8>,
    variation_set_id: Option<String>,
    parent_id: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AbcTransposeArgs {
    abc: String,
    semitones: Option<i8>,
    target_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GardenSeekArgs {
    beat: f64,
}

#[derive(Debug, Deserialize)]
struct GardenSetTempoArgs {
    bpm: f64,
}

#[derive(Debug, Deserialize)]
struct GardenQueryArgs {
    query: String,
    variables: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GardenCreateRegionArgs {
    position: f64,
    duration: f64,
    behavior_type: String,
    content_id: String,
}

#[derive(Debug, Deserialize)]
struct GardenDeleteRegionArgs {
    region_id: String,
}

#[derive(Debug, Deserialize)]
struct GardenMoveRegionArgs {
    region_id: String,
    new_position: f64,
}

#[derive(Debug, Default, Deserialize)]
struct GardenGetRegionsArgs {
    start: Option<f64>,
    end: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct GardenAttachAudioArgs {
    device_name: Option<String>,
    sample_rate: Option<u32>,
    latency_frames: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct GardenAttachInputArgs {
    device_name: Option<String>,
    sample_rate: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct AudioCaptureArgs {
    duration_seconds: Option<f32>,
    source: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct GardenSetMonitorArgs {
    enabled: Option<bool>,
    gain: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct JobListArgs {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JobPollArgs {
    job_ids: Vec<String>,
    timeout_ms: u64,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JobCancelArgs {
    job_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct EventPollArgs {
    cursor: Option<u64>,
    since_ms: Option<u64>,
    types: Option<Vec<String>>,
    timeout_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct OrpheusGenerateArgs {
    model: Option<String>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    num_variations: Option<u32>,
    variation_set_id: Option<String>,
    parent_id: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrpheusContinueArgs {
    input_hash: String,
    model: Option<String>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    num_variations: Option<u32>,
    variation_set_id: Option<String>,
    parent_id: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrpheusBridgeArgs {
    section_a_hash: String,
    section_b_hash: Option<String>,
    model: Option<String>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    variation_set_id: Option<String>,
    parent_id: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OrpheusLoopsArgs {
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    num_variations: Option<u32>,
    seed_hash: Option<String>,
    variation_set_id: Option<String>,
    parent_id: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrpheusClassifyArgs {
    midi_hash: String,
}

#[derive(Debug, Default, Deserialize)]
struct MusicgenGenerateArgs {
    prompt: Option<String>,
    duration: Option<f32>,
    temperature: Option<f32>,
    top_k: Option<u32>,
    top_p: Option<f32>,
    guidance_scale: Option<f32>,
    do_sample: Option<bool>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
    parent_id: Option<String>,
    variation_set_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YueGenerateArgs {
    lyrics: String,
    genre: Option<String>,
    max_new_tokens: Option<u32>,
    run_n_segments: Option<u32>,
    seed: Option<u64>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
    parent_id: Option<String>,
    variation_set_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct BeatthisAnalyzeArgs {
    audio_hash: Option<String>,
    audio_path: Option<String>,
    include_frames: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ClapAnalyzeArgs {
    audio_hash: String,
    audio_b_hash: Option<String>,
    tasks: Option<Vec<String>>,
    text_candidates: Option<Vec<String>>,
    creator: Option<String>,
    parent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MidiInfoArgs {
    artifact_id: Option<String>,
    hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AudioInfoArgs {
    artifact_id: Option<String>,
    hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArtifactUploadArgs {
    file_path: String,
    mime_type: String,
    variation_set_id: Option<String>,
    parent_id: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ArtifactListArgs {
    tag: Option<String>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArtifactGetArgs {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GraphQueryArgs {
    query: String,
    variables: Option<serde_json::Value>,
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct GraphFindArgs {
    name: Option<String>,
    tag_namespace: Option<String>,
    tag_value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphBindArgs {
    id: String,
    name: String,
    #[serde(default)]
    hints: Vec<GraphHintArgs>,
}

#[derive(Debug, Deserialize)]
struct GraphHintArgs {
    kind: String,
    value: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
}

fn default_confidence() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
struct GraphTagArgs {
    identity_id: String,
    namespace: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct GraphConnectArgs {
    from_identity: String,
    from_port: String,
    to_identity: String,
    to_port: String,
    transport: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct GraphContextArgs {
    tag: Option<String>,
    vibe_search: Option<String>,
    creator: Option<String>,
    limit: Option<usize>,
    #[serde(default)]
    include_metadata: bool,
    #[serde(default = "default_true")]
    include_annotations: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct AddAnnotationArgs {
    artifact_id: String,
    message: String,
    vibe: Option<String>,
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConvertMidiToWavArgs {
    input_hash: String,
    soundfont_hash: String,
    sample_rate: Option<u32>,
    variation_set_id: Option<String>,
    parent_id: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SoundfontInspectArgs {
    soundfont_hash: String,
    include_drum_map: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigGetArgs {
    section: Option<String>,
    key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct GetToolHelpArgs {
    topic: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WeaveEvalArgs {
    code: String,
}

#[derive(Debug, Default, Deserialize)]
struct WeaveResetArgs {
    #[serde(default)]
    clear_session: bool,
}

// === RAVE Args ===

#[derive(Debug, Deserialize)]
struct RaveEncodeArgs {
    audio_hash: String,
    model: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RaveDecodeArgs {
    latent_hash: String,
    latent_shape: Vec<u32>,
    model: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RaveReconstructArgs {
    audio_hash: String,
    model: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RaveGenerateArgs {
    model: Option<String>,
    duration_seconds: Option<f32>,
    temperature: Option<f32>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RaveStreamStartArgs {
    model: Option<String>,
    input_identity: String,
    output_identity: String,
    buffer_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RaveStreamStopArgs {
    stream_id: String,
}

#[derive(Debug, Deserialize)]
struct RaveStreamStatusArgs {
    stream_id: String,
}

// === MIDI I/O Args ===
#[derive(Debug, Default, Deserialize)]
struct MidiAttachArgs {
    port_pattern: String,
}

#[derive(Debug, Default, Deserialize)]
struct MidiDetachArgs {
    port_pattern: String,
}

#[derive(Debug, Default, Deserialize)]
struct MidiSendArgs {
    port_pattern: Option<String>,
    message: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct MidiPlayArgs {
    artifact_id: String,
    port_pattern: Option<String>,
    start_beat: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct MidiStopArgs {
    region_id: String,
}

// === AudioLDM2 Args ===

#[derive(Debug, Default, Deserialize)]
struct Audioldm2GenerateArgs {
    prompt: Option<String>,
    negative_prompt: Option<String>,
    duration: Option<f32>,
    num_inference_steps: Option<u32>,
    guidance_scale: Option<f32>,
    seed: Option<u64>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
    parent_id: Option<String>,
    variation_set_id: Option<String>,
}

// === Anticipatory Music Transformer Args ===

#[derive(Debug, Default, Deserialize)]
struct AnticipatoryGenerateArgs {
    length_seconds: Option<f32>,
    top_p: Option<f32>,
    model_size: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
    parent_id: Option<String>,
    variation_set_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnticipatoryContinueArgs {
    input_hash: String,
    length_seconds: Option<f32>,
    prime_seconds: Option<f32>,
    top_p: Option<f32>,
    model_size: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
    parent_id: Option<String>,
    variation_set_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnticipatoryEmbedArgs {
    input_hash: String,
    model_size: Option<String>,
    embed_layer: Option<i32>,
}

// === Demucs Args ===

#[derive(Debug, Deserialize)]
struct DemucsSeparateArgs {
    audio_hash: String,
    model: Option<String>,
    stems: Option<Vec<String>>,
    two_stems: Option<String>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
    parent_id: Option<String>,
    variation_set_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MidiAnalyzeArgs {
    artifact_id: Option<String>,
    hash: Option<String>,
    polyphony_threshold: Option<f64>,
    density_window_beats: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct MidiVoiceSeparateArgs {
    artifact_id: Option<String>,
    hash: Option<String>,
    method: Option<String>,
    max_pitch_jump: Option<u8>,
    max_gap_beats: Option<f64>,
    max_voices: Option<u8>,
    track_indices: Option<Vec<u16>>,
}

#[derive(Debug, Deserialize)]
struct MidiStemsExportArgs {
    artifact_id: Option<String>,
    hash: Option<String>,
    voice_data: String,
    combined_file: Option<bool>,
    tags: Option<Vec<String>>,
    creator: Option<String>,
    parent_id: Option<String>,
    variation_set_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MidiClassifyVoicesArgs {
    artifact_id: Option<String>,
    hash: Option<String>,
    voice_data: String,
    use_ml: Option<bool>,
}