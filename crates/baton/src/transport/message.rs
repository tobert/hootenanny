//! Message Handler
//!
//! Handles POST /message requests containing JSON-RPC messages.
//! Supports W3C Trace Context propagation via traceparent header.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response, sse::Event},
    Json,
};
use opentelemetry::global;
use opentelemetry_http::HeaderExtractor;
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
/// 1. Extract W3C Trace Context from traceparent header
/// 2. Validate session exists
/// 3. Parse JSON-RPC request
/// 4. Dispatch to appropriate handler with trace context
/// 5. Send response via SSE channel
/// 6. Return 202 Accepted
#[tracing::instrument(skip(state, body), fields(session_id = %params.session_id))]
pub async fn message_handler<H: Handler>(
    State(state): State<Arc<McpState<H>>>,
    Query(params): Query<MessageParams>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    // Extract W3C Trace Context from incoming traceparent header
    let parent_context = global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(&headers))
    });
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

    // Dispatch to protocol handler with parent trace context
    let result = crate::protocol::dispatch(
        &state,
        &params.session_id,
        &message,
        parent_context,
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

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use opentelemetry::trace::{TraceContextExt, TraceId, SpanId};
    use opentelemetry_sdk::propagation::TraceContextPropagator;

    #[test]
    fn test_message_handler_traceparent_extraction() {
        // Set up the global propagator for W3C Trace Context
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

        // Create a W3C traceparent header
        // Format: 00-{trace_id}-{span_id}-{flags}
        let trace_id = TraceId::from_hex("abcdef1234567890abcdef1234567890").unwrap();
        let span_id = SpanId::from_hex("1234567890abcdef").unwrap();
        let traceparent = format!("00-{}-{}-01", trace_id, span_id);

        // Create headers with traceparent
        let mut headers = HeaderMap::new();
        headers.insert("traceparent", traceparent.parse().unwrap());

        // Extract trace context (same logic as message_handler)
        let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&opentelemetry_http::HeaderExtractor(&headers))
        });

        // Verify the extracted context has the correct trace_id and span_id
        let span_ref = parent_context.span();
        let span_context = span_ref.span_context();
        assert!(span_context.is_valid(), "Extracted span context should be valid");
        assert_eq!(span_context.trace_id(), trace_id, "Trace ID should match");
        assert_eq!(span_context.span_id(), span_id, "Span ID should match");
        assert!(span_context.is_sampled(), "Should be sampled (flag 01)");
    }

    #[test]
    fn test_message_handler_traceparent_extraction_without_header() {
        // Set up the global propagator for W3C Trace Context
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

        // Create empty headers
        let headers = HeaderMap::new();

        // Extract trace context (should return empty context)
        let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&opentelemetry_http::HeaderExtractor(&headers))
        });

        // Verify the extracted context is invalid (no traceparent header)
        let span_ref = parent_context.span();
        let span_context = span_ref.span_context();
        assert!(!span_context.is_valid(), "Span context should be invalid without traceparent");
    }
}
