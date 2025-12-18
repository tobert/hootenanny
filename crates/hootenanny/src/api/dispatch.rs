//! Tool dispatch for ZMQ server
//!
//! This module dispatches tool calls to handlers and converts results to JSON.
//! Tools return hooteproto::ToolResult, which we convert to JSON for ZMQ transport.

use crate::api::schema::*;
use crate::api::service::EventDualityServer;
use audio_graph_mcp::{graph_bind, graph_connect, graph_find, graph_tag, HintKind};
use hooteproto::{ToolResult, ToolError, ToolInfo};
use serde_json::Value;
use schemars::JsonSchema;

/// Result of tool dispatch - either success with JSON or error with details
pub type DispatchResult = Result<Value, DispatchError>;

/// Error from tool dispatch
#[derive(Debug, Clone)]
pub struct DispatchError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl From<ToolError> for DispatchError {
    fn from(e: ToolError) -> Self {
        Self {
            code: e.code().to_string(),
            message: e.message(),
            details: None,
        }
    }
}

impl DispatchError {
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: "invalid_params".to_string(),
            message: msg.into(),
            details: None,
        }
    }

    pub fn not_found(tool: &str) -> Self {
        Self {
            code: "tool_not_found".to_string(),
            message: format!("Unknown tool: {}", tool),
            details: None,
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: "internal_error".to_string(),
            message: msg.into(),
            details: None,
        }
    }
}

/// Helper to generate JSON schema for a type
fn schema_for<T: JsonSchema>() -> Value {
    let settings = schemars::generate::SchemaSettings::draft07().with(|s| {
        s.inline_subschemas = true;
    });
    let gen = settings.into_generator();
    let schema = gen.into_root_schema_for::<T>();
    serde_json::to_value(schema).unwrap_or_default()
}

/// List all tools supported by the dispatcher
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
        ToolInfo {
            name: "artifact_upload".to_string(),
            description: "Upload file and create artifact".to_string(),
            input_schema: schema_for::<ArtifactUploadRequest>(),
        },

        // Conversion Tools
        ToolInfo {
            name: "convert_midi_to_wav".to_string(),
            description: "Render MIDI to WAV using SoundFont".to_string(),
            input_schema: schema_for::<MidiToWavRequest>(),
        },
        ToolInfo {
            name: "soundfont_inspect".to_string(),
            description: "Inspect SoundFont presets".to_string(),
            input_schema: schema_for::<SoundfontInspectRequest>(),
        },
        ToolInfo {
            name: "soundfont_preset_inspect".to_string(),
            description: "Inspect specific SoundFont preset".to_string(),
            input_schema: schema_for::<SoundfontPresetInspectRequest>(),
        },

        // Orpheus Tools
        ToolInfo {
            name: "orpheus_generate".to_string(),
            description: "Generate MIDI from scratch".to_string(),
            input_schema: schema_for::<OrpheusGenerateRequest>(),
        },
        ToolInfo {
            name: "orpheus_generate_seeded".to_string(),
            description: "Generate MIDI from seed".to_string(),
            input_schema: schema_for::<OrpheusGenerateSeededRequest>(),
        },
        ToolInfo {
            name: "orpheus_continue".to_string(),
            description: "Continue existing MIDI".to_string(),
            input_schema: schema_for::<OrpheusContinueRequest>(),
        },
        ToolInfo {
            name: "orpheus_bridge".to_string(),
            description: "Create bridge between two MIDI sections".to_string(),
            input_schema: schema_for::<OrpheusBridgeRequest>(),
        },
        ToolInfo {
            name: "orpheus_loops".to_string(),
            description: "Generate loopable MIDI".to_string(),
            input_schema: schema_for::<OrpheusLoopsRequest>(),
        },
        ToolInfo {
            name: "orpheus_classify".to_string(),
            description: "Classify MIDI content".to_string(),
            input_schema: schema_for::<OrpheusClassifyRequest>(),
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
        ToolInfo {
            name: "add_annotation".to_string(),
            description: "Add annotation to artifact".to_string(),
            input_schema: schema_for::<AddAnnotationRequest>(),
        },

        // ABC Tools
        ToolInfo {
            name: "abc_parse".to_string(),
            description: "Parse ABC notation".to_string(),
            input_schema: schema_for::<AbcParseRequest>(),
        },
        ToolInfo {
            name: "abc_to_midi".to_string(),
            description: "Convert ABC to MIDI".to_string(),
            input_schema: schema_for::<AbcToMidiRequest>(),
        },
        ToolInfo {
            name: "abc_validate".to_string(),
            description: "Validate ABC notation".to_string(),
            input_schema: schema_for::<AbcValidateRequest>(),
        },
        ToolInfo {
            name: "abc_transpose".to_string(),
            description: "Transpose ABC notation".to_string(),
            input_schema: schema_for::<AbcTransposeRequest>(),
        },

        // Analysis Tools
        ToolInfo {
            name: "beatthis_analyze".to_string(),
            description: "Analyze beats in audio".to_string(),
            input_schema: schema_for::<AnalyzeBeatsRequest>(),
        },
        ToolInfo {
            name: "clap_analyze".to_string(),
            description: "Analyze audio with CLAP".to_string(),
            input_schema: schema_for::<ClapAnalyzeRequest>(),
        },

        // Generation Tools
        ToolInfo {
            name: "musicgen_generate".to_string(),
            description: "Generate audio with MusicGen".to_string(),
            input_schema: schema_for::<MusicgenGenerateRequest>(),
        },
        ToolInfo {
            name: "yue_generate".to_string(),
            description: "Generate song with YuE".to_string(),
            input_schema: schema_for::<YueGenerateRequest>(),
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
        ToolInfo {
            name: "garden_emergency_pause".to_string(),
            description: "Emergency pause".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        // Config Tools
        ToolInfo {
            name: "config_get".to_string(),
            description: "Get configuration values (read-only)".to_string(),
            input_schema: schema_for::<super::tools::config::ConfigGetRequest>(),
        },

        // Garden region operations
        ToolInfo {
            name: "garden_create_region".to_string(),
            description: "Create a region on the timeline".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenCreateRegionRequest>(),
        },
        ToolInfo {
            name: "garden_delete_region".to_string(),
            description: "Delete a region from the timeline".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenDeleteRegionRequest>(),
        },
        ToolInfo {
            name: "garden_move_region".to_string(),
            description: "Move a region to a new position".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenMoveRegionRequest>(),
        },
        ToolInfo {
            name: "garden_get_regions".to_string(),
            description: "Get regions from the timeline".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenGetRegionsRequest>(),
        },
        // Audio attachment
        ToolInfo {
            name: "garden_attach_audio".to_string(),
            description: "Attach PipeWire audio output".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenAttachAudioRequest>(),
        },
        ToolInfo {
            name: "garden_detach_audio".to_string(),
            description: "Detach audio output".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "garden_audio_status".to_string(),
            description: "Get audio output status".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        // Monitor input
        ToolInfo {
            name: "garden_attach_input".to_string(),
            description: "Attach PipeWire monitor input (for mic passthrough)".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenAttachInputRequest>(),
        },
        ToolInfo {
            name: "garden_detach_input".to_string(),
            description: "Detach monitor input".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "garden_input_status".to_string(),
            description: "Get monitor input status".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        ToolInfo {
            name: "garden_set_monitor".to_string(),
            description: "Set monitor enabled/gain (input -> output passthrough)".to_string(),
            input_schema: schema_for::<super::tools::garden::GardenSetMonitorRequest>(),
        },
    ]
}

/// Dispatch a tool call to the appropriate handler
pub async fn dispatch_tool(
    server: &EventDualityServer,
    name: &str,
    args: Value,
) -> DispatchResult {
    match name {
        // CAS tools
        "cas_store" => {
            let request: CasStoreRequest = parse_args(args)?;
            tool_to_json(server.cas_store(request).await)
        }
        "cas_inspect" => {
            let request: CasInspectRequest = parse_args(args)?;
            tool_to_json(server.cas_inspect(request).await)
        }
        "cas_stats" => {
            tool_to_json(server.cas_stats().await)
        }
        "cas_upload_file" => {
            let request: UploadFileRequest = parse_args(args)?;
            tool_to_json(server.upload_file(request).await)
        }
        "artifact_upload" => {
            let request: ArtifactUploadRequest = parse_args(args)?;
            tool_to_json(server.artifact_upload(request).await)
        }

        // Conversion tools
        "convert_midi_to_wav" => {
            let request: MidiToWavRequest = parse_args(args)?;
            tool_to_json(server.midi_to_wav(request).await)
        }
        "soundfont_inspect" => {
            let request: SoundfontInspectRequest = parse_args(args)?;
            tool_to_json(server.soundfont_inspect(request).await)
        }
        "soundfont_preset_inspect" => {
            let request: SoundfontPresetInspectRequest = parse_args(args)?;
            tool_to_json(server.soundfont_preset_inspect(request).await)
        }

        // Orpheus tools
        "orpheus_generate" => {
            let request: OrpheusGenerateRequest = parse_args(args)?;
            tool_to_json(server.orpheus_generate(request).await)
        }
        "orpheus_generate_seeded" => {
            let request: OrpheusGenerateSeededRequest = parse_args(args)?;
            tool_to_json(server.orpheus_generate_seeded(request).await)
        }
        "orpheus_continue" => {
            let request: OrpheusContinueRequest = parse_args(args)?;
            tool_to_json(server.orpheus_continue(request).await)
        }
        "orpheus_bridge" => {
            let request: OrpheusBridgeRequest = parse_args(args)?;
            tool_to_json(server.orpheus_bridge(request).await)
        }
        "orpheus_loops" => {
            let request: OrpheusLoopsRequest = parse_args(args)?;
            tool_to_json(server.orpheus_loops(request).await)
        }
        "orpheus_classify" => {
            let request: OrpheusClassifyRequest = parse_args(args)?;
            tool_to_json(server.orpheus_classify(request).await)
        }

        // Job tools
        "job_status" => {
            let request: GetJobStatusRequest = parse_args(args)?;
            tool_to_json(server.get_job_status(request).await)
        }
        "job_list" => {
            tool_to_json(server.list_jobs().await)
        }
        "job_cancel" => {
            let request: CancelJobRequest = parse_args(args)?;
            tool_to_json(server.cancel_job(request).await)
        }
        "job_poll" => {
            let request: PollRequest = parse_args(args)?;
            tool_to_json(server.poll(request).await)
        }
        "job_sleep" => {
            let request: SleepRequest = parse_args(args)?;
            tool_to_json(server.sleep(request).await)
        }

        // Graph tools - these call audio_graph_mcp directly
        "graph_bind" => {
            let request: GraphBindRequest = parse_args(args)?;
            dispatch_graph_bind(server, request)
        }
        "graph_tag" => {
            let request: GraphTagRequest = parse_args(args)?;
            dispatch_graph_tag(server, request)
        }
        "graph_connect" => {
            let request: GraphConnectRequest = parse_args(args)?;
            dispatch_graph_connect(server, request)
        }
        "graph_find" => {
            let request: GraphFindRequest = parse_args(args)?;
            dispatch_graph_find(server, request)
        }
        "graph_context" => {
            let request: GraphContextRequest = parse_args(args)?;
            tool_to_json(server.graph_context(request).await)
        }
        "graph_query" => {
            let request: GraphQueryRequest = parse_args(args)?;
            tool_to_json(server.graph_query(request).await)
        }
        "add_annotation" => {
            let request: AddAnnotationRequest = parse_args(args)?;
            tool_to_json(server.add_annotation(request).await)
        }

        // ABC notation tools
        "abc_parse" => {
            let request: AbcParseRequest = parse_args(args)?;
            tool_to_json(server.abc_parse(request).await)
        }
        "abc_to_midi" => {
            let request: AbcToMidiRequest = parse_args(args)?;
            tool_to_json(server.abc_to_midi(request).await)
        }
        "abc_validate" => {
            let request: AbcValidateRequest = parse_args(args)?;
            tool_to_json(server.abc_validate(request).await)
        }
        "abc_transpose" => {
            let request: AbcTransposeRequest = parse_args(args)?;
            tool_to_json(server.abc_transpose(request).await)
        }

        // Analysis tools
        "beatthis_analyze" => {
            let request: AnalyzeBeatsRequest = parse_args(args)?;
            tool_to_json(server.analyze_beats(request).await)
        }
        "clap_analyze" => {
            let request: ClapAnalyzeRequest = parse_args(args)?;
            tool_to_json(server.clap_analyze(request).await)
        }

        // Generation tools
        "musicgen_generate" => {
            let request: MusicgenGenerateRequest = parse_args(args)?;
            tool_to_json(server.musicgen_generate(request).await)
        }
        "yue_generate" => {
            let request: YueGenerateRequest = parse_args(args)?;
            tool_to_json(server.yue_generate(request).await)
        }

        // Garden tools
        "garden_status" => {
            tool_to_json(server.garden_status().await)
        }
        "garden_play" => {
            tool_to_json(server.garden_play().await)
        }
        "garden_pause" => {
            tool_to_json(server.garden_pause().await)
        }
        "garden_stop" => {
            tool_to_json(server.garden_stop().await)
        }
        "garden_seek" => {
            let request: super::tools::garden::GardenSeekRequest = parse_args(args)?;
            tool_to_json(server.garden_seek(request).await)
        }
        "garden_set_tempo" => {
            let request: super::tools::garden::GardenSetTempoRequest = parse_args(args)?;
            tool_to_json(server.garden_set_tempo(request).await)
        }
        "garden_query" => {
            let request: super::tools::garden::GardenQueryRequest = parse_args(args)?;
            tool_to_json(server.garden_query(request).await)
        }
        "garden_emergency_pause" => {
            tool_to_json(server.garden_emergency_pause().await)
        }

        // Config tools
        "config_get" => {
            let request: super::tools::config::ConfigGetRequest = parse_args(args)?;
            tool_to_json(server.config_get(request).await)
        }

        "garden_create_region" => {
            let request: super::tools::garden::GardenCreateRegionRequest = parse_args(args)?;
            tool_to_json(server.garden_create_region(request).await)
        }
        "garden_delete_region" => {
            let request: super::tools::garden::GardenDeleteRegionRequest = parse_args(args)?;
            tool_to_json(server.garden_delete_region(request).await)
        }
        "garden_move_region" => {
            let request: super::tools::garden::GardenMoveRegionRequest = parse_args(args)?;
            tool_to_json(server.garden_move_region(request).await)
        }
        "garden_get_regions" => {
            let request: super::tools::garden::GardenGetRegionsRequest = parse_args(args)?;
            tool_to_json(server.garden_get_regions(request).await)
        }
        "garden_attach_audio" => {
            let request: super::tools::garden::GardenAttachAudioRequest = parse_args(args)?;
            tool_to_json(server.garden_attach_audio(request).await)
        }
        "garden_detach_audio" => {
            tool_to_json(server.garden_detach_audio().await)
        }
        "garden_audio_status" => {
            tool_to_json(server.garden_audio_status().await)
        }
        // Monitor input
        "garden_attach_input" => {
            let request: super::tools::garden::GardenAttachInputRequest = parse_args(args)?;
            tool_to_json(server.garden_attach_input(request).await)
        }
        "garden_detach_input" => {
            tool_to_json(server.garden_detach_input().await)
        }
        "garden_input_status" => {
            tool_to_json(server.garden_input_status().await)
        }
        "garden_set_monitor" => {
            let request: super::tools::garden::GardenSetMonitorRequest = parse_args(args)?;
            tool_to_json(server.garden_set_monitor(request).await)
        }

        _ => Err(DispatchError::not_found(name)),
    }
}

fn parse_args<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, DispatchError> {
    serde_json::from_value(args).map_err(|e| DispatchError::invalid_params(e.to_string()))
}

/// Convert a hooteproto ToolResult to JSON for ZMQ transport
fn tool_to_json(result: ToolResult) -> DispatchResult {
    match result {
        Ok(output) => {
            // Prefer structured data if available
            if output.data != Value::Null {
                Ok(output.data)
            } else {
                // Fall back to text as JSON
                Ok(serde_json::json!({ "text": output.text }))
            }
        }
        Err(e) => Err(e.into()),
    }
}

// Graph tool dispatchers that don't go through EventDualityServer
// (they call audio_graph_mcp directly)

fn dispatch_graph_bind(server: &EventDualityServer, request: GraphBindRequest) -> DispatchResult {
    let hints: Vec<(HintKind, String, f64)> = request
        .hints
        .into_iter()
        .filter_map(|h| {
            h.kind
                .parse::<HintKind>()
                .ok()
                .map(|kind| (kind, h.value, h.confidence))
        })
        .collect();

    match graph_bind(&server.audio_graph_db, &request.id, &request.name, hints) {
        Ok(identity) => Ok(serde_json::json!({
            "id": identity.id.0,
            "name": identity.name,
            "created_at": identity.created_at,
        })),
        Err(e) => Err(DispatchError::internal(e)),
    }
}

fn dispatch_graph_tag(server: &EventDualityServer, request: GraphTagRequest) -> DispatchResult {
    let add = vec![(request.namespace.clone(), request.value.clone())];
    match graph_tag(
        &server.audio_graph_db,
        &request.identity_id,
        add,
        vec![],
    ) {
        Ok(tags) => Ok(serde_json::json!({
            "identity_id": request.identity_id,
            "tags": tags.iter().map(|t| serde_json::json!({
                "namespace": t.namespace,
                "value": t.value,
            })).collect::<Vec<_>>(),
        })),
        Err(e) => Err(DispatchError::internal(e)),
    }
}

fn dispatch_graph_connect(
    server: &EventDualityServer,
    request: GraphConnectRequest,
) -> DispatchResult {
    match graph_connect(
        &server.audio_graph_db,
        &request.from_identity,
        &request.from_port,
        &request.to_identity,
        &request.to_port,
        request.transport.as_deref(),
    ) {
        Ok(conn) => Ok(serde_json::json!({
            "connection_id": conn.id,
            "from": format!("{}:{}", conn.from_identity.0, conn.from_port),
            "to": format!("{}:{}", conn.to_identity.0, conn.to_port),
            "transport": conn.transport_kind.unwrap_or_else(|| "unknown".to_string()),
        })),
        Err(e) => Err(DispatchError::internal(e)),
    }
}

fn dispatch_graph_find(server: &EventDualityServer, request: GraphFindRequest) -> DispatchResult {
    match graph_find(
        &server.audio_graph_db,
        request.name.as_deref(),
        request.tag_namespace.as_deref(),
        request.tag_value.as_deref(),
    ) {
        Ok(identities) => Ok(serde_json::json!({
            "identities": identities.iter().map(|i| serde_json::json!({
                "id": i.id,
                "name": i.name,
                "tags": i.tags.iter().map(|t| format!("{}:{}", t.namespace, t.value)).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "count": identities.len(),
        })),
        Err(e) => Err(DispatchError::internal(e)),
    }
}