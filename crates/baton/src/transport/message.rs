//! Message Handler
//!
//! Handles POST /message requests containing JSON-RPC messages.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response, sse::Event},
    Json,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

use super::McpState;
use crate::session::SseSender;
use crate::types::error::ErrorData;
use crate::types::jsonrpc::JsonRpcMessage;
use crate::Handler;

/// Query parameters for message endpoint.
#[derive(Debug, Deserialize)]
pub struct MessageParams {
    /// Session ID (required).
    #[serde(rename = "sessionId")]
    pub session_id: String,
}

/// JSON-RPC error response for HTTP errors.
#[derive(serde::Serialize)]
struct ErrorResponse {
    jsonrpc: &'static str,
    id: Value,
    error: ErrorData,
}

/// Handle incoming JSON-RPC messages.
///
/// Per MCP Streamable HTTP spec:
/// 1. Validate session exists
/// 2. Parse JSON-RPC request
/// 3. Dispatch to appropriate handler
/// 4. Send response via SSE channel
/// 5. Return 202 Accepted
#[tracing::instrument(skip(state, body), fields(session_id = %params.session_id))]
pub async fn message_handler<H: Handler>(
    State(state): State<Arc<McpState<H>>>,
    Query(params): Query<MessageParams>,
    Json(body): Json<Value>,
) -> Response {
    // Get session, touch it, and clone SSE sender before releasing lock.
    // This avoids holding the DashMap lock across async .await points.
    let tx: Option<SseSender> = {
        let mut session = match state.sessions.get_mut(&params.session_id) {
            Some(s) => s,
            None => {
                let error = ErrorResponse {
                    jsonrpc: "2.0",
                    id: Value::Null,
                    error: ErrorData::invalid_request("Session not found"),
                };
                return (StatusCode::NOT_FOUND, Json(error)).into_response();
            }
        };
        session.touch();
        session.tx.clone()
    };
    // Lock released here

    // Extract request ID for error responses
    let request_id = body
        .get("id")
        .cloned()
        .unwrap_or(Value::Null);

    // Check if this is a response (has result/error field, no method field)
    if body.get("result").is_some() || body.get("error").is_some() {
        // This is a response to a sampling request
        if let (Some(id), Some(result)) = (body.get("id"), body.get("result")) {
            if let Ok(sampling_response) = serde_json::from_value(result.clone()) {
                state.sampling_client.handle_response(&id.to_string(), sampling_response);
                return StatusCode::ACCEPTED.into_response();
            }
        }
        // Invalid response format
        return StatusCode::BAD_REQUEST.into_response();
    }

    // Parse JSON-RPC message (request or notification)
    let message: JsonRpcMessage = match serde_json::from_value(body) {
        Ok(r) => r,
        Err(e) => {
            let error = ErrorResponse {
                jsonrpc: "2.0",
                id: request_id,
                error: ErrorData::parse_error(format!("Invalid JSON-RPC: {}", e)),
            };
            return (StatusCode::BAD_REQUEST, Json(error)).into_response();
        }
    };

    let is_notification = message.is_notification();
    tracing::info!(
        method = %message.method,
        request_id = ?message.id,
        is_notification = is_notification,
        "Processing MCP message"
    );

    // Dispatch to protocol handler (no parent context for SSE messages)
    let result = crate::protocol::dispatch(
        &state,
        &params.session_id,
        &message,
        opentelemetry::Context::current(),
    )
    .await;

    // For notifications, no response is expected - just return 202
    if is_notification {
        return StatusCode::ACCEPTED.into_response();
    }

    // Build response JSON (only for requests with id)
    let response_json = match result {
        Ok(result_value) => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": message.id,
                "result": result_value
            })
        }
        Err(error) => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": message.id,
                "error": error
            })
        }
    };

    // Send response via SSE (using cloned sender, no lock held)
    let sse_event = Event::default()
        .event("message")
        .data(response_json.to_string());

    if let Some(sender) = tx {
        if let Err(e) = sender.send(Ok(sse_event)).await {
            tracing::warn!(error = ?e, "Failed to send response via SSE");
        }
    } else {
        tracing::warn!("No SSE connection for session, response dropped");
    }

    // Return 202 Accepted (response sent via SSE)
    StatusCode::ACCEPTED.into_response()
}
