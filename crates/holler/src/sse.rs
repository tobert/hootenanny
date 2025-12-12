//! SSE endpoint for server-initiated notifications
//!
//! Clients connect via GET /sse to receive broadcast events from backends.
//! Events are forwarded from ZMQ SUB sockets connected to backend PUB sockets.

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use hooteproto::Broadcast;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::{debug, info};

use crate::mcp::AppState;

/// Handle SSE connection requests.
///
/// Clients connect here to receive broadcast events from all backends.
/// Events include job completions, artifact creation, timeline events, etc.
#[tracing::instrument(skip(state))]
pub async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("SSE client connected");

    // Subscribe to broadcast channel
    let rx = state.broadcast_tx.subscribe();

    // Convert broadcast receiver to SSE event stream
    let stream = BroadcastStream::new(rx)
        .filter_map(|result| {
            match result {
                Ok(broadcast) => {
                    // Convert Broadcast to SSE Event
                    let event = broadcast_to_sse_event(&broadcast);
                    Some(Ok(event))
                }
                Err(e) => {
                    debug!("Broadcast receive error: {}", e);
                    None
                }
            }
        });

    // Return SSE with keep-alive pings
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"),
    )
}

/// Convert a hooteproto Broadcast to an SSE Event
fn broadcast_to_sse_event(broadcast: &Broadcast) -> Event {
    let (event_type, data) = match broadcast {
        Broadcast::ConfigUpdate { key, value } => (
            "config_update",
            serde_json::json!({ "key": key, "value": value }),
        ),
        Broadcast::Shutdown { reason } => (
            "shutdown",
            serde_json::json!({ "reason": reason }),
        ),
        Broadcast::ScriptInvalidate { hash } => (
            "script_invalidate",
            serde_json::json!({ "hash": hash }),
        ),
        Broadcast::JobStateChanged { job_id, state, result } => (
            "job_state_changed",
            serde_json::json!({
                "job_id": job_id,
                "state": state,
                "result": result,
            }),
        ),
        Broadcast::ArtifactCreated { artifact_id, content_hash, tags, creator } => (
            "artifact_created",
            serde_json::json!({
                "artifact_id": artifact_id,
                "content_hash": content_hash,
                "tags": tags,
                "creator": creator,
            }),
        ),
        Broadcast::TransportStateChanged { state, position_beats, tempo_bpm } => (
            "transport_state_changed",
            serde_json::json!({
                "state": state,
                "position_beats": position_beats,
                "tempo_bpm": tempo_bpm,
            }),
        ),
        Broadcast::MarkerReached { position_beats, marker_type, metadata } => (
            "marker_reached",
            serde_json::json!({
                "position_beats": position_beats,
                "marker_type": marker_type,
                "metadata": metadata,
            }),
        ),
        Broadcast::BeatTick { beat, position_beats, tempo_bpm } => (
            "beat_tick",
            serde_json::json!({
                "beat": beat,
                "position_beats": position_beats,
                "tempo_bpm": tempo_bpm,
            }),
        ),
        Broadcast::Log { level, message, source } => (
            "log",
            serde_json::json!({
                "level": level,
                "message": message,
                "source": source,
            }),
        ),
    };

    Event::default()
        .event(event_type)
        .data(data.to_string())
}

/// Create a broadcast channel for SSE events
pub fn create_broadcast_channel() -> (broadcast::Sender<Broadcast>, broadcast::Receiver<Broadcast>) {
    broadcast::channel(256)
}
