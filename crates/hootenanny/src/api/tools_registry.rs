//! Tool registry - list of all available tools
//!
//! This module provides tool metadata for discovery.

use crate::api::schema::*;
use hooteproto::ToolInfo;
use schemars::JsonSchema;
use serde_json::Value;

/// Helper to generate JSON schema for a type
fn schema_for<T: JsonSchema>() -> Value {
    let settings = schemars::generate::SchemaSettings::draft07().with(|s| {
        s.inline_subschemas = true;
    });
    let gen = settings.into_generator();
    let schema = gen.into_root_schema_for::<T>();
    serde_json::to_value(schema).unwrap_or_default()
}

/// List all tools supported by hootenanny
pub fn list_tools() -> Vec<ToolInfo> {
    vec![
        // CAS Tools
        ToolInfo {
            name: "cas_store".to_string(),
            description: "Store raw content in CAS".to_string(),
            input_schema: schema_for::<CasStoreRequest>(),
        },
        ToolInfo {
            name: "cas_inspect".to_string(),
            description: "Inspect content in CAS".to_string(),
            input_schema: schema_for::<CasInspectRequest>(),
        },
        ToolInfo {
            name: "cas_stats".to_string(),
            description: "Get CAS storage statistics".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "cas_upload_file".to_string(),
            description: "Upload file from disk to CAS".to_string(),
            input_schema: schema_for::<UploadFileRequest>(),
        },
        // Artifact Tools
        ToolInfo {
            name: "artifact_upload".to_string(),
            description: "Upload file and create artifact".to_string(),
            input_schema: schema_for::<ArtifactUploadRequest>(),
        },
        ToolInfo {
            name: "artifact_list".to_string(),
            description: "List artifacts".to_string(),
            input_schema: schema_for::<ArtifactListRequest>(),
        },
        ToolInfo {
            name: "artifact_get".to_string(),
            description: "Get artifact by ID".to_string(),
            input_schema: schema_for::<ArtifactGetRequest>(),
        },
        // SoundFont Tools
        ToolInfo {
            name: "soundfont_inspect".to_string(),
            description: "Inspect SoundFont presets".to_string(),
            input_schema: schema_for::<SoundfontInspectRequest>(),
        },
        // Job Tools
        ToolInfo {
            name: "job_status".to_string(),
            description: "Get status of a job".to_string(),
            input_schema: schema_for::<GetJobStatusRequest>(),
        },
        ToolInfo {
            name: "job_list".to_string(),
            description: "List all jobs".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "job_cancel".to_string(),
            description: "Cancel a running job".to_string(),
            input_schema: schema_for::<CancelJobRequest>(),
        },
        ToolInfo {
            name: "job_poll".to_string(),
            description: "Poll for job completion".to_string(),
            input_schema: schema_for::<PollRequest>(),
        },
        ToolInfo {
            name: "job_sleep".to_string(),
            description: "Sleep for a duration".to_string(),
            input_schema: schema_for::<SleepRequest>(),
        },
        // Graph Tools
        ToolInfo {
            name: "graph_bind".to_string(),
            description: "Bind an identity to a device".to_string(),
            input_schema: schema_for::<GraphBindRequest>(),
        },
        ToolInfo {
            name: "graph_tag".to_string(),
            description: "Tag an identity".to_string(),
            input_schema: schema_for::<GraphTagRequest>(),
        },
        ToolInfo {
            name: "graph_connect".to_string(),
            description: "Connect two identities".to_string(),
            input_schema: schema_for::<GraphConnectRequest>(),
        },
        ToolInfo {
            name: "graph_find".to_string(),
            description: "Find identities".to_string(),
            input_schema: schema_for::<GraphFindRequest>(),
        },
        ToolInfo {
            name: "graph_context".to_string(),
            description: "Get graph context for LLM".to_string(),
            input_schema: schema_for::<GraphContextRequest>(),
        },
        ToolInfo {
            name: "graph_query".to_string(),
            description: "Execute Trustfall query on graph".to_string(),
            input_schema: schema_for::<GraphQueryRequest>(),
        },
        // ABC Tools
        ToolInfo {
            name: "abc_validate".to_string(),
            description: "Validate ABC notation".to_string(),
            input_schema: schema_for::<AbcValidateRequest>(),
        },
        // Garden Tools
        ToolInfo {
            name: "garden_status".to_string(),
            description: "Get chaosgarden status".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "garden_play".to_string(),
            description: "Start playback".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "garden_pause".to_string(),
            description: "Pause playback".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "garden_stop".to_string(),
            description: "Stop playback".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "garden_seek".to_string(),
            description: "Seek to position".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenSeekRequest>(),
        },
        ToolInfo {
            name: "garden_set_tempo".to_string(),
            description: "Set tempo".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenSetTempoRequest>(),
        },
        ToolInfo {
            name: "garden_query".to_string(),
            description: "Query garden state".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenQueryRequest>(),
        },
        // Config Tools
        ToolInfo {
            name: "config_get".to_string(),
            description: "Get configuration values".to_string(),
            input_schema: schema_for::<super::tools::config::ConfigGetRequest>(),
        },
        // Generation Tools
        ToolInfo {
            name: "sample".to_string(),
            description: "Generate MIDI from scratch".to_string(),
            input_schema: schema_for::<super::native::SampleRequest>(),
        },
        ToolInfo {
            name: "extend".to_string(),
            description: "Continue existing MIDI content".to_string(),
            input_schema: schema_for::<super::native::extend::ExtendRequest>(),
        },
        ToolInfo {
            name: "schedule".to_string(),
            description: "Schedule content on timeline".to_string(),
            input_schema: schema_for::<super::native::ScheduleRequest>(),
        },
        // AsyncLong Tools (return job_id immediately)
        ToolInfo {
            name: "musicgen_generate".to_string(),
            description: "Generate audio from text prompt using MusicGen".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Text prompt for generation" },
                    "duration": { "type": "number", "description": "Duration in seconds" },
                    "temperature": { "type": "number" },
                    "top_k": { "type": "integer" },
                    "top_p": { "type": "number" },
                    "guidance_scale": { "type": "number" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "yue_generate".to_string(),
            description: "Generate song from lyrics using YuE".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["lyrics"],
                "properties": {
                    "lyrics": { "type": "string", "description": "Song lyrics" },
                    "genre": { "type": "string" },
                    "max_new_tokens": { "type": "integer" },
                    "run_n_segments": { "type": "integer" },
                    "seed": { "type": "integer" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "beatthis_analyze".to_string(),
            description: "Analyze audio for beat detection".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "audio_hash": { "type": "string", "description": "CAS hash of audio" },
                    "audio_path": { "type": "string", "description": "Path to audio file" },
                    "include_frames": { "type": "boolean" }
                }
            }),
        },
        ToolInfo {
            name: "clap_analyze".to_string(),
            description: "Analyze audio with CLAP model".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["audio_hash"],
                "properties": {
                    "audio_hash": { "type": "string", "description": "CAS hash of audio" },
                    "audio_b_hash": { "type": "string", "description": "Optional second audio for comparison" },
                    "tasks": { "type": "array", "items": { "type": "string" } },
                    "text_candidates": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        // Vibeweaver Tools (Python Kernel)
        ToolInfo {
            name: "weave_eval".to_string(),
            description: "Execute Python code in vibeweaver kernel".to_string(),
            input_schema: schema_for::<WeaveEvalRequest>(),
        },
        ToolInfo {
            name: "weave_session".to_string(),
            description: "Get current vibeweaver session state".to_string(),
            input_schema: schema_for::<WeaveSessionRequest>(),
        },
        ToolInfo {
            name: "weave_reset".to_string(),
            description: "Reset vibeweaver kernel".to_string(),
            input_schema: schema_for::<WeaveResetRequest>(),
        },
        ToolInfo {
            name: "weave_help".to_string(),
            description: "Get vibeweaver help documentation".to_string(),
            input_schema: schema_for::<WeaveHelpRequest>(),
        },
    ]
}
