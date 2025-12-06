//! Streamable HTTP Transport
//!
//! Implements the MCP Streamable HTTP transport:
//! - POST / - Send JSON-RPC request, receive response directly or as SSE stream
//! - Session ID via Mcp-Session-Id header
//! - W3C Trace Context propagation via traceparent header

use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use opentelemetry::global;
use opentelemetry_http::HeaderExtractor;
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
    // Extract W3C Trace Context from incoming traceparent header
    let parent_context = global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(&headers))
    });

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

    // Dispatch to protocol handler with parent trace context
    let result = crate::protocol::dispatch(&state, &session_id, &message, parent_context).await;

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

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use opentelemetry::trace::{TraceContextExt, TraceId, SpanId};
    use opentelemetry_sdk::propagation::TraceContextPropagator;

    #[test]
    fn test_traceparent_extraction() {
        // Set up the global propagator for W3C Trace Context
        opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

        // Create a W3C traceparent header
        // Format: 00-{trace_id}-{span_id}-{flags}
        let trace_id = TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736").unwrap();
        let span_id = SpanId::from_hex("00f067aa0ba902b7").unwrap();
        let traceparent = format!("00-{}-{}-01", trace_id, span_id);

        // Create headers with traceparent
        let mut headers = HeaderMap::new();
        headers.insert("traceparent", traceparent.parse().unwrap());

        // Extract trace context (this is what streamable_handler does)
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
    fn test_traceparent_extraction_without_header() {
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
