//! Manual JSON schemas for llama.cpp compatibility.
//!
//! llama.cpp cannot parse JSON schemas containing `default` fields - it fails with
//! "Unrecognized schema" errors. When Rust types use `#[serde(default)]` or
//! `#[serde(default = "fn")]`, schemars 1.x automatically emits `default` values
//! in the generated schema.
//!
//! This module provides hand-written schemas for types that have serde defaults.
//! Types without defaults can still use `schema_for::<T>()` from schemars.
//!
//! Key llama.cpp schema requirements:
//! - Every property node must have explicit `type` field
//! - No `default` fields anywhere in the schema
//! - Use `type: ["T", "null"]` for nullable, not `nullable: true`
//! - `oneOf`/`anyOf` are supported for union types

use serde_json::{json, Value};

/// Manual schema for PollRequest.
///
/// Reason: `#[serde(default)]` on `job_ids: Vec<String>` causes schemars to emit
/// `"default": []` which llama.cpp cannot parse.
pub fn poll_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "timeout_ms": {
                "type": "integer",
                "minimum": 0,
                "description": "Timeout in milliseconds"
            },
            "job_ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Job IDs to poll (empty = all pending)"
            },
            "mode": {
                "type": ["string", "null"],
                "description": "Mode: 'any' (return on first complete) or 'all' (wait for all). Default: 'any'"
            }
        },
        "required": ["timeout_ms"]
    })
}

/// Manual schema for ArtifactUploadRequest.
///
/// Reason: `#[serde(default)]` on `tags: Vec<String>` and
/// `#[serde(default = "default_creator")]` on `creator` cause schemars to emit
/// default values that llama.cpp cannot parse.
pub fn artifact_upload_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "file_path": {
                "type": "string",
                "description": "Absolute path to file to upload"
            },
            "mime_type": {
                "type": "string",
                "description": "MIME type of the file (e.g., 'audio/wav', 'audio/midi', 'audio/soundfont')"
            },
            "variation_set_id": {
                "type": ["string", "null"],
                "description": "Optional variation set ID to group related artifacts"
            },
            "parent_id": {
                "type": ["string", "null"],
                "description": "Optional parent artifact ID"
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional tags for organizing artifacts"
            },
            "creator": {
                "type": ["string", "null"],
                "description": "Creator identifier (agent or user ID). Default: 'unknown'"
            }
        },
        "required": ["file_path", "mime_type"]
    })
}

/// Manual schema for GraphBindRequest.
///
/// Reason: `#[serde(default)]` on `hints: Vec<GraphHint>` causes schemars to emit
/// a default value that llama.cpp cannot parse.
pub fn graph_bind_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "Identity ID"
            },
            "name": {
                "type": "string",
                "description": "Human-readable name"
            },
            "hints": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": {
                            "type": "string",
                            "description": "Hint kind (usb_device_id, midi_name, alsa_card, pipewire_name)"
                        },
                        "value": {
                            "type": "string",
                            "description": "Hint value"
                        },
                        "confidence": {
                            "type": "number",
                            "description": "Confidence score 0.0-1.0. Default: 1.0"
                        }
                    },
                    "required": ["kind", "value"]
                },
                "description": "Hints for matching devices"
            }
        },
        "required": ["id", "name"]
    })
}

/// Manual schema for SoundfontInspectRequest.
///
/// Reason: `#[serde(default)]` on `include_drum_map: bool` causes schemars to emit
/// `"default": false` which llama.cpp cannot parse.
pub fn soundfont_inspect_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "soundfont_hash": {
                "type": "string",
                "description": "CAS hash of SoundFont file to inspect"
            },
            "include_drum_map": {
                "type": "boolean",
                "description": "Include detailed drum mappings for percussion presets (bank 128). Default: false"
            }
        },
        "required": ["soundfont_hash"]
    })
}

/// Manual schema for GraphContextRequest.
///
/// Reason: Multiple defaults - `#[serde(default)]` on `include_metadata: bool` and
/// `#[serde(default = "default_true")]` on `include_annotations: bool` cause
/// schemars to emit default values that llama.cpp cannot parse.
pub fn graph_context_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tag": {
                "type": ["string", "null"],
                "description": "Filter by artifact tag (e.g., 'type:soundfont', 'type:midi', 'source:orpheus')"
            },
            "vibe_search": {
                "type": ["string", "null"],
                "description": "Search annotations/vibes for this text (e.g., 'warm', 'jazzy')"
            },
            "creator": {
                "type": ["string", "null"],
                "description": "Filter by creator (e.g., 'claude', 'user')"
            },
            "limit": {
                "type": ["integer", "null"],
                "minimum": 0,
                "description": "Maximum number of artifacts to include. Default: 20"
            },
            "include_metadata": {
                "type": "boolean",
                "description": "Include full metadata. Default: false"
            },
            "include_annotations": {
                "type": "boolean",
                "description": "Include annotations. Default: true"
            },
            "within_minutes": {
                "type": ["integer", "null"],
                "description": "Time window in minutes for recent artifacts. Default: 10"
            }
        }
    })
}

/// Manual schema for GraphQueryRequest.
///
/// Reason: `#[serde(default)]` on `variables: serde_json::Value` causes schemars
/// to emit a complex default that llama.cpp cannot parse.
pub fn graph_query_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "GraphQL query string OR artifact ID containing a saved query. Query example: '{ Artifact(tag: \"type:midi\") { id @output } }'"
            },
            "variables": {
                "type": "object",
                "description": "Variables for parameterized queries as JSON object (e.g., {\"artifact_id\": \"artifact_123\"})"
            },
            "limit": {
                "type": ["integer", "null"],
                "minimum": 0,
                "description": "Maximum number of results to return. Default: 100"
            }
        },
        "required": ["query"]
    })
}

/// Manual schema for WeaveResetRequest.
///
/// Reason: `#[serde(default)]` on `clear_session: bool` causes schemars to emit
/// `"default": false` which llama.cpp cannot parse.
pub fn weave_reset_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "clear_session": {
                "type": "boolean",
                "description": "If true, also clear session data (rules, markers, history). Default: false"
            }
        }
    })
}

/// Manual schema for SampleRequest.
///
/// Reason: Multiple defaults - `#[serde(default)]` on `inference`, `as_loop`, `tags`;
/// `#[serde(default = "default_one")]` on `num_variations`; and
/// `#[serde(default = "default_creator")]` on `creator` all cause schemars to emit
/// default values that llama.cpp cannot parse.
pub fn sample_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "space": {
                "type": "string",
                "enum": ["orpheus", "orpheus_children", "orpheus_mono_melodies", "orpheus_loops", "orpheus_bridge", "music_gen", "yue", "abc"],
                "description": "Generative space to sample from"
            },
            "inference": {
                "type": "object",
                "properties": {
                    "temperature": { "type": ["number", "null"], "description": "Sampling temperature 0.0-2.0" },
                    "top_p": { "type": ["number", "null"], "description": "Nucleus sampling 0.0-1.0" },
                    "top_k": { "type": ["integer", "null"], "description": "Top-k filtering (0 = disabled)" },
                    "seed": { "type": ["integer", "null"], "description": "Random seed for reproducibility" },
                    "max_tokens": { "type": ["integer", "null"], "description": "Max tokens to generate" },
                    "duration_seconds": { "type": ["number", "null"], "description": "Duration in seconds (for audio spaces)" },
                    "guidance_scale": { "type": ["number", "null"], "description": "Guidance scale for CFG" },
                    "variant": { "type": ["string", "null"], "description": "Model variant within space" }
                },
                "description": "Inference parameters"
            },
            "num_variations": {
                "type": ["integer", "null"],
                "description": "Number of variations to generate. Default: 1"
            },
            "prompt": {
                "type": ["string", "null"],
                "description": "Text prompt (for prompted spaces like musicgen, yue)"
            },
            "seed": {
                "oneOf": [
                    { "type": "null" },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "midi" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "audio" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "abc" },
                            "notation": { "type": "string" }
                        },
                        "required": ["type", "notation"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "hash" },
                            "content_hash": { "type": "string" },
                            "format": { "type": "string" }
                        },
                        "required": ["type", "content_hash", "format"]
                    }
                ],
                "description": "Seed encoding to condition on"
            },
            "as_loop": {
                "type": "boolean",
                "description": "Generate as loopable pattern (orpheus only). Default: false"
            },
            "variation_set_id": {
                "type": ["string", "null"],
                "description": "Variation set ID for grouping"
            },
            "parent_id": {
                "type": ["string", "null"],
                "description": "Parent artifact ID for refinements"
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Tags for organizing"
            },
            "creator": {
                "type": ["string", "null"],
                "description": "Creator identifier. Default: 'unknown'"
            }
        },
        "required": ["space"]
    })
}

/// Manual schema for ExtendRequest.
///
/// Reason: Same as SampleRequest - multiple `#[serde(default)]` annotations on
/// `inference`, `tags`, `num_variations`, and `creator` cause schemars to emit
/// default values that llama.cpp cannot parse.
pub fn extend_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "encoding": {
                "oneOf": [
                    {
                        "type": "object",
                        "description": "MIDI content via artifact ID",
                        "properties": {
                            "type": { "const": "midi" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "description": "Audio content via artifact ID",
                        "properties": {
                            "type": { "const": "audio" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "description": "ABC notation as raw string",
                        "properties": {
                            "type": { "const": "abc" },
                            "notation": { "type": "string" }
                        },
                        "required": ["type", "notation"]
                    },
                    {
                        "type": "object",
                        "description": "Raw content via CAS hash",
                        "properties": {
                            "type": { "const": "hash" },
                            "content_hash": { "type": "string" },
                            "format": { "type": "string" }
                        },
                        "required": ["type", "content_hash", "format"]
                    }
                ],
                "description": "Content to continue from"
            },
            "space": {
                "type": ["string", "null"],
                "enum": ["orpheus", "orpheus_children", "orpheus_mono_melodies", "orpheus_loops", null],
                "description": "Space to use (inferred from encoding if omitted)"
            },
            "inference": {
                "type": "object",
                "properties": {
                    "temperature": { "type": ["number", "null"] },
                    "top_p": { "type": ["number", "null"] },
                    "top_k": { "type": ["integer", "null"] },
                    "seed": { "type": ["integer", "null"] },
                    "max_tokens": { "type": ["integer", "null"] },
                    "duration_seconds": { "type": ["number", "null"] },
                    "guidance_scale": { "type": ["number", "null"] },
                    "variant": { "type": ["string", "null"] }
                },
                "description": "Inference parameters"
            },
            "num_variations": {
                "type": ["integer", "null"],
                "description": "Number of variations. Default: 1"
            },
            "variation_set_id": { "type": ["string", "null"] },
            "parent_id": { "type": ["string", "null"] },
            "tags": {
                "type": "array",
                "items": { "type": "string" }
            },
            "creator": {
                "type": ["string", "null"],
                "description": "Creator identifier. Default: 'unknown'"
            }
        },
        "required": ["encoding"]
    })
}

/// Manual schema for GardenQueryRequest.
///
/// Reason: `#[serde(default)]` on `variables: serde_json::Value` causes schemars
/// to emit a default value that llama.cpp cannot parse.
pub fn garden_query_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "GraphQL-style Trustfall query"
            },
            "variables": {
                "type": "object",
                "description": "Query variables as JSON object"
            }
        },
        "required": ["query"]
    })
}

/// Manual schema for GardenAttachAudioRequest.
///
/// Reason: `#[serde(default)]` on all optional fields (`device_name`, `sample_rate`,
/// `latency_frames`) causes schemars to emit default values that llama.cpp cannot parse.
pub fn garden_attach_audio_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "device_name": {
                "type": ["string", "null"],
                "description": "Device name hint (empty for default output)"
            },
            "sample_rate": {
                "type": ["integer", "null"],
                "description": "Sample rate in Hz. Default: 48000"
            },
            "latency_frames": {
                "type": ["integer", "null"],
                "description": "Latency in frames. Default: 256"
            }
        }
    })
}

/// Manual schema for GardenAttachInputRequest.
///
/// Reason: `#[serde(default)]` on optional fields (`device_name`, `sample_rate`)
/// causes schemars to emit default values that llama.cpp cannot parse.
pub fn garden_attach_input_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "device_name": {
                "type": ["string", "null"],
                "description": "Device name hint (empty for default input)"
            },
            "sample_rate": {
                "type": ["integer", "null"],
                "description": "Sample rate in Hz (should match output). Default: 48000"
            }
        }
    })
}

/// Manual schema for GardenSetMonitorRequest.
///
/// Reason: `#[serde(default)]` on optional fields (`enabled`, `gain`) causes
/// schemars to emit default values that llama.cpp cannot parse.
pub fn garden_set_monitor_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "enabled": {
                "type": ["boolean", "null"],
                "description": "Enable/disable monitor (null = don't change)"
            },
            "gain": {
                "type": ["number", "null"],
                "description": "Monitor gain 0.0-1.0 (null = don't change)"
            }
        }
    })
}

/// Manual schema for `BridgeRequest`.
///
/// Reason: `#[serde(default)]` on inference, tags, creator.
pub fn bridge_request() -> Value {
    json!({
        "type": "object",
        "required": ["from"],
        "properties": {
            "from": {
                "description": "Starting content (section A)",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "midi" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "audio" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "hash" },
                            "content_hash": { "type": "string" },
                            "format": { "type": "string" }
                        },
                        "required": ["type", "content_hash", "format"]
                    }
                ]
            },
            "to": {
                "description": "Target content (section B) - optional for A->B bridging",
                "oneOf": [
                    { "type": "null" },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "midi" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "hash" },
                            "content_hash": { "type": "string" },
                            "format": { "type": "string" }
                        },
                        "required": ["type", "content_hash", "format"]
                    }
                ]
            },
            "inference": {
                "type": "object",
                "description": "Inference parameters",
                "properties": {
                    "temperature": { "type": "number", "description": "Sampling temperature 0.0-2.0" },
                    "top_p": { "type": "number", "description": "Nucleus sampling 0.0-1.0" },
                    "max_tokens": { "type": "integer", "description": "Max tokens to generate" }
                }
            },
            "variation_set_id": { "type": "string" },
            "parent_id": { "type": "string" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "creator": { "type": "string" }
        }
    })
}

/// Manual schema for `ProjectRequest`.
///
/// Reason: `#[serde(default)]` on tags, creator.
pub fn project_request() -> Value {
    json!({
        "type": "object",
        "required": ["encoding", "target"],
        "properties": {
            "encoding": {
                "description": "Source content to project",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "midi" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "abc" },
                            "notation": { "type": "string" }
                        },
                        "required": ["type", "notation"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "hash" },
                            "content_hash": { "type": "string" },
                            "format": { "type": "string" }
                        },
                        "required": ["type", "content_hash", "format"]
                    }
                ]
            },
            "target": {
                "description": "Target format for projection",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "audio" },
                            "soundfont_hash": { "type": "string", "description": "SoundFont CAS hash" },
                            "sample_rate": { "type": "integer", "description": "Output sample rate" }
                        },
                        "required": ["type", "soundfont_hash"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "midi" },
                            "channel": { "type": "integer", "description": "MIDI channel 0-15" },
                            "velocity": { "type": "integer", "description": "Note velocity 1-127" }
                        },
                        "required": ["type"]
                    }
                ]
            },
            "variation_set_id": { "type": "string" },
            "parent_id": { "type": "string" },
            "tags": { "type": "array", "items": { "type": "string" } },
            "creator": { "type": "string" }
        }
    })
}

/// Manual schema for `AnalyzeRequest`.
///
/// Reason: Uses tagged enum for tasks with ZeroShot variant.
pub fn analyze_request() -> Value {
    json!({
        "type": "object",
        "required": ["encoding", "tasks"],
        "properties": {
            "encoding": {
                "description": "Content to analyze",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "midi" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "audio" },
                            "artifact_id": { "type": "string" }
                        },
                        "required": ["type", "artifact_id"]
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": { "const": "hash" },
                            "content_hash": { "type": "string" },
                            "format": { "type": "string" }
                        },
                        "required": ["type", "content_hash", "format"]
                    }
                ]
            },
            "tasks": {
                "type": "array",
                "description": "Analysis tasks to run",
                "items": {
                    "oneOf": [
                        { "const": "classify", "description": "Classify MIDI content" },
                        { "const": "beats", "description": "Detect beats and downbeats" },
                        { "const": "embeddings", "description": "Extract CLAP embeddings" },
                        { "const": "genre", "description": "Classify genre" },
                        { "const": "mood", "description": "Classify mood/energy" },
                        {
                            "type": "object",
                            "description": "Zero-shot classification with custom labels",
                            "properties": {
                                "zero_shot": {
                                    "type": "object",
                                    "properties": {
                                        "labels": {
                                            "type": "array",
                                            "items": { "type": "string" }
                                        }
                                    },
                                    "required": ["labels"]
                                }
                            },
                            "required": ["zero_shot"]
                        }
                    ]
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recursively check that no schema node contains a "default" field.
    fn assert_no_defaults(schema: &Value, path: &str) {
        if let Value::Object(map) = schema {
            assert!(
                !map.contains_key("default"),
                "{} contains 'default' field which breaks llama.cpp",
                path
            );

            if let Some(Value::Object(props)) = map.get("properties") {
                for (name, prop) in props {
                    assert_no_defaults(prop, &format!("{}.{}", path, name));
                }
            }

            for key in &["allOf", "anyOf", "oneOf"] {
                if let Some(Value::Array(items)) = map.get(*key) {
                    for (i, item) in items.iter().enumerate() {
                        assert_no_defaults(item, &format!("{}[{}][{}]", path, key, i));
                    }
                }
            }

            if let Some(items) = map.get("items") {
                assert_no_defaults(items, &format!("{}.items", path));
            }
        }
    }

    /// Verify that property nodes have explicit type fields.
    fn assert_has_types(schema: &Value, path: &str) {
        if let Value::Object(map) = schema {
            if let Some(Value::Object(props)) = map.get("properties") {
                for (name, prop) in props {
                    if let Value::Object(prop_map) = prop {
                        let has_type = prop_map.contains_key("type")
                            || prop_map.contains_key("oneOf")
                            || prop_map.contains_key("anyOf")
                            || prop_map.contains_key("allOf")
                            || prop_map.contains_key("const");
                        assert!(
                            has_type,
                            "{}.{} has no explicit type (llama.cpp requirement)",
                            path, name
                        );
                    }
                    assert_has_types(prop, &format!("{}.{}", path, name));
                }
            }
        }
    }

    #[test]
    fn test_poll_request_schema() {
        let schema = poll_request();
        assert_no_defaults(&schema, "poll_request");
        assert_has_types(&schema, "poll_request");
    }

    #[test]
    fn test_artifact_upload_request_schema() {
        let schema = artifact_upload_request();
        assert_no_defaults(&schema, "artifact_upload_request");
        assert_has_types(&schema, "artifact_upload_request");
    }

    #[test]
    fn test_graph_bind_request_schema() {
        let schema = graph_bind_request();
        assert_no_defaults(&schema, "graph_bind_request");
        assert_has_types(&schema, "graph_bind_request");
    }

    #[test]
    fn test_soundfont_inspect_request_schema() {
        let schema = soundfont_inspect_request();
        assert_no_defaults(&schema, "soundfont_inspect_request");
        assert_has_types(&schema, "soundfont_inspect_request");
    }

    #[test]
    fn test_graph_context_request_schema() {
        let schema = graph_context_request();
        assert_no_defaults(&schema, "graph_context_request");
        assert_has_types(&schema, "graph_context_request");
    }

    #[test]
    fn test_graph_query_request_schema() {
        let schema = graph_query_request();
        assert_no_defaults(&schema, "graph_query_request");
        assert_has_types(&schema, "graph_query_request");
    }

    #[test]
    fn test_weave_reset_request_schema() {
        let schema = weave_reset_request();
        assert_no_defaults(&schema, "weave_reset_request");
        assert_has_types(&schema, "weave_reset_request");
    }

    #[test]
    fn test_sample_request_schema() {
        let schema = sample_request();
        assert_no_defaults(&schema, "sample_request");
        assert_has_types(&schema, "sample_request");
    }

    #[test]
    fn test_extend_request_schema() {
        let schema = extend_request();
        assert_no_defaults(&schema, "extend_request");
        assert_has_types(&schema, "extend_request");
    }

    #[test]
    fn test_garden_query_request_schema() {
        let schema = garden_query_request();
        assert_no_defaults(&schema, "garden_query_request");
        assert_has_types(&schema, "garden_query_request");
    }

    #[test]
    fn test_garden_attach_audio_request_schema() {
        let schema = garden_attach_audio_request();
        assert_no_defaults(&schema, "garden_attach_audio_request");
        assert_has_types(&schema, "garden_attach_audio_request");
    }

    #[test]
    fn test_garden_attach_input_request_schema() {
        let schema = garden_attach_input_request();
        assert_no_defaults(&schema, "garden_attach_input_request");
        assert_has_types(&schema, "garden_attach_input_request");
    }

    #[test]
    fn test_garden_set_monitor_request_schema() {
        let schema = garden_set_monitor_request();
        assert_no_defaults(&schema, "garden_set_monitor_request");
        assert_has_types(&schema, "garden_set_monitor_request");
    }
}
