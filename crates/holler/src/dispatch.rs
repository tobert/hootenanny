//! JSON → Typed Payload conversion
//!
//! This is the JSON boundary. MCP sends us tool name + JSON args,
//! we parse to typed Payload variants for ZMQ transport.
//!
//! hooteproto should have NO serde_json::Value in Payload variants.
//! All JSON parsing happens here in holler.

use anyhow::{Context, Result};
use hooteproto::Payload;
use serde::Deserialize;
use serde_json::Value;

/// Convert MCP tool call (name + JSON args) to typed Payload.
///
/// This is where JSON parsing happens. hooteproto Payload variants
/// should be typed, not contain serde_json::Value.
pub fn json_to_payload(name: &str, args: Value) -> Result<Payload> {
    match name {
        // === ABC Tools ===
        "abc_parse" => {
            let p: AbcParseArgs = serde_json::from_value(args)
                .context("Invalid abc_parse arguments")?;
            Ok(Payload::AbcParse { abc: p.abc })
        }
        "abc_validate" => {
            let p: AbcValidateArgs = serde_json::from_value(args)
                .context("Invalid abc_validate arguments")?;
            Ok(Payload::AbcValidate { abc: p.abc })
        }
        "abc_to_midi" => {
            let p: AbcToMidiArgs = serde_json::from_value(args)
                .context("Invalid abc_to_midi arguments")?;
            Ok(Payload::AbcToMidi {
                abc: p.abc,
                tempo_override: p.tempo_override,
                transpose: p.transpose,
                velocity: p.velocity,
                channel: p.channel,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })
        }
        "abc_transpose" => {
            let p: AbcTransposeArgs = serde_json::from_value(args)
                .context("Invalid abc_transpose arguments")?;
            Ok(Payload::AbcTranspose {
                abc: p.abc,
                semitones: p.semitones,
                target_key: p.target_key,
            })
        }

        // === CAS Tools ===
        "cas_store" => {
            let p: CasStoreArgs = serde_json::from_value(args)
                .context("Invalid cas_store arguments")?;
            let data = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                &p.content_base64,
            )
            .context("Invalid base64 in content_base64")?;
            Ok(Payload::CasStore {
                data,
                mime_type: p.mime_type,
            })
        }
        "cas_inspect" => {
            let p: CasInspectArgs = serde_json::from_value(args)
                .context("Invalid cas_inspect arguments")?;
            Ok(Payload::CasInspect { hash: p.hash })
        }
        "cas_upload_file" => {
            let p: CasUploadFileArgs = serde_json::from_value(args)
                .context("Invalid cas_upload_file arguments")?;
            Ok(Payload::CasUploadFile {
                file_path: p.file_path,
                mime_type: p.mime_type,
            })
        }

        // === Garden Tools ===
        "garden_status" => Ok(Payload::GardenStatus),
        "garden_play" => Ok(Payload::GardenPlay),
        "garden_pause" => Ok(Payload::GardenPause),
        "garden_stop" => Ok(Payload::GardenStop),
        "garden_seek" => {
            let p: GardenSeekArgs = serde_json::from_value(args)
                .context("Invalid garden_seek arguments")?;
            Ok(Payload::GardenSeek { beat: p.beat })
        }
        "garden_set_tempo" => {
            let p: GardenSetTempoArgs = serde_json::from_value(args)
                .context("Invalid garden_set_tempo arguments")?;
            Ok(Payload::GardenSetTempo { bpm: p.bpm })
        }
        "garden_query" => {
            let p: GardenQueryArgs = serde_json::from_value(args)
                .context("Invalid garden_query arguments")?;
            // GardenQuery keeps JSON for Trustfall variables (exception to the rule)
            Ok(Payload::GardenQuery {
                query: p.query,
                variables: p.variables,
            })
        }

        // === Job Tools ===
        "job_status" => {
            let p: JobStatusArgs = serde_json::from_value(args)
                .context("Invalid job_status arguments")?;
            Ok(Payload::JobStatus { job_id: p.job_id })
        }
        "job_list" => {
            let p: JobListArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::JobList { status: p.status })
        }
        "job_poll" => {
            let p: JobPollArgs = serde_json::from_value(args)
                .context("Invalid job_poll arguments")?;
            let mode = match p.mode.as_deref() {
                Some("all") => hooteproto::PollMode::All,
                _ => hooteproto::PollMode::Any,
            };
            Ok(Payload::JobPoll {
                job_ids: p.job_ids,
                timeout_ms: p.timeout_ms,
                mode,
            })
        }
        "job_cancel" => {
            let p: JobCancelArgs = serde_json::from_value(args)
                .context("Invalid job_cancel arguments")?;
            Ok(Payload::JobCancel { job_id: p.job_id })
        }
        "job_sleep" => {
            let p: JobSleepArgs = serde_json::from_value(args)
                .context("Invalid job_sleep arguments")?;
            Ok(Payload::JobSleep {
                milliseconds: p.milliseconds,
            })
        }

        // === Orpheus Tools ===
        "sample" | "orpheus_generate" => {
            let p: OrpheusGenerateArgs = serde_json::from_value(args)
                .context("Invalid orpheus_generate arguments")?;
            Ok(Payload::OrpheusGenerate {
                model: p.model,
                temperature: p.temperature,
                top_p: p.top_p,
                max_tokens: p.max_tokens,
                num_variations: p.num_variations,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })
        }
        "extend" | "orpheus_continue" => {
            let p: OrpheusContinueArgs = serde_json::from_value(args)
                .context("Invalid orpheus_continue arguments")?;
            Ok(Payload::OrpheusContinue {
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
            })
        }
        "bridge" | "orpheus_bridge" => {
            let p: OrpheusBridgeArgs = serde_json::from_value(args)
                .context("Invalid orpheus_bridge arguments")?;
            Ok(Payload::OrpheusBridge {
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
            })
        }
        "orpheus_loops" => {
            let p: OrpheusLoopsArgs = serde_json::from_value(args)
                .context("Invalid orpheus_loops arguments")?;
            Ok(Payload::OrpheusLoops {
                temperature: p.temperature,
                top_p: p.top_p,
                max_tokens: p.max_tokens,
                num_variations: p.num_variations,
                seed_hash: p.seed_hash,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })
        }
        "orpheus_classify" => {
            let p: OrpheusClassifyArgs = serde_json::from_value(args)
                .context("Invalid orpheus_classify arguments")?;
            Ok(Payload::OrpheusClassify {
                midi_hash: p.midi_hash,
            })
        }

        // === Artifact Tools ===
        "artifact_upload" => {
            let p: ArtifactUploadArgs = serde_json::from_value(args)
                .context("Invalid artifact_upload arguments")?;
            Ok(Payload::ArtifactUpload {
                file_path: p.file_path,
                mime_type: p.mime_type,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })
        }
        "artifact_list" => {
            let p: ArtifactListArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ArtifactList {
                tag: p.tag,
                creator: p.creator,
            })
        }
        "artifact_get" => {
            let p: ArtifactGetArgs = serde_json::from_value(args)
                .context("Invalid artifact_get arguments")?;
            Ok(Payload::ArtifactGet { id: p.id })
        }

        // === Graph Tools ===
        "graph_query" => {
            let p: GraphQueryArgs = serde_json::from_value(args)
                .context("Invalid graph_query arguments")?;
            // GraphQuery keeps JSON for Trustfall variables (exception to the rule)
            Ok(Payload::GraphQuery {
                query: p.query,
                variables: p.variables.unwrap_or_default(),
                limit: p.limit,
            })
        }
        "graph_find" => {
            let p: GraphFindArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::GraphFind {
                name: p.name,
                tag_namespace: p.tag_namespace,
                tag_value: p.tag_value,
            })
        }

        // === MIDI/Audio Tools ===
        "project" | "convert_midi_to_wav" => {
            let p: ConvertMidiToWavArgs = serde_json::from_value(args)
                .context("Invalid convert_midi_to_wav arguments")?;
            Ok(Payload::ConvertMidiToWav {
                input_hash: p.input_hash,
                soundfont_hash: p.soundfont_hash,
                sample_rate: p.sample_rate,
                variation_set_id: p.variation_set_id,
                parent_id: p.parent_id,
                tags: p.tags.unwrap_or_default(),
                creator: p.creator,
            })
        }
        "soundfont_inspect" => {
            let p: SoundfontInspectArgs = serde_json::from_value(args)
                .context("Invalid soundfont_inspect arguments")?;
            Ok(Payload::SoundfontInspect {
                soundfont_hash: p.soundfont_hash,
                include_drum_map: p.include_drum_map.unwrap_or(false),
            })
        }

        // === Config Tools ===
        "config_get" => {
            let p: ConfigGetArgs = serde_json::from_value(args).unwrap_or_default();
            Ok(Payload::ConfigGet {
                section: p.section,
                key: p.key,
            })
        }

        // === Tool Discovery ===
        "list_tools" => Ok(Payload::ListTools),

        // === Fallback: Unknown tool ===
        // TODO: Remove this fallback once all tools are typed
        _ => Ok(Payload::ToolCall {
            name: name.to_string(),
            args,
        }),
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
