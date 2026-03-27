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

// DAW schema functions removed (sample_request, extend_request, bridge_request, project_request, analyze_request, schedule_request)

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
            },
            "monitor": {
                "type": ["boolean", "null"],
                "description": "Enable monitor passthrough immediately (avoids buffer overruns). Default: false"
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

// =============================================================================
// Additional schemas for types that were using schema_for::<T>()
// These are simple schemas without defaults - llama.cpp compatible
// =============================================================================

/// Schema for ArtifactListRequest
pub fn artifact_list_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tag": {
                "type": ["string", "null"],
                "description": "Filter by tag"
            },
            "creator": {
                "type": ["string", "null"],
                "description": "Filter by creator"
            }
        }
    })
}

/// Schema for ArtifactGetRequest
pub fn artifact_get_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "Artifact ID"
            }
        },
        "required": ["id"]
    })
}

/// Schema for CancelJobRequest (job_cancel)
pub fn cancel_job_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "job_id": {
                "type": "string",
                "description": "Job ID to cancel"
            }
        },
        "required": ["job_id"]
    })
}

/// Schema for EventPollRequest (event_poll)
pub fn event_poll_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "cursor": {
                "type": ["integer", "null"],
                "minimum": 0,
                "description": "Cursor from previous poll (omit for initial poll to get recent events)"
            },
            "since_ms": {
                "type": ["integer", "null"],
                "minimum": 0,
                "description": "Get events from the last N milliseconds (alternative to cursor). Useful for real-time UIs."
            },
            "types": {
                "type": ["array", "null"],
                "items": { "type": "string" },
                "description": "Event types to filter (e.g., [\"job_state_changed\", \"artifact_created\"]). Omit for all types."
            },
            "timeout_ms": {
                "type": ["integer", "null"],
                "minimum": 0,
                "description": "How long to wait for events (ms). Default: 5000, max: 30000"
            },
            "limit": {
                "type": ["integer", "null"],
                "minimum": 1,
                "description": "Max events to return. Default: 100, max: 1000"
            }
        }
    })
}

/// Schema for AbcValidateRequest
pub fn abc_validate_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "abc": {
                "type": "string",
                "description": "ABC notation string to validate"
            }
        },
        "required": ["abc"]
    })
}

/// Schema for GardenSeekRequest
pub fn garden_seek_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "beat": {
                "type": "number",
                "description": "Beat position to seek to"
            }
        },
        "required": ["beat"]
    })
}

/// Schema for GardenSetTempoRequest
pub fn garden_set_tempo_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "bpm": {
                "type": "number",
                "description": "Tempo in beats per minute"
            }
        },
        "required": ["bpm"]
    })
}

/// Schema for GardenCreateRegionRequest
pub fn garden_create_region_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "position": {
                "type": "number",
                "description": "Beat position for the region"
            },
            "duration": {
                "type": "number",
                "description": "Duration in beats"
            },
            "behavior_type": {
                "type": "string",
                "description": "Type of region behavior (e.g., 'play_audio')"
            },
            "content_id": {
                "type": "string",
                "description": "Content identifier (artifact ID or CAS hash)"
            }
        },
        "required": ["position", "duration", "behavior_type", "content_id"]
    })
}

/// Schema for GardenDeleteRegionRequest
pub fn garden_delete_region_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "region_id": {
                "type": "string",
                "description": "UUID of the region to delete"
            }
        },
        "required": ["region_id"]
    })
}

/// Schema for GardenMoveRegionRequest
pub fn garden_move_region_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "region_id": {
                "type": "string",
                "description": "UUID of the region to move"
            },
            "new_position": {
                "type": "number",
                "description": "New beat position"
            }
        },
        "required": ["region_id", "new_position"]
    })
}

/// Schema for GardenGetRegionsRequest
pub fn garden_get_regions_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "start": {
                "type": ["number", "null"],
                "description": "Start of range in beats (optional)"
            },
            "end": {
                "type": ["number", "null"],
                "description": "End of range in beats (optional)"
            }
        }
    })
}

/// Schema for ConfigGetRequest
pub fn config_get_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "section": {
                "type": ["string", "null"],
                "description": "Config section: 'paths', 'bind', 'telemetry', 'models', 'connections', 'media', 'defaults'. Omit for full config."
            },
            "key": {
                "type": ["string", "null"],
                "description": "Specific key within section (e.g. 'cas_dir' in paths section)"
            }
        }
    })
}

/// Schema for WeaveEvalRequest
pub fn weave_eval_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "code": {
                "type": "string",
                "description": "Python code to execute in the vibeweaver kernel"
            }
        },
        "required": ["code"]
    })
}

/// Schema for WeaveSessionRequest
pub fn weave_session_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {
                "type": ["string", "null"],
                "description": "Session ID (uses current session if not specified)"
            }
        }
    })
}

/// Schema for GardenGraphRequest (no parameters)
pub fn garden_graph_request() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

/// Schema for TimeConvertRequest
pub fn time_convert_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "value": {
                "type": "number",
                "description": "The value to convert"
            },
            "from": {
                "type": "string",
                "enum": ["beats", "seconds"],
                "description": "The unit to convert from"
            },
            "to": {
                "type": "string",
                "enum": ["beats", "seconds"],
                "description": "The unit to convert to"
            }
        },
        "required": ["value", "from", "to"],
        "additionalProperties": false
    })
}

/// Schema for WeaveHelpRequest
pub fn weave_help_request() -> Value {
    json!({
        "type": "object",
        "properties": {
            "topic": {
                "type": ["string", "null"],
                "description": "Help topic: 'api', 'session', 'scheduler', 'examples', or omit for overview"
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
    fn test_soundfont_inspect_request_schema() {
        let schema = soundfont_inspect_request();
        assert_no_defaults(&schema, "soundfont_inspect_request");
        assert_has_types(&schema, "soundfont_inspect_request");
    }

    #[test]
    fn test_weave_reset_request_schema() {
        let schema = weave_reset_request();
        assert_no_defaults(&schema, "weave_reset_request");
        assert_has_types(&schema, "weave_reset_request");
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
