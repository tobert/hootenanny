//! MCP HTTP Handlers
//!
//! Implements the Streamable HTTP transport for MCP:
//! - GET /sse - Establish SSE connection
//! - POST /message - Send JSON-RPC requests

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures::stream::Stream;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, ClientRequest, ErrorCode,
    ErrorData, Implementation, InitializeRequest, InitializeResult, JsonRpcRequest,
    ListToolsResult, ProtocolVersion, ServerCapabilities, ServerResult, Tool,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{self, instrument};

use super::state::AppState;
use crate::api::schema::*;
use crate::api::service::EventDualityServer;

#[derive(Debug, Deserialize)]
pub struct SseQueryParams {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageQueryParams {
    #[serde(rename = "sessionId")]
    session_id: String,
}


#[derive(Debug, Serialize)]
struct JsonRpcErrorResponse {
    jsonrpc: &'static str,
    id: Value,
    error: ErrorData,
}

#[instrument(skip(state), fields(session_id = tracing::field::Empty))]
pub async fn sse_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SseQueryParams>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id_hint = params.session_id.or_else(|| {
        headers
            .get("Mcp-Session-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });

    let session_id = state.get_or_create_session(session_id_hint);
    tracing::Span::current().record("session_id", &session_id);

    let (tx, rx) = mpsc::channel::<Result<Event, axum::Error>>(32);

    state.register_connection(&session_id, tx.clone());

    let endpoint_data = serde_json::json!({
        "uri": format!("/mcp/message?sessionId={}", session_id)
    });

    let endpoint_event = Event::default()
        .event("endpoint")
        .data(endpoint_data.to_string());

    if tx.send(Ok(endpoint_event)).await.is_err() {
        tracing::warn!("Failed to send initial endpoint event");
    }

    let stream = ReceiverStream::new(rx).map(|result| match result {
        Ok(event) => Ok(event),
        Err(_) => Ok(Event::default().data("error")),
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}

#[instrument(skip(state, body), fields(session_id = %params.session_id))]
pub async fn message_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MessageQueryParams>,
    Json(body): Json<Value>,
) -> Response {
    let session = match state.sessions.get(&params.session_id) {
        Some(s) => s,
        None => {
            let error_response = JsonRpcErrorResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                error: ErrorData::new(
                    ErrorCode::INVALID_REQUEST,
                    "Session not found",
                    None,
                ),
            };
            return (StatusCode::NOT_FOUND, Json(error_response)).into_response();
        }
    };

    let request_id = body
        .get("id")
        .cloned()
        .unwrap_or(Value::Null);

    let message: JsonRpcRequest<ClientRequest> = match serde_json::from_value(body) {
        Ok(m) => m,
        Err(e) => {
            let error_response = JsonRpcErrorResponse {
                jsonrpc: "2.0",
                id: request_id,
                error: ErrorData::parse_error(format!("Invalid JSON-RPC: {}", e), None),
            };
            return (StatusCode::BAD_REQUEST, Json(error_response)).into_response();
        }
    };

    let result = dispatch_request(&state.server, &request_id, request_id.clone(), &request_id, &message.request).await;

    let response_json = match result {
        Ok(server_result) => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "result": server_result
            })
        }
        Err(error) => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": error
            })
        }
    };

    let sse_event = Event::default()
        .event("message")
        .data(response_json.to_string());

    if let Some(tx) = &session.tx {
        if tx.send(Ok(sse_event)).await.is_err() {
            tracing::warn!("Failed to send response via SSE");
        }
    }

    StatusCode::ACCEPTED.into_response()
}

async fn dispatch_request(
    server: &EventDualityServer,
    _session_id: &Value,
    _request_id_val: Value,
    _request_id: &Value,
    request: &ClientRequest,
) -> Result<ServerResult, ErrorData> {
    match request {
        ClientRequest::PingRequest(_) => {
            Ok(ServerResult::EmptyResult(rmcp::model::EmptyObject {}))
        }

        ClientRequest::InitializeRequest(req) => {
            let result = handle_initialize(req).await;
            Ok(ServerResult::InitializeResult(result))
        }

        ClientRequest::ListToolsRequest(_) => {
            let tools = get_tool_definitions();
            Ok(ServerResult::ListToolsResult(ListToolsResult::with_all_items(tools)))
        }

        ClientRequest::CallToolRequest(req) => {
            let result = dispatch_tool_call(server, &req.params).await?;
            Ok(ServerResult::CallToolResult(result))
        }

        _ => Err(ErrorData::new(
            ErrorCode::METHOD_NOT_FOUND,
            format!("Method not implemented: {:?}", std::mem::discriminant(request)),
            None,
        )),
    }
}

async fn handle_initialize(_req: &InitializeRequest) -> InitializeResult {
    InitializeResult {
        protocol_version: ProtocolVersion::LATEST,
        capabilities: ServerCapabilities::builder()
            .enable_tools()
            .build(),
        server_info: Implementation {
            name: "hootenanny".to_string(),
            title: Some("HalfRemembered MCP Server".to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),
            icons: None,
            website_url: Some("https://github.com/halfremembered".to_string()),
        },
        instructions: Some(
            "Hootenanny is an ensemble performance space for LLM agents and humans to create music together.".to_string()
        ),
    }
}

fn get_tool_definitions() -> Vec<Tool> {
    use serde_json::Map;
    let empty_schema = Map::new();

    vec![
        Tool::new("play", "Play a musical note with emotional expression", empty_schema.clone())
            .with_input_schema::<AddNodeRequest>(),
        Tool::new("add_node", "Add a node to the conversation tree", empty_schema.clone())
            .with_input_schema::<AddNodeRequest>(),
        Tool::new("fork_branch", "Fork the conversation to explore an alternative", empty_schema.clone())
            .with_input_schema::<ForkRequest>(),
        Tool::new("get_tree_status", "Get the current status of the conversation tree", empty_schema.clone()),
        Tool::new("cas_store", "Store content in the Content Addressable Storage", empty_schema.clone())
            .with_input_schema::<CasStoreRequest>(),
        Tool::new("cas_inspect", "Inspect content in the CAS by hash", empty_schema.clone())
            .with_input_schema::<CasInspectRequest>(),
        Tool::new("upload_file", "Upload a file to the CAS", empty_schema.clone())
            .with_input_schema::<UploadFileRequest>(),
        Tool::new("orpheus_generate", "Generate MIDI with the Orpheus model", empty_schema.clone())
            .with_input_schema::<OrpheusGenerateRequest>(),
        Tool::new("orpheus_generate_seeded", "Generate MIDI from a seed with Orpheus", empty_schema.clone())
            .with_input_schema::<OrpheusGenerateSeededRequest>(),
        Tool::new("orpheus_continue", "Continue existing MIDI with Orpheus", empty_schema.clone())
            .with_input_schema::<OrpheusContinueRequest>(),
        Tool::new("orpheus_bridge", "Create a bridge between MIDI sections", empty_schema.clone())
            .with_input_schema::<OrpheusBridgeRequest>(),
        Tool::new("get_job_status", "Get the status of an async job", empty_schema.clone())
            .with_input_schema::<GetJobStatusRequest>(),
        Tool::new("list_jobs", "List all jobs", empty_schema.clone()),
        Tool::new("cancel_job", "Cancel a running job", empty_schema.clone())
            .with_input_schema::<CancelJobRequest>(),
        Tool::new("poll", "Poll for job completion", empty_schema.clone())
            .with_input_schema::<PollRequest>(),
        Tool::new("sleep", "Sleep for a specified duration", empty_schema.clone())
            .with_input_schema::<SleepRequest>(),
        Tool::new("graph_bind", "Bind an identity in the audio graph", empty_schema.clone())
            .with_input_schema::<GraphBindRequest>(),
        Tool::new("graph_tag", "Tag an identity in the audio graph", empty_schema.clone())
            .with_input_schema::<GraphTagRequest>(),
        Tool::new("graph_connect", "Connect nodes in the audio graph", empty_schema.clone())
            .with_input_schema::<GraphConnectRequest>(),
        Tool::new("graph_find", "Find identities in the audio graph", empty_schema.clone())
            .with_input_schema::<GraphFindRequest>(),
    ]
}

async fn dispatch_tool_call(
    server: &EventDualityServer,
    params: &CallToolRequestParam,
) -> Result<CallToolResult, ErrorData> {
    let name = params.name.as_ref();
    let args = params.arguments.as_ref()
        .map(|a| serde_json::Value::Object(a.clone()))
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    match name {
        "play" => {
            let request: AddNodeRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.play(request).await
        }
        "add_node" => {
            let request: AddNodeRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.add_node(request).await
        }
        "fork_branch" => {
            let request: ForkRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.fork_branch(request).await
        }
        "get_tree_status" => {
            server.get_tree_status().await
        }
        "cas_store" => {
            let request: CasStoreRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.cas_store(request).await
        }
        "cas_inspect" => {
            let request: CasInspectRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.cas_inspect(request).await
        }
        "upload_file" => {
            let request: UploadFileRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.upload_file(request).await
        }
        "orpheus_generate" => {
            let request: OrpheusGenerateRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.orpheus_generate(request).await
        }
        "orpheus_generate_seeded" => {
            let request: OrpheusGenerateSeededRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.orpheus_generate_seeded(request).await
        }
        "orpheus_continue" => {
            let request: OrpheusContinueRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.orpheus_continue(request).await
        }
        "orpheus_bridge" => {
            let request: OrpheusBridgeRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.orpheus_bridge(request).await
        }
        "get_job_status" => {
            let request: GetJobStatusRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.get_job_status(request).await
        }
        "list_jobs" => {
            server.list_jobs().await
        }
        "cancel_job" => {
            let request: CancelJobRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.cancel_job(request).await
        }
        "poll" => {
            let request: PollRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.poll(request).await
        }
        "sleep" => {
            let request: SleepRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
            server.sleep(request).await
        }
        _ => Err(ErrorData::new(
            ErrorCode::METHOD_NOT_FOUND,
            format!("Unknown tool: {}", name),
            None,
        )),
    }
}

pub fn router() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/sse", axum::routing::get(sse_handler))
        .route("/message", axum::routing::post(message_handler))
}
