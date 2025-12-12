//! Tool dispatch for ZMQ server
//!
//! This module provides tool dispatch without MCP/baton dependencies.
//! It takes tool names and JSON arguments, executes the tool, and returns JSON results.

use crate::api::schema::*;
use crate::api::service::EventDualityServer;
use audio_graph_mcp::{graph_bind, graph_connect, graph_find, graph_tag, HintKind};
use serde_json::Value;

/// Error from tool dispatch
#[derive(Debug)]
pub struct ToolError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl ToolError {
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

/// Result of tool dispatch - either success with JSON or error
pub type DispatchResult = Result<Value, ToolError>;

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
            tool_result_to_json(server.cas_store(request).await)
        }
        "cas_inspect" => {
            let request: CasInspectRequest = parse_args(args)?;
            tool_result_to_json(server.cas_inspect(request).await)
        }
        "cas_upload_file" => {
            let request: UploadFileRequest = parse_args(args)?;
            tool_result_to_json(server.upload_file(request).await)
        }
        "artifact_upload" => {
            let request: ArtifactUploadRequest = parse_args(args)?;
            tool_result_to_json(server.artifact_upload(request).await)
        }

        // Conversion tools
        "convert_midi_to_wav" => {
            let request: MidiToWavRequest = parse_args(args)?;
            tool_result_to_json(server.midi_to_wav(request).await)
        }
        "soundfont_inspect" => {
            let request: SoundfontInspectRequest = parse_args(args)?;
            tool_result_to_json(server.soundfont_inspect(request).await)
        }
        "soundfont_preset_inspect" => {
            let request: SoundfontPresetInspectRequest = parse_args(args)?;
            tool_result_to_json(server.soundfont_preset_inspect(request).await)
        }

        // Orpheus tools
        "orpheus_generate" => {
            let request: OrpheusGenerateRequest = parse_args(args)?;
            tool_result_to_json(server.orpheus_generate(request).await)
        }
        "orpheus_generate_seeded" => {
            let request: OrpheusGenerateSeededRequest = parse_args(args)?;
            tool_result_to_json(server.orpheus_generate_seeded(request).await)
        }
        "orpheus_continue" => {
            let request: OrpheusContinueRequest = parse_args(args)?;
            tool_result_to_json(server.orpheus_continue(request).await)
        }
        "orpheus_bridge" => {
            let request: OrpheusBridgeRequest = parse_args(args)?;
            tool_result_to_json(server.orpheus_bridge(request).await)
        }
        "orpheus_loops" => {
            let request: OrpheusLoopsRequest = parse_args(args)?;
            tool_result_to_json(server.orpheus_loops(request).await)
        }
        "orpheus_classify" => {
            let request: OrpheusClassifyRequest = parse_args(args)?;
            tool_result_to_json(server.orpheus_classify(request).await)
        }

        // Job tools
        "job_status" => {
            let request: GetJobStatusRequest = parse_args(args)?;
            tool_result_to_json(server.get_job_status(request).await)
        }
        "job_list" => {
            tool_result_to_json(server.list_jobs().await)
        }
        "job_cancel" => {
            let request: CancelJobRequest = parse_args(args)?;
            tool_result_to_json(server.cancel_job(request).await)
        }
        "job_poll" => {
            let request: PollRequest = parse_args(args)?;
            tool_result_to_json(server.poll(request).await)
        }
        "job_sleep" => {
            let request: SleepRequest = parse_args(args)?;
            tool_result_to_json(server.sleep(request).await)
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
            tool_result_to_json(server.graph_context(request).await)
        }
        "graph_query" => {
            let request: GraphQueryRequest = parse_args(args)?;
            tool_result_to_json(server.graph_query(request).await)
        }
        "add_annotation" => {
            let request: AddAnnotationRequest = parse_args(args)?;
            tool_result_to_json(server.add_annotation(request).await)
        }

        // ABC notation tools
        "abc_parse" => {
            let request: AbcParseRequest = parse_args(args)?;
            tool_result_to_json(server.abc_parse(request).await)
        }
        "abc_to_midi" => {
            let request: AbcToMidiRequest = parse_args(args)?;
            tool_result_to_json(server.abc_to_midi(request).await)
        }
        "abc_validate" => {
            let request: AbcValidateRequest = parse_args(args)?;
            tool_result_to_json(server.abc_validate(request).await)
        }
        "abc_transpose" => {
            let request: AbcTransposeRequest = parse_args(args)?;
            tool_result_to_json(server.abc_transpose(request).await)
        }

        // Analysis tools
        "beatthis_analyze" => {
            let request: AnalyzeBeatsRequest = parse_args(args)?;
            tool_result_to_json(server.analyze_beats(request).await)
        }
        "clap_analyze" => {
            let request: ClapAnalyzeRequest = parse_args(args)?;
            tool_result_to_json(server.clap_analyze(request).await)
        }

        // Generation tools
        "musicgen_generate" => {
            let request: MusicgenGenerateRequest = parse_args(args)?;
            tool_result_to_json(server.musicgen_generate(request).await)
        }
        "yue_generate" => {
            let request: YueGenerateRequest = parse_args(args)?;
            tool_result_to_json(server.yue_generate(request).await)
        }

        // Garden tools
        "garden_status" => {
            tool_result_to_json(server.garden_status().await)
        }
        "garden_play" => {
            tool_result_to_json(server.garden_play().await)
        }
        "garden_pause" => {
            tool_result_to_json(server.garden_pause().await)
        }
        "garden_stop" => {
            tool_result_to_json(server.garden_stop().await)
        }
        "garden_seek" => {
            let request: super::tools::garden::GardenSeekRequest = parse_args(args)?;
            tool_result_to_json(server.garden_seek(request).await)
        }
        "garden_set_tempo" => {
            let request: super::tools::garden::GardenSetTempoRequest = parse_args(args)?;
            tool_result_to_json(server.garden_set_tempo(request).await)
        }
        "garden_query" => {
            let request: super::tools::garden::GardenQueryRequest = parse_args(args)?;
            tool_result_to_json(server.garden_query(request).await)
        }
        "garden_emergency_pause" => {
            tool_result_to_json(server.garden_emergency_pause().await)
        }

        _ => Err(ToolError::not_found(name)),
    }
}

fn parse_args<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, ToolError> {
    serde_json::from_value(args).map_err(|e| ToolError::invalid_params(e.to_string()))
}

/// Convert a baton CallToolResult to JSON
/// This is the bridge between baton types and our JSON output
fn tool_result_to_json(
    result: Result<baton::CallToolResult, baton::ErrorData>,
) -> DispatchResult {
    match result {
        Ok(call_result) => {
            // Prefer structured content if available
            if let Some(structured) = call_result.structured_content {
                return Ok(structured);
            }

            // Extract text content
            let texts: Vec<String> = call_result
                .content
                .iter()
                .filter_map(|c| match c {
                    baton::Content::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect();

            if call_result.is_error {
                Err(ToolError {
                    code: "tool_error".to_string(),
                    message: texts.join("\n"),
                    details: None,
                })
            } else if texts.len() == 1 {
                // Try to parse single text as JSON
                serde_json::from_str(&texts[0])
                    .unwrap_or_else(|_| serde_json::json!({ "text": texts[0] }))
                    .pipe(Ok)
            } else {
                Ok(serde_json::json!({ "texts": texts }))
            }
        }
        Err(e) => Err(ToolError {
            code: e.code.to_string(),
            message: e.message,
            details: e.data,
        }),
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
        Err(e) => Err(ToolError::internal(e)),
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
        Err(e) => Err(ToolError::internal(e)),
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
        Err(e) => Err(ToolError::internal(e)),
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
        Err(e) => Err(ToolError::internal(e)),
    }
}

/// Extension trait for pipe syntax
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

impl<T> Pipe for T {}
