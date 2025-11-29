//! Streamable HTTP Transport
//!
//! Implements the MCP Streamable HTTP transport:
//! - POST / - Send JSON-RPC request, receive response directly or as SSE stream
//! - Session ID via Mcp-Session-Id header

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use std::sync::Arc;

use super::McpState;
use crate::types::error::ErrorData;
use crate::types::jsonrpc::JsonRpcMessage;
use crate::Handler;

const SESSION_HEADER: &str = "mcp-session-id";

/// JSON-RPC error response.
#[derive(serde::Serialize)]
struct ErrorResponse {
    jsonrpc: &'static str,
    id: Value,
    error: ErrorData,
}

/// Handle Streamable HTTP requests.
///
/// Per MCP Streamable HTTP spec:
/// 1. Get or create session from Mcp-Session-Id header
/// 2. Parse JSON-RPC message (request or notification)
/// 3. Dispatch to appropriate handler
/// 4. Return response directly with session ID header (or 202 for notifications)
#[tracing::instrument(skip(state, body), fields(session_id = tracing::field::Empty))]
pub async fn streamable_handler<H: Handler>(
    State(state): State<Arc<McpState<H>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    // Get session ID from header, or create new session
    let session_id_hint = headers
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let session_id = state.sessions.get_or_create(session_id_hint.as_deref());
    tracing::Span::current().record("session_id", &session_id);

    // Update last_seen
    state.sessions.touch(&session_id);

    // Extract request ID for error responses (may be null for notifications)
    let request_id = body.get("id").cloned().unwrap_or(Value::Null);

    // Parse JSON-RPC message (can be request or notification)
    let message: JsonRpcMessage = match serde_json::from_value(body) {
        Ok(r) => r,
        Err(e) => {
            let error = ErrorResponse {
                jsonrpc: "2.0",
                id: request_id,
                error: ErrorData::parse_error(format!("Invalid JSON-RPC: {}", e)),
            };
            return build_response(StatusCode::BAD_REQUEST, &session_id, Json(error));
        }
    };

    // Handle notifications (no id = no response expected)
    if message.is_notification() {
        tracing::info!(
            method = %message.method,
            "Processing MCP notification (streamable)"
        );

        // Handle known notifications
        match message.method.as_str() {
            "notifications/initialized" => {
                tracing::info!(session_id = %session_id, "Client initialized notification received");
            }
            "notifications/cancelled" => {
                tracing::info!(session_id = %session_id, "Request cancelled notification received");
            }
            other => {
                tracing::debug!(method = %other, "Unknown notification received");
            }
        }

        // Return 202 Accepted for notifications (no body)
        return build_response_no_body(StatusCode::ACCEPTED, &session_id);
    }

    // It's a request - we need to respond
    let request_id = message.id.clone().expect("request has id");

    tracing::info!(
        method = %message.method,
        request_id = ?request_id,
        "Processing MCP request (streamable)"
    );

    // Dispatch to protocol handler (now accepts JsonRpcMessage directly)
    let result = crate::protocol::dispatch(&state, &session_id, &message).await;

    // Build response JSON
    let response_json = match result {
        Ok(result_value) => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "result": result_value
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

    build_response(StatusCode::OK, &session_id, Json(response_json))
}

/// Build response with session ID header.
fn build_response<T: IntoResponse>(status: StatusCode, session_id: &str, body: T) -> Response {
    let mut response = body.into_response();
    *response.status_mut() = status;

    if let Ok(header_value) = HeaderValue::from_str(session_id) {
        response
            .headers_mut()
            .insert(SESSION_HEADER, header_value);
    }

    response
}

/// Build response with session ID header but no body (for notifications).
fn build_response_no_body(status: StatusCode, session_id: &str) -> Response {
    let mut response = status.into_response();

    if let Ok(header_value) = HeaderValue::from_str(session_id) {
        response
            .headers_mut()
            .insert(SESSION_HEADER, header_value);
    }

    response
}

/// Handle DELETE requests (session termination).
#[tracing::instrument(skip(state), fields(session_id = tracing::field::Empty))]
pub async fn delete_handler<H: Handler>(
    State(state): State<Arc<McpState<H>>>,
    headers: HeaderMap,
) -> Response {
    let session_id = match headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) {
        Some(id) => id.to_string(),
        None => {
            return (StatusCode::BAD_REQUEST, "Missing Mcp-Session-Id header").into_response();
        }
    };

    tracing::Span::current().record("session_id", &session_id);

    // Remove the session
    state.sessions.remove(&session_id);

    tracing::info!(session_id = %session_id, "Session terminated");

    StatusCode::NO_CONTENT.into_response()
}
