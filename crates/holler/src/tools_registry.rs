//! Tool registry - list of all available tools
//!
//! This module provides tool metadata for MCP discovery.
//! All schemas are manually written to ensure llama.cpp compatibility
//! (no `default` fields that llama.cpp cannot parse).

use crate::manual_schemas;
use hooteproto::ToolInfo;

/// List all tools supported by hootenanny
pub fn list_tools() -> Vec<ToolInfo> {
    vec![
        // ==========================================================================
        // Artifact Tools
        // ==========================================================================
        ToolInfo {
            name: "artifact_upload".to_string(),
            description: "Upload file and create artifact".to_string(),
            input_schema: manual_schemas::artifact_upload_request(),
        },
        ToolInfo {
            name: "artifact_list".to_string(),
            description: "List artifacts".to_string(),
            input_schema: manual_schemas::artifact_list_request(),
        },
        ToolInfo {
            name: "artifact_get".to_string(),
            description: "Get artifact by ID".to_string(),
            input_schema: manual_schemas::artifact_get_request(),
        },

        // ==========================================================================
        // Generation Tools
        // ==========================================================================
        ToolInfo {
            name: "orpheus_generate".to_string(),
            description: "Generate MIDI".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "temperature": { "type": "number" },
                    "top_p": { "type": "number" },
                    "max_tokens": { "type": "integer" },
                    "num_variations": { "type": "integer" },
                    "seed_hash": { "type": "string", "description": "Optional seed MIDI hash" },
                    "as_loop": { "type": "boolean", "description": "Generate as loop" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "orpheus_continue".to_string(),
            description: "Continue MIDI".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["input_hash"],
                "properties": {
                    "input_hash": { "type": "string", "description": "MIDI to continue from" },
                    "temperature": { "type": "number" },
                    "top_p": { "type": "number" },
                    "max_tokens": { "type": "integer" },
                    "num_variations": { "type": "integer" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "orpheus_bridge".to_string(),
            description: "Bridge sections".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["section_a_hash"],
                "properties": {
                    "section_a_hash": { "type": "string", "description": "First section" },
                    "section_b_hash": { "type": "string", "description": "Optional target section" },
                    "temperature": { "type": "number" },
                    "top_p": { "type": "number" },
                    "max_tokens": { "type": "integer" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },

        // ==========================================================================
        // Rendering Tools
        // ==========================================================================
        ToolInfo {
            name: "soundfont_inspect".to_string(),
            description: "List presets".to_string(),
            input_schema: manual_schemas::soundfont_inspect_request(),
        },
        ToolInfo {
            name: "midi_render".to_string(),
            description: "MIDI to audio".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["input_hash", "soundfont_hash"],
                "properties": {
                    "input_hash": { "type": "string", "description": "MIDI CAS hash" },
                    "soundfont_hash": { "type": "string", "description": "SoundFont CAS hash" },
                    "sample_rate": { "type": "integer" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },

        // ==========================================================================
        // Job Tools
        // ==========================================================================
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
            input_schema: manual_schemas::cancel_job_request(),
        },
        ToolInfo {
            name: "job_poll".to_string(),
            description: "Poll for job completion".to_string(),
            input_schema: manual_schemas::poll_request(),
        },

        // ==========================================================================
        // Event Polling
        // ==========================================================================
        ToolInfo {
            name: "event_poll".to_string(),
            description: "Poll for buffered broadcast events with cursor-based pagination".to_string(),
            input_schema: manual_schemas::event_poll_request(),
        },

        // ==========================================================================
        // Graph Tools
        // ==========================================================================
        ToolInfo {
            name: "graph_bind".to_string(),
            description: "Bind identity".to_string(),
            input_schema: manual_schemas::graph_bind_request(),
        },
        ToolInfo {
            name: "graph_tag".to_string(),
            description: "Tag identity".to_string(),
            input_schema: manual_schemas::graph_tag_request(),
        },
        ToolInfo {
            name: "graph_connect".to_string(),
            description: "Connect identities".to_string(),
            input_schema: manual_schemas::graph_connect_request(),
        },
        ToolInfo {
            name: "graph_find".to_string(),
            description: "Find identities".to_string(),
            input_schema: manual_schemas::graph_find_request(),
        },
        ToolInfo {
            name: "graph_context".to_string(),
            description: "LLM context".to_string(),
            input_schema: manual_schemas::graph_context_request(),
        },
        ToolInfo {
            name: "graph_query".to_string(),
            description: "Trustfall query".to_string(),
            input_schema: manual_schemas::graph_query_request(),
        },

        // ==========================================================================
        // ABC Tools
        // ==========================================================================
        ToolInfo {
            name: "abc_validate".to_string(),
            description: "Validate notation".to_string(),
            input_schema: manual_schemas::abc_validate_request(),
        },
        ToolInfo {
            name: "abc_to_midi".to_string(),
            description: "Convert to MIDI".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["abc"],
                "properties": {
                    "abc": { "type": "string", "description": "ABC notation" },
                    "tempo_override": { "type": "integer" },
                    "transpose": { "type": "integer" },
                    "velocity": { "type": "integer" },
                    "channel": { "type": "integer" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },

        // ==========================================================================
        // Playback Tools
        // ==========================================================================
        ToolInfo {
            name: "status".to_string(),
            description: "System status".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "play".to_string(),
            description: "Start playback".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "pause".to_string(),
            description: "Pause playback".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "stop".to_string(),
            description: "Stop playback".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "seek".to_string(),
            description: "Seek to beat".to_string(),
            input_schema: manual_schemas::garden_seek_request(),
        },
        ToolInfo {
            name: "tempo".to_string(),
            description: "Set BPM".to_string(),
            input_schema: manual_schemas::garden_set_tempo_request(),
        },
        ToolInfo {
            name: "garden_query".to_string(),
            description: "Trustfall query".to_string(),
            input_schema: manual_schemas::garden_query_request(),
        },

        // ==========================================================================
        // Audio I/O Tools
        // ==========================================================================
        ToolInfo {
            name: "audio_output_attach".to_string(),
            description: "Attach output".to_string(),
            input_schema: manual_schemas::garden_attach_audio_request(),
        },
        ToolInfo {
            name: "audio_output_detach".to_string(),
            description: "Detach output".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolInfo {
            name: "audio_output_status".to_string(),
            description: "Output status".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolInfo {
            name: "audio_monitor".to_string(),
            description: "Monitor gain".to_string(),
            input_schema: manual_schemas::garden_set_monitor_request(),
        },
        ToolInfo {
            name: "audio_input_attach".to_string(),
            description: "Attach input".to_string(),
            input_schema: manual_schemas::garden_attach_input_request(),
        },
        ToolInfo {
            name: "audio_input_detach".to_string(),
            description: "Detach input".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolInfo {
            name: "audio_input_status".to_string(),
            description: "Input status".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },

        // ==========================================================================
        // Timeline Tools
        // ==========================================================================
        ToolInfo {
            name: "timeline_region_create".to_string(),
            description: "Create region".to_string(),
            input_schema: manual_schemas::garden_create_region_request(),
        },
        ToolInfo {
            name: "timeline_region_delete".to_string(),
            description: "Delete region".to_string(),
            input_schema: manual_schemas::garden_delete_region_request(),
        },
        ToolInfo {
            name: "timeline_region_move".to_string(),
            description: "Move region".to_string(),
            input_schema: manual_schemas::garden_move_region_request(),
        },
        ToolInfo {
            name: "timeline_clear".to_string(),
            description: "Clear timeline".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolInfo {
            name: "timeline_region_list".to_string(),
            description: "List regions".to_string(),
            input_schema: manual_schemas::garden_get_regions_request(),
        },

        // ==========================================================================
        // System Tools
        // ==========================================================================
        ToolInfo {
            name: "config".to_string(),
            description: "Get config".to_string(),
            input_schema: manual_schemas::config_get_request(),
        },
        ToolInfo {
            name: "storage_stats".to_string(),
            description: "Storage statistics".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },

        // ==========================================================================
        // AsyncLong Tools (return job_id immediately)
        // ==========================================================================
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
            name: "beats_detect".to_string(),
            description: "Detect beats".to_string(),
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
            name: "audio_analyze".to_string(),
            description: "Audio embeddings".to_string(),
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
        ToolInfo {
            name: "midi_classify".to_string(),
            description: "Classify MIDI".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["midi_hash"],
                "properties": {
                    "midi_hash": { "type": "string", "description": "MIDI CAS hash" }
                }
            }),
        },
        ToolInfo {
            name: "midi_info".to_string(),
            description: "MIDI metadata".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact ID of MIDI file" },
                    "hash": { "type": "string", "description": "CAS hash of MIDI file (alternative to artifact_id)" }
                }
            }),
        },

        // ==========================================================================
        // Kernel Tools (Python)
        // ==========================================================================
        ToolInfo {
            name: "kernel_eval".to_string(),
            description: "Execute Python".to_string(),
            input_schema: manual_schemas::weave_eval_request(),
        },
        ToolInfo {
            name: "kernel_session".to_string(),
            description: "Session state".to_string(),
            input_schema: manual_schemas::weave_session_request(),
        },
        ToolInfo {
            name: "kernel_reset".to_string(),
            description: "Reset kernel".to_string(),
            input_schema: manual_schemas::weave_reset_request(),
        },

        // ==========================================================================
        // Help Tool
        // ==========================================================================
        ToolInfo {
            name: "help".to_string(),
            description: "Tool documentation".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "tool": { "type": "string", "description": "Tool name to get help for" },
                    "category": { "type": "string", "description": "Category to list (generation, abc, analysis, rendering, playback, timeline, audio, artifacts, jobs, system, kernel, graph)" }
                }
            }),
        },
    ]
}
