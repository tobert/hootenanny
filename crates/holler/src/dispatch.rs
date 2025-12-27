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

/// Preprocess JSON args to handle string-encoded nested objects.
fn preprocess_encoding_field(mut args: Value) -> Value {
    if let Some(obj) = args.as_object_mut() {
        // Handle encoding field (used by schedule, analyze, project, etc.)
        if let Some(encoding) = obj.get_mut("encoding") {
            if let Some(s) = encoding.as_str() {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    *encoding = parsed;
                }
            }
        }
        // Handle target field (used by project)
        if let Some(target) = obj.get_mut("target") {
            if let Some(s) = target.as_str() {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    *target = parsed;
                }
            }
        }
        // Handle seed field (used by sample)
        if let Some(seed) = obj.get_mut("seed") {
            if let Some(s) = seed.as_str() {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    *seed = parsed;
                }
            }
        }
        // Handle inference field (used by sample, extend, bridge)
        if let Some(inference) = obj.get_mut("inference") {
            if let Some(s) = inference.as_str() {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    *inference = parsed;
                }
            }
        }
    }
    args
}

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

        // === CAS Tools ===
        "cas_store" => {
            let p: CasStoreArgs = serde_json::from_value(args).context("Invalid cas_store arguments")?;
            let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &p.content_base64)
                .context("Invalid base64 in content_base64")?;
            Ok(Payload::ToolRequest(ToolRequest::CasStore(request::CasStoreRequest {
                data,
                mime_type: p.mime_type,
            })))
        }
        "cas_inspect" => {
            let p: CasInspectArgs = serde_json::from_value(args).context("Invalid cas_inspect arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::CasInspect(request::CasInspectRequest { hash: p.hash })))
        }
        "cas_upload_file" => {
            let p: CasUploadFileArgs = serde_json::from_value(args).context("Invalid cas_upload_file arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::CasUploadFile(request::CasUploadFileRequest {
                file_path: p.file_path,
                mime_type: p.mime_type,
            })))
        }
        "cas_stats" => Ok(Payload::ToolRequest(ToolRequest::CasStats)),

        // === Garden Tools ===
        "garden_status" => Ok(Payload::ToolRequest(ToolRequest::GardenStatus)),
        "garden_play" => Ok(Payload::ToolRequest(ToolRequest::GardenPlay)),
        "garden_pause" => Ok(Payload::ToolRequest(ToolRequest::GardenPause)),
        "garden_stop" => Ok(Payload::ToolRequest(ToolRequest::GardenStop)),
        "garden_seek" => {
            let p: GardenSeekArgs = serde_json::from_value(args).context("Invalid garden_seek arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenSeek(request::GardenSeekRequest { beat: p.beat })))
        }
        "garden_set_tempo" => {
            let p: GardenSetTempoArgs = serde_json::from_value(args).context("Invalid garden_set_tempo arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenSetTempo(request::GardenSetTempoRequest { bpm: p.bpm })))
        }
        "garden_query" => {
            let p: GardenQueryArgs = serde_json::from_value(args).context("Invalid garden_query arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenQuery(request::GardenQueryRequest {
                query: p.query,
                variables: p.variables,
            })))
        }
        "garden_emergency_pause" => Ok(Payload::ToolRequest(ToolRequest::GardenEmergencyPause)),
        "garden_create_region" => {
            let p: GardenCreateRegionArgs = serde_json::from_value(args).context("Invalid garden_create_region arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenCreateRegion(request::GardenCreateRegionRequest {
                position: p.position,
                duration: p.duration,
                behavior_type: p.behavior_type,
                content_id: p.content_id,
            })))
        }
        "garden_delete_region" => {
            let p: GardenDeleteRegionArgs = serde_json::from_value(args).context("Invalid garden_delete_region arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenDeleteRegion(request::GardenDeleteRegionRequest {
                region_id: p.region_id,
            })))
        }
        "garden_move_region" => {
            let p: GardenMoveRegionArgs = serde_json::from_value(args).context("Invalid garden_move_region arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::GardenMoveRegion(request::GardenMoveRegionRequest {
                region_id: p.region_id,
                new_position: p.new_position,
            })))
        }
        "garden_get_regions" => {
            let p: GardenGetRegionsArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GardenGetRegions(request::GardenGetRegionsRequest {
                start: p.start,
                end: p.end,
            })))
        }
        "garden_attach_audio" => {
            let p: GardenAttachAudioArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GardenAttachAudio(request::GardenAttachAudioRequest {
                device_name: p.device_name,
                sample_rate: p.sample_rate,
                latency_frames: p.latency_frames,
            })))
        }
        "garden_detach_audio" => Ok(Payload::ToolRequest(ToolRequest::GardenDetachAudio)),
        "garden_audio_status" => Ok(Payload::ToolRequest(ToolRequest::GardenAudioStatus)),
        "garden_attach_input" => {
            let p: GardenAttachInputArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GardenAttachInput(request::GardenAttachInputRequest {
                device_name: p.device_name,
                sample_rate: p.sample_rate,
            })))
        }
        "garden_detach_input" => Ok(Payload::ToolRequest(ToolRequest::GardenDetachInput)),
        "garden_input_status" => Ok(Payload::ToolRequest(ToolRequest::GardenInputStatus)),
        "garden_set_monitor" => {
            let p: GardenSetMonitorArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GardenSetMonitor(request::GardenSetMonitorRequest {
                enabled: p.enabled,
                gain: p.gain,
            })))
        }

        // === Job Tools ===
        "job_status" => {
            let p: JobStatusArgs = serde_json::from_value(args).context("Invalid job_status arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::JobStatus(request::JobStatusRequest { job_id: p.job_id })))
        }
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
        "job_sleep" => {
            let p: JobSleepArgs = serde_json::from_value(args).context("Invalid job_sleep arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::JobSleep(request::JobSleepRequest { milliseconds: p.milliseconds })))
        }

        // === Orpheus Tools ===
        "sample" | "orpheus_generate" => {
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
        "extend" | "orpheus_continue" => {
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
        "bridge" | "orpheus_bridge" => {
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
        "orpheus_classify" => {
            let p: OrpheusClassifyArgs = serde_json::from_value(args).context("Invalid orpheus_classify arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::OrpheusClassify(request::OrpheusClassifyRequest { midi_hash: p.midi_hash })))
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

        // === MIDI/Audio Tools ===
        "project" | "convert_midi_to_wav" => {
            let p: ConvertMidiToWavArgs = serde_json::from_value(args).context("Invalid convert_midi_to_wav arguments")?;
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

        // === Config Tools ===
        "config_get" => {
            let p: ConfigGetArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::ConfigGet(request::ConfigGetRequest {
                section: p.section,
                key: p.key,
            })))
        }

        // === Vibeweaver Tools ===
        "weave_eval" => {
            let p: WeaveEvalArgs = serde_json::from_value(args).context("Invalid weave_eval arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::WeaveEval(request::WeaveEvalRequest {
                code: p.code,
            })))
        }
        "weave_session" => {
            Ok(Payload::ToolRequest(ToolRequest::WeaveSession))
        }
        "weave_reset" => {
            let p: WeaveResetArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::WeaveReset(request::WeaveResetRequest {
                clear_session: p.clear_session,
            })))
        }
        "weave_help" => {
            let p: WeaveHelpArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::WeaveHelp(request::WeaveHelpRequest {
                topic: p.topic,
            })))
        }

        // === Tool Discovery ===
        "list_tools" => Ok(Payload::ToolRequest(ToolRequest::ListTools)),

        // === Tool Help ===
        "holler_help" | "get_tool_help" => {
            let p: GetToolHelpArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ToolRequest(ToolRequest::GetToolHelp(request::GetToolHelpRequest { topic: p.topic })))
        }

        // === Model-Native API ===
        "schedule" => {
            let args = preprocess_encoding_field(args);
            let p: ScheduleArgs = serde_json::from_value(args).context("Invalid schedule arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::Schedule(request::ScheduleRequest {
                encoding: p.encoding,
                at: p.at,
                duration: p.duration,
                gain: p.gain,
                rate: p.rate,
            })))
        }
        "analyze" => {
            let args = preprocess_encoding_field(args);
            let p: AnalyzeArgs = serde_json::from_value(args).context("Invalid analyze arguments")?;
            Ok(Payload::ToolRequest(ToolRequest::Analyze(request::AnalyzeRequest {
                encoding: p.encoding,
                tasks: p.tasks,
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
struct CasStoreArgs {
    content_base64: String,
    mime_type: String,
}

#[derive(Debug, Deserialize)]
struct CasInspectArgs {
    hash: String,
}

#[derive(Debug, Deserialize)]
struct CasUploadFileArgs {
    file_path: String,
    mime_type: String,
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
struct GardenSetMonitorArgs {
    enabled: Option<bool>,
    gain: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct JobStatusArgs {
    job_id: String,
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

#[derive(Debug, Deserialize)]
struct JobSleepArgs {
    milliseconds: u64,
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
struct ScheduleArgs {
    encoding: hooteproto::Encoding,
    at: f64,
    duration: Option<f64>,
    gain: Option<f64>,
    rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeArgs {
    encoding: hooteproto::Encoding,
    tasks: Vec<hooteproto::AnalysisTask>,
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

#[derive(Debug, Default, Deserialize)]
struct WeaveHelpArgs {
    topic: Option<String>,
}