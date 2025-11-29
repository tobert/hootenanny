//! SSE Handler
//!
//! Handles GET /sse requests to establish SSE connections.

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use super::McpState;
use crate::Handler;

/// Query parameters for SSE endpoint.
#[derive(Debug, Deserialize)]
pub struct SseParams {
    /// Optional session ID to resume.
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

/// Handle SSE connection requests.
///
/// Per MCP Streamable HTTP spec:
/// 1. Accept session ID from query param or header
/// 2. Create or resume session
/// 3. Send endpoint event with POST URL
/// 4. Keep connection alive with pings
#[tracing::instrument(skip(state), fields(session_id = tracing::field::Empty))]
pub async fn sse_handler<H: Handler>(
    State(state): State<Arc<McpState<H>>>,
    Query(params): Query<SseParams>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Get session ID from query param or header
    let session_id_hint = params.session_id.or_else(|| {
        headers
            .get("Mcp-Session-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    });

    // Get or create session
    let session_id = state.sessions.get_or_create(session_id_hint.as_deref());
    tracing::Span::current().record("session_id", &session_id);

    // Create SSE channel
    let (tx, rx) = mpsc::channel::<Result<Event, axum::Error>>(32);

    // Register SSE connection with session
    state.sessions.register_sse(&session_id, tx.clone());

    // Send initial endpoint event
    let endpoint_data = serde_json::json!({
        "uri": format!("/mcp/message?sessionId={}", session_id)
    });

    let endpoint_event = Event::default()
        .event("endpoint")
        .data(endpoint_data.to_string());

    if tx.send(Ok(endpoint_event)).await.is_err() {
        tracing::warn!("Failed to send initial endpoint event");
    }

    tracing::info!(session_id = %session_id, "SSE connection established");

    // Convert channel to stream
    let stream = ReceiverStream::new(rx).map(|result| match result {
        Ok(event) => Ok(event),
        Err(_) => Ok(Event::default().data("error")),
    });

    // Return SSE with keep-alive
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    )
}
