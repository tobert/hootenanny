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
            input_schema: serde_json::json!({
                "type": "object",
                "description": "Returns unified system status with transport, audio, and MIDI subsystems. Response includes: transport (state, position_beats, tempo_bpm, region_count), audio_output (attached, device_name, sample_rate, latency_frames, callbacks, samples_written, underruns), audio_input (attached, device_name, sample_rate, channels, callbacks, samples_captured, overruns), monitor (enabled, gain), midi (inputs, outputs with port_name and messages count)."
            }),
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
        ToolInfo {
            name: "audio_capture".to_string(),
            description: "Capture audio from monitor input to CAS".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "duration_seconds": {
                        "type": "number",
                        "description": "Duration to capture in seconds (default: 5.0)"
                    },
                    "source": {
                        "type": "string",
                        "description": "Source to capture from: 'monitor' (default), 'timeline', 'mix'"
                    },
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Tags for the resulting artifact"
                    },
                    "creator": {
                        "type": "string",
                        "description": "Creator identifier"
                    }
                }
            }),
        },

        // ==========================================================================
        // MIDI I/O Tools (direct ALSA for low latency)
        // ==========================================================================
        ToolInfo {
            name: "midi_list_ports".to_string(),
            description: "List available MIDI ports".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolInfo {
            name: "midi_input_attach".to_string(),
            description: "Attach MIDI input port".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["port_pattern"],
                "properties": {
                    "port_pattern": {
                        "type": "string",
                        "description": "Port name pattern to match (e.g., 'NiftyCASE', 'BRAINS')"
                    }
                }
            }),
        },
        ToolInfo {
            name: "midi_input_detach".to_string(),
            description: "Detach MIDI input port".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["port_pattern"],
                "properties": {
                    "port_pattern": {
                        "type": "string",
                        "description": "Port name pattern to match"
                    }
                }
            }),
        },
        ToolInfo {
            name: "midi_output_attach".to_string(),
            description: "Attach MIDI output port".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["port_pattern"],
                "properties": {
                    "port_pattern": {
                        "type": "string",
                        "description": "Port name pattern to match (e.g., 'NiftyCASE', 'BRAINS')"
                    }
                }
            }),
        },
        ToolInfo {
            name: "midi_output_detach".to_string(),
            description: "Detach MIDI output port".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["port_pattern"],
                "properties": {
                    "port_pattern": {
                        "type": "string",
                        "description": "Port name pattern to match"
                    }
                }
            }),
        },
        ToolInfo {
            name: "midi_send".to_string(),
            description: "Send MIDI message to outputs".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["message"],
                "properties": {
                    "port_pattern": {
                        "type": "string",
                        "description": "Target port pattern (omit for all outputs)"
                    },
                    "message": {
                        "type": "object",
                        "description": "MIDI message to send",
                        "properties": {
                            "type": {
                                "type": "string",
                                "description": "Message type: note_on, note_off, control_change, program_change, pitch_bend"
                            },
                            "channel": {"type": "integer"},
                            "pitch": {"type": "integer"},
                            "velocity": {"type": "integer"},
                            "controller": {"type": "integer"},
                            "value": {"type": "integer"},
                            "program": {"type": "integer"}
                        }
                    }
                }
            }),
        },
        ToolInfo {
            name: "midi_status".to_string(),
            description: "Get MIDI I/O status".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolInfo {
            name: "midi_play".to_string(),
            description: "Play a MIDI artifact to external MIDI outputs".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "artifact_id": {
                        "description": "Artifact ID of the MIDI file to play",
                        "type": "string"
                    },
                    "port_pattern": {
                        "description": "Target port pattern (omit for all outputs)",
                        "type": "string"
                    },
                    "start_beat": {
                        "description": "Timeline position to start playback (default: 0)",
                        "type": "number"
                    }
                },
                "required": ["artifact_id"]
            }),
        },
        ToolInfo {
            name: "midi_stop".to_string(),
            description: "Stop MIDI file playback".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "region_id": {
                        "description": "Region ID returned by midi_play",
                        "type": "string"
                    }
                },
                "required": ["region_id"]
            }),
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
        ToolInfo {
            name: "audio_info".to_string(),
            description: "Get audio file information (levels, duration, sample rate) without GPU".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact ID of audio file" },
                    "hash": { "type": "string", "description": "CAS hash of audio file (alternative to artifact_id)" }
                }
            }),
        },

        // ==========================================================================
        // AudioLDM2 Tools
        // ==========================================================================
        ToolInfo {
            name: "audioldm2_generate".to_string(),
            description: "Generate audio from text prompt using AudioLDM2".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Text prompt for generation" },
                    "negative_prompt": { "type": "string", "description": "Negative prompt (what to avoid)" },
                    "duration": { "type": "number", "description": "Duration in seconds" },
                    "num_inference_steps": { "type": "integer", "description": "Number of diffusion steps (default 200)" },
                    "guidance_scale": { "type": "number", "description": "Classifier-free guidance scale" },
                    "seed": { "type": "integer", "description": "Random seed for reproducibility" },
                    "creator": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },

        // ==========================================================================
        // Anticipatory Music Transformer Tools
        // ==========================================================================
        ToolInfo {
            name: "anticipatory_generate".to_string(),
            description: "Generate polyphonic MIDI using Anticipatory Music Transformer".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "length_seconds": { "type": "number", "description": "Duration in seconds (default 20)" },
                    "top_p": { "type": "number", "description": "Nucleus sampling threshold (default 0.95)" },
                    "model_size": { "type": "string", "description": "Model size: small, medium, or large" },
                    "creator": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        ToolInfo {
            name: "anticipatory_continue".to_string(),
            description: "Continue existing MIDI using Anticipatory Music Transformer".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["input_hash"],
                "properties": {
                    "input_hash": { "type": "string", "description": "CAS hash of input MIDI" },
                    "length_seconds": { "type": "number", "description": "Total output length in seconds" },
                    "prime_seconds": { "type": "number", "description": "Seconds of input to use as context" },
                    "top_p": { "type": "number" },
                    "model_size": { "type": "string" },
                    "creator": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },
        ToolInfo {
            name: "anticipatory_embed".to_string(),
            description: "Extract hidden-state embeddings from MIDI".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["input_hash"],
                "properties": {
                    "input_hash": { "type": "string", "description": "CAS hash of input MIDI" },
                    "model_size": { "type": "string", "description": "Model size: small, medium, or large" },
                    "embed_layer": { "type": "integer", "description": "Transformer layer for embeddings (default -3)" }
                }
            }),
        },

        // ==========================================================================
        // Demucs Tools (Audio Source Separation)
        // ==========================================================================
        ToolInfo {
            name: "demucs_separate".to_string(),
            description: "Separate audio into stems (drums, bass, vocals, other) using Demucs".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["audio_hash"],
                "properties": {
                    "audio_hash": { "type": "string", "description": "CAS hash of input audio" },
                    "model": { "type": "string", "description": "Model: htdemucs, htdemucs_ft, htdemucs_6s" },
                    "stems": { "type": "array", "items": { "type": "string" }, "description": "Filter to specific stems" },
                    "two_stems": { "type": "string", "description": "Karaoke mode: isolate one stem vs rest" },
                    "creator": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },

        // ==========================================================================
        // MIDI Analysis / Voice Separation
        // ==========================================================================
        ToolInfo {
            name: "midi_analyze".to_string(),
            description: "Analyze MIDI structure: extract notes, profile tracks, detect merged voices needing separation".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact ID of MIDI file" },
                    "hash": { "type": "string", "description": "CAS hash of MIDI file (alternative to artifact_id)" },
                    "polyphony_threshold": { "type": "number", "description": "Polyphonic ratio threshold for flagging merged voices (default 0.3)" },
                    "density_window_beats": { "type": "number", "description": "Window size in beats for density analysis (default 4.0)" }
                }
            }),
        },
        ToolInfo {
            name: "midi_voice_separate".to_string(),
            description: "Separate merged voices in MIDI tracks into individual musical lines using pitch contiguity, channel split, skyline, or bassline algorithms".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact ID of MIDI file" },
                    "hash": { "type": "string", "description": "CAS hash of MIDI file (alternative to artifact_id)" },
                    "method": { "type": "string", "description": "Separation method: auto (default), channel_split, pitch_contiguity, skyline, bassline" },
                    "max_pitch_jump": { "type": "integer", "description": "Max pitch jump in semitones before new voice (default 12)" },
                    "max_gap_beats": { "type": "number", "description": "Max gap in beats before voice is stale (default 4.0)" },
                    "max_voices": { "type": "integer", "description": "Max voices to extract per track (default 8)" },
                    "track_indices": { "type": "array", "items": { "type": "integer" }, "description": "Which tracks to separate (empty = all flagged)" }
                }
            }),
        },
        ToolInfo {
            name: "midi_stems_export".to_string(),
            description: "Export separated MIDI voices as individual MIDI files stored in CAS, one artifact per voice".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["voice_data"],
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact ID of original MIDI (for context)" },
                    "hash": { "type": "string", "description": "CAS hash of original MIDI (for context)" },
                    "voice_data": { "type": "string", "description": "JSON voice separation data from midi_voice_separate" },
                    "combined_file": { "type": "boolean", "description": "Also export combined multi-track MIDI (default false)" },
                    "creator": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                }
            }),
        },

        ToolInfo {
            name: "midi_classify_voices".to_string(),
            description: "Classify separated MIDI voices by musical role (melody, bass, countermelody, harmony, percussion, etc.) using heuristic analysis or optional ML".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["voice_data"],
                "properties": {
                    "artifact_id": { "type": "string", "description": "Artifact ID of original MIDI (for context/features)" },
                    "hash": { "type": "string", "description": "CAS hash of original MIDI (alternative to artifact_id)" },
                    "voice_data": { "type": "string", "description": "JSON voice separation data from midi_voice_separate" },
                    "use_ml": { "type": "boolean", "description": "Try ML classification service (falls back to heuristic, default false)" }
                }
            }),
        },

        // ==========================================================================
        // RAVE Tools (Realtime Audio Variational autoEncoder)
        // ==========================================================================
        ToolInfo {
            name: "rave_encode".to_string(),
            description: "Encode audio to RAVE latent space".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["audio_hash"],
                "properties": {
                    "audio_hash": { "type": "string", "description": "CAS hash of input audio (WAV)" },
                    "model": { "type": "string", "description": "Model name (e.g., 'vintage', 'percussion')" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "rave_decode".to_string(),
            description: "Decode RAVE latent codes to audio".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["latent_hash", "latent_shape"],
                "properties": {
                    "latent_hash": { "type": "string", "description": "CAS hash of latent codes" },
                    "latent_shape": { "type": "array", "items": { "type": "integer" }, "description": "Shape of latent tensor" },
                    "model": { "type": "string", "description": "Model name" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "rave_reconstruct".to_string(),
            description: "Encode then decode audio (round-trip reconstruction)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["audio_hash"],
                "properties": {
                    "audio_hash": { "type": "string", "description": "CAS hash of input audio" },
                    "model": { "type": "string", "description": "Model name" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "rave_generate".to_string(),
            description: "Generate audio by sampling from RAVE prior".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "model": { "type": "string", "description": "Model name" },
                    "duration_seconds": { "type": "number", "description": "Duration in seconds" },
                    "temperature": { "type": "number", "description": "Sampling temperature (default 1.0)" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "creator": { "type": "string" }
                }
            }),
        },
        ToolInfo {
            name: "rave_stream_start".to_string(),
            description: "Start realtime RAVE audio streaming".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["input_identity", "output_identity"],
                "properties": {
                    "model": { "type": "string", "description": "Model name" },
                    "input_identity": { "type": "string", "description": "Graph identity for audio input source" },
                    "output_identity": { "type": "string", "description": "Graph identity for audio output sink" },
                    "buffer_size": { "type": "integer", "description": "Samples per buffer (default 2048)" }
                }
            }),
        },
        ToolInfo {
            name: "rave_stream_stop".to_string(),
            description: "Stop a RAVE streaming session".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["stream_id"],
                "properties": {
                    "stream_id": { "type": "string", "description": "Streaming session ID" }
                }
            }),
        },
        ToolInfo {
            name: "rave_stream_status".to_string(),
            description: "Get status of a RAVE streaming session".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "required": ["stream_id"],
                "properties": {
                    "stream_id": { "type": "string", "description": "Streaming session ID" }
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
