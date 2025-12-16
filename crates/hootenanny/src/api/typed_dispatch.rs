//! Typed tool dispatch - returns ToolResponse instead of JSON.
//!
//! This module provides typed dispatch for tools. Currently it wraps
//! the existing JSON-based dispatch, but provides typed responses for
//! the ZMQ layer to serialize to MsgPack.
//!
//! ## Migration Plan
//!
//! Phase 1 (current): Wrap existing dispatch, convert JSON → ToolResponse
//! Phase 2 (later): Refactor tools to return ToolResponse directly
//!
//! The key benefit even in Phase 1: callers get typed responses that
//! serialize efficiently to MsgPack, avoiding JSON at the wire level.

use crate::api::dispatch::{dispatch_tool, DispatchError};
use crate::api::service::EventDualityServer;
use hooteproto::responses::*;
use hooteproto::{Payload, ToolResponse};
use serde_json::Value;

/// Error type for typed dispatch
#[derive(Debug, Clone)]
pub struct TypedDispatchError {
    pub code: String,
    pub message: String,
}

impl From<DispatchError> for TypedDispatchError {
    fn from(e: DispatchError) -> Self {
        Self {
            code: e.code,
            message: e.message,
        }
    }
}

/// Result type for typed dispatch
pub type TypedDispatchResult = Result<ToolResponse, TypedDispatchError>;

/// Dispatch a Payload to a typed ToolResponse.
///
/// Converts the Payload to tool name + args, calls existing dispatch,
/// then converts the JSON result to a typed ToolResponse.
pub async fn typed_dispatch(
    server: &EventDualityServer,
    payload: Payload,
) -> TypedDispatchResult {
    let (tool_name, args) = payload_to_tool_args(&payload)?;

    let json_result = dispatch_tool(server, &tool_name, args).await?;

    // Convert JSON result to typed response based on tool
    json_to_typed_response(&tool_name, json_result)
}

/// Convert Payload to tool name and JSON arguments.
fn payload_to_tool_args(payload: &Payload) -> Result<(String, Value), TypedDispatchError> {
    let json = serde_json::to_value(payload).map_err(|e| TypedDispatchError {
        code: "serialization_error".to_string(),
        message: e.to_string(),
    })?;

    let tool_name = payload_type_name(payload).to_string();

    let mut args = json.as_object().cloned().unwrap_or_default();
    args.remove("type");

    Ok((tool_name, Value::Object(args)))
}

/// Convert JSON dispatch result to typed ToolResponse.
fn json_to_typed_response(tool_name: &str, json: Value) -> TypedDispatchResult {
    match tool_name {
        // CAS operations
        "cas_store" => Ok(ToolResponse::CasStored(CasStoredResponse {
            hash: json_str(&json, "hash"),
            size: json_usize(&json, "size"),
            mime_type: json_str(&json, "mime_type"),
        })),

        "cas_inspect" => Ok(ToolResponse::CasInspected(CasInspectedResponse {
            hash: json_str(&json, "hash"),
            exists: json.get("exists").and_then(|v| v.as_bool()).unwrap_or(false),
            size: json.get("size").and_then(|v| v.as_u64()).map(|n| n as usize),
            preview: json.get("preview").and_then(|v| v.as_str()).map(String::from),
        })),

        // Garden/transport
        "garden_status" => Ok(ToolResponse::GardenStatus(GardenStatusResponse {
            state: match json.get("state").and_then(|v| v.as_str()) {
                Some("playing") => TransportState::Playing,
                Some("paused") => TransportState::Paused,
                _ => TransportState::Stopped,
            },
            position_beats: json_f64(&json, "position"),
            tempo_bpm: json.get("tempo").and_then(|v| v.as_f64()).unwrap_or(120.0),
            region_count: json_usize(&json, "region_count"),
        })),

        "garden_play" | "garden_pause" | "garden_stop" | "garden_seek" | "garden_set_tempo" => {
            Ok(ToolResponse::ack("ok"))
        }

        "garden_get_regions" => {
            let regions: Vec<GardenRegionInfo> = json
                .get("regions")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|r| {
                            Some(GardenRegionInfo {
                                region_id: r.get("region_id")?.as_str()?.to_string(),
                                position: r.get("position")?.as_f64()?,
                                duration: r.get("duration")?.as_f64()?,
                                behavior_type: r.get("behavior_type")?.as_str()?.to_string(),
                                content_id: r.get("content_id")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let count = regions.len();
            Ok(ToolResponse::GardenRegions(GardenRegionsResponse {
                regions,
                count,
            }))
        }

        // Job operations
        "job_status" => Ok(ToolResponse::JobStatus(JobStatusResponse {
            job_id: json_str(&json, "job_id"),
            status: match json.get("status").and_then(|v| v.as_str()) {
                Some("pending") => JobState::Pending,
                Some("running") => JobState::Running,
                Some("complete") => JobState::Complete,
                Some("failed") => JobState::Failed,
                Some("cancelled") => JobState::Cancelled,
                _ => JobState::Pending,
            },
            source: json_str(&json, "source"),
            result: None,
            error: json.get("error").and_then(|v| v.as_str()).map(String::from),
            created_at: json.get("created_at").and_then(|v| v.as_u64()).unwrap_or(0),
            started_at: json.get("started_at").and_then(|v| v.as_u64()),
            completed_at: json.get("completed_at").and_then(|v| v.as_u64()),
        })),

        // Async tools that return job_id
        "orpheus_generate" | "orpheus_generate_seeded" | "orpheus_continue" | "orpheus_bridge"
        | "orpheus_loops" | "convert_midi_to_wav" | "musicgen_generate" | "yue_generate"
        | "clap_analyze" | "beatthis_analyze" => {
            if let Some(job_id) = json.get("job_id").and_then(|v| v.as_str()) {
                Ok(ToolResponse::job_started(job_id, tool_name))
            } else {
                // Completed immediately (unlikely for these tools)
                Ok(ToolResponse::GraphQueryResult(GraphQueryResultResponse {
                    results: vec![json],
                    count: 1,
                }))
            }
        }

        // Graph operations
        "graph_bind" => Ok(ToolResponse::GraphIdentity(GraphIdentityResponse {
            id: json_str(&json, "id"),
            name: json_str(&json, "name"),
            created_at: json.get("created_at").and_then(|v| v.as_u64()).unwrap_or(0),
        })),

        "graph_find" => {
            let identities: Vec<GraphIdentityInfo> = json
                .get("identities")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|i| {
                            Some(GraphIdentityInfo {
                                id: i.get("id")?.as_str()?.to_string(),
                                name: i.get("name")?.as_str()?.to_string(),
                                tags: i
                                    .get("tags")
                                    .and_then(|t| t.as_array())
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|v| v.as_str().map(String::from))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let count = identities.len();
            Ok(ToolResponse::GraphIdentities(GraphIdentitiesResponse {
                identities,
                count,
            }))
        }

        // Default: wrap as generic query result
        _ => Ok(ToolResponse::GraphQueryResult(GraphQueryResultResponse {
            results: vec![json],
            count: 1,
        })),
    }
}

/// Get payload type name for routing
fn payload_type_name(payload: &Payload) -> &'static str {
    match payload {
        Payload::CasStore { .. } => "cas_store",
        Payload::CasInspect { .. } => "cas_inspect",
        Payload::CasGet { .. } => "cas_get",
        Payload::GardenStatus => "garden_status",
        Payload::GardenPlay => "garden_play",
        Payload::GardenPause => "garden_pause",
        Payload::GardenStop => "garden_stop",
        Payload::GardenSeek { .. } => "garden_seek",
        Payload::GardenSetTempo { .. } => "garden_set_tempo",
        Payload::GardenGetRegions { .. } => "garden_get_regions",
        Payload::JobStatus { .. } => "job_status",
        Payload::JobList { .. } => "job_list",
        Payload::OrpheusGenerate { .. } => "orpheus_generate",
        Payload::OrpheusGenerateSeeded { .. } => "orpheus_generate_seeded",
        Payload::OrpheusContinue { .. } => "orpheus_continue",
        Payload::OrpheusBridge { .. } => "orpheus_bridge",
        Payload::OrpheusLoops { .. } => "orpheus_loops",
        Payload::ConvertMidiToWav { .. } => "convert_midi_to_wav",
        Payload::MusicgenGenerate { .. } => "musicgen_generate",
        Payload::YueGenerate { .. } => "yue_generate",
        Payload::GraphBind { .. } => "graph_bind",
        Payload::GraphFind { .. } => "graph_find",
        _ => "unknown",
    }
}

// Helper functions for JSON extraction
fn json_str(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn json_usize(v: &Value, key: &str) -> usize {
    v.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as usize
}

fn json_f64(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}
