//! MCP Streamable HTTP server
//!
//! Implements the MCP protocol over HTTP, routing tool calls to ZMQ backends.

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use hooteproto::{Broadcast, Payload, ToolInfo};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument};

use crate::backend::BackendPool;
use crate::telemetry;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub backends: Arc<BackendPool>,
    pub start_time: Instant,
    /// Broadcast channel for SSE events
    pub broadcast_tx: broadcast::Sender<Broadcast>,
}

/// JSON-RPC request wrapper
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC response wrapper
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Handle MCP JSON-RPC requests
#[instrument(skip(state, headers, request), fields(method = %request.method))]
pub async fn handle_mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Response {
    debug!("MCP request: {} {:?}", request.method, request.params);

    // Extract traceparent from incoming request, or generate one
    let traceparent = telemetry::extract_traceparent(&headers)
        .or_else(telemetry::current_traceparent);

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(&state, request.id, request.params).await,
        "tools/list" => handle_tools_list(&state, request.id).await,
        "tools/call" => handle_tools_call(&state, request.id, request.params, traceparent).await,
        "ping" => JsonRpcResponse::success(request.id, serde_json::json!({})),
        _ => JsonRpcResponse::error(
            request.id,
            -32601,
            format!("Method not found: {}", request.method),
        ),
    };

    Json(response).into_response()
}

async fn handle_initialize(_state: &AppState, id: Option<Value>, _params: Value) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "holler",
                "version": env!("CARGO_PKG_VERSION"),
            }
        }),
    )
}

async fn handle_tools_list(state: &AppState, id: Option<Value>) -> JsonRpcResponse {
    let mut all_tools: Vec<ToolInfo> = Vec::new();

    // Collect tools from all connected backends
    for (name, backend_opt) in [
        ("luanette", &state.backends.luanette),
        ("hootenanny", &state.backends.hootenanny),
        ("chaosgarden", &state.backends.chaosgarden),
    ] {
        if let Some(ref backend) = backend_opt {
            match backend.request(Payload::ListTools).await {
                Ok(Payload::ToolList { tools }) => {
                    debug!("Got {} tools from {}", tools.len(), name);
                    all_tools.extend(tools);
                }
                Ok(other) => {
                    error!("{} returned unexpected response to ListTools: {:?}", name, other);
                }
                Err(e) => {
                    error!("Failed to list tools from {}: {}", name, e);
                }
            }
        }
    }

    // Convert to MCP format
    let tools: Vec<Value> = all_tools
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
            })
        })
        .collect();

    JsonRpcResponse::success(id, serde_json::json!({ "tools": tools }))
}

async fn handle_tools_call(
    state: &AppState,
    id: Option<Value>,
    params: Value,
    traceparent: Option<String>,
) -> JsonRpcResponse {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Object(Default::default()));

    info!(tool = %name, traceparent = ?traceparent, "Tool call");

    // Route to backend
    let backend = match state.backends.route_tool(name) {
        Some(b) => b,
        None => {
            return JsonRpcResponse::error(
                id,
                -32602,
                format!("No backend available for tool: {}", name),
            );
        }
    };

    // Convert MCP tool call to hooteproto Payload
    let payload = match tool_to_payload(name, &arguments) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(id, -32602, format!("Invalid tool arguments: {}", e));
        }
    };

    // Send to backend with traceparent
    match backend.request_with_trace(payload, traceparent).await {
        Ok(Payload::Success { result }) => {
            JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result).unwrap_or_default(),
                    }],
                    "isError": false,
                }),
            )
        }
        Ok(Payload::Error { code, message, details }) => {
            let error_text = if let Some(d) = details {
                format!("{}: {}\n{}", code, message, serde_json::to_string_pretty(&d).unwrap_or_default())
            } else {
                format!("{}: {}", code, message)
            };
            JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": error_text,
                    }],
                    "isError": true,
                }),
            )
        }
        Ok(other) => {
            JsonRpcResponse::error(id, -32603, format!("Unexpected response: {:?}", other))
        }
        Err(e) => {
            JsonRpcResponse::error(id, -32603, format!("Backend error: {}", e))
        }
    }
}

/// Convert an MCP tool call to a hooteproto Payload
fn tool_to_payload(name: &str, args: &Value) -> anyhow::Result<Payload> {
    match name {
        // === Lua Tools (Luanette) ===
        "lua_eval" => Ok(Payload::LuaEval {
            code: args
                .get("code")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'code' argument"))?
                .to_string(),
            params: args.get("params").cloned(),
        }),

        "lua_describe" => Ok(Payload::LuaDescribe {
            script_hash: args
                .get("script_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'script_hash' argument"))?
                .to_string(),
        }),

        // === Job Tools (Luanette) ===
        "job_execute" => Ok(Payload::JobExecute {
            script_hash: args
                .get("script_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'script_hash' argument"))?
                .to_string(),
            params: args.get("params").cloned().unwrap_or(Value::Object(Default::default())),
            tags: args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
        }),

        "job_status" => Ok(Payload::JobStatus {
            job_id: args
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'job_id' argument"))?
                .to_string(),
        }),

        "job_list" => Ok(Payload::JobList {
            status: args.get("status").and_then(|v| v.as_str()).map(String::from),
        }),

        "job_cancel" => Ok(Payload::JobCancel {
            job_id: args
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'job_id' argument"))?
                .to_string(),
        }),

        "job_poll" => {
            let job_ids = args
                .get("job_ids")
                .and_then(|v| v.as_array())
                .ok_or_else(|| anyhow::anyhow!("Missing 'job_ids' argument"))?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            let timeout_ms = args
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(30000);

            let mode = match args.get("mode").and_then(|v| v.as_str()).unwrap_or("any") {
                "all" => hooteproto::PollMode::All,
                _ => hooteproto::PollMode::Any,
            };

            Ok(Payload::JobPoll {
                job_ids,
                timeout_ms,
                mode,
            })
        }

        // === Script Tools (Luanette) ===
        "script_store" => Ok(Payload::ScriptStore {
            content: args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'content' argument"))?
                .to_string(),
            tags: args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "script_search" => Ok(Payload::ScriptSearch {
            tag: args.get("tag").and_then(|v| v.as_str()).map(String::from),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
            vibe: args.get("vibe").and_then(|v| v.as_str()).map(String::from),
        }),

        // === CAS Tools (Hootenanny) ===
        "cas_store" => {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let data_str = args
                .get("data")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'data' argument (base64)"))?;
            let data = STANDARD
                .decode(data_str)
                .map_err(|e| anyhow::anyhow!("Invalid base64 data: {}", e))?;
            Ok(Payload::CasStore {
                data,
                mime_type: args.get("mime_type").and_then(|v| v.as_str()).map(String::from),
            })
        }

        "cas_inspect" => Ok(Payload::CasInspect {
            hash: args
                .get("hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'hash' argument"))?
                .to_string(),
        }),

        "cas_get" => Ok(Payload::CasGet {
            hash: args
                .get("hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'hash' argument"))?
                .to_string(),
        }),

        // === Artifact Tools (Hootenanny) ===
        "artifact_get" => Ok(Payload::ArtifactGet {
            id: args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'id' argument"))?
                .to_string(),
        }),

        "artifact_list" => Ok(Payload::ArtifactList {
            tag: args.get("tag").and_then(|v| v.as_str()).map(String::from),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "artifact_create" => Ok(Payload::ArtifactCreate {
            cas_hash: args
                .get("cas_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'cas_hash' argument"))?
                .to_string(),
            tags: args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
            metadata: args.get("metadata").cloned().unwrap_or(Value::Object(Default::default())),
        }),

        // === Graph Tools (Hootenanny) ===
        "graph_query" => Ok(Payload::GraphQuery {
            query: args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?
                .to_string(),
            variables: args.get("variables").cloned().unwrap_or(Value::Object(Default::default())),
        }),

        "graph_bind" => Ok(Payload::GraphBind {
            identity: args
                .get("identity")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'identity' argument"))?
                .to_string(),
            hints: args
                .get("hints")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
        }),

        // === Transport Tools (Chaosgarden) ===
        "transport_play" => Ok(Payload::TransportPlay),
        "transport_stop" => Ok(Payload::TransportStop),
        "transport_status" => Ok(Payload::TransportStatus),

        "transport_seek" => Ok(Payload::TransportSeek {
            position_beats: args
                .get("position_beats")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'position_beats' argument"))?,
        }),

        // === Timeline Tools (Chaosgarden) ===
        "timeline_query" => Ok(Payload::TimelineQuery {
            from_beats: args.get("from_beats").and_then(|v| v.as_f64()),
            to_beats: args.get("to_beats").and_then(|v| v.as_f64()),
        }),

        "timeline_add_marker" => Ok(Payload::TimelineAddMarker {
            position_beats: args
                .get("position_beats")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'position_beats' argument"))?,
            marker_type: args
                .get("marker_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'marker_type' argument"))?
                .to_string(),
            metadata: args.get("metadata").cloned().unwrap_or(Value::Object(Default::default())),
        }),

        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}

/// Health check endpoint
pub async fn handle_health(State(state): State<AppState>) -> Json<Value> {
    let uptime = state.start_time.elapsed();

    Json(serde_json::json!({
        "status": "healthy",
        "uptime_secs": uptime.as_secs(),
        "version": env!("CARGO_PKG_VERSION"),
        "backends": {
            "luanette": state.backends.luanette.is_some(),
            "hootenanny": state.backends.hootenanny.is_some(),
            "chaosgarden": state.backends.chaosgarden.is_some(),
        }
    }))
}
