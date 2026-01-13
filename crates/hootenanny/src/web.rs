//! Web endpoints for Hootenanny.
//!
//! Provides HTTP access to artifacts. Content is served through artifact IDs,
//! with CAS as an internal implementation detail.
//!
//! Note: MCP handlers have migrated to the baton crate.

use crate::artifact_store::{ArtifactStore, FileStore};
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use cas::{ContentStore, FileStore as CasFileStore};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tokio_util::io::ReaderStream;

/// Shared state for web handlers
#[derive(Clone)]
pub struct WebState {
    pub artifact_store: Arc<RwLock<FileStore>>,
    pub cas: Arc<CasFileStore>,
    /// Optional connection to chaosgarden for live audio streaming
    pub garden_manager: Option<Arc<crate::zmq::GardenManager>>,
}

pub fn router(state: WebState) -> Router {
    Router::new()
        .route("/artifact/{id}", get(download_artifact))
        .route("/artifact/{id}/meta", get(artifact_meta))
        .route("/artifacts", get(list_artifacts))
        .route("/ui", get(serve_ui))
        .route("/stream/live", get(stream_live_ws))
        .route("/stream/live/status", get(stream_status))
        .route("/", get(serve_root))
        .with_state(state)
}

/// Serve root discovery endpoint
async fn serve_root() -> impl IntoResponse {
    let links = serde_json::json!({
        "name": "Hootenanny",
        "version": env!("CARGO_PKG_VERSION"),
        "links": {
            "ui": "/ui",
            "artifacts": "/artifacts",
            "health": "/health",
        }
    });
    Json(links)
}

/// Serve the UI page
async fn serve_ui() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(UI_HTML.to_string())
        .unwrap()
}

/// WebSocket handler for live audio streaming
async fn stream_live_ws(
    State(state): State<WebState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_live_stream(socket, state.garden_manager))
}

/// Handle a live stream WebSocket connection
async fn handle_live_stream(
    socket: WebSocket,
    garden_manager: Option<Arc<crate::zmq::GardenManager>>,
) {
    use hooteproto::garden::{ShellReply, ShellRequest};

    let (mut sender, mut receiver) = socket.split();

    // Wait for client to send start message
    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            if let Ok(cmd) = serde_json::from_str::<StreamCommand>(&text) {
                match cmd.r#type.as_str() {
                    "start" => {
                        tracing::info!("Live stream started, buffer_ms: {:?}", cmd.buffer_ms);
                        break;
                    }
                    "stop" => {
                        tracing::info!("Live stream stopped by client");
                        return;
                    }
                    _ => {}
                }
            }
        }
    }

    // Default audio parameters
    let default_sample_rate: u32 = 48000;
    let default_channels: u16 = 2;
    let default_format: u16 = 0; // f32le
    let frames_per_request: u32 = 512; // ~10.7ms at 48kHz

    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(10));

    loop {
        interval.tick().await;

        // Try to get audio from chaosgarden
        let (sample_rate, channels, format, samples) = if let Some(ref manager) = garden_manager {
            match manager.request(ShellRequest::GetAudioSnapshot { frames: frames_per_request }).await {
                Ok(ShellReply::AudioSnapshot { sample_rate, channels, format, samples }) => {
                    (sample_rate, channels, format, samples)
                }
                Ok(_) => {
                    // Unexpected reply - send silence
                    (default_sample_rate, default_channels, default_format, vec![0.0f32; frames_per_request as usize * 2])
                }
                Err(e) => {
                    tracing::debug!("Audio snapshot error: {}", e);
                    (default_sample_rate, default_channels, default_format, vec![0.0f32; frames_per_request as usize * 2])
                }
            }
        } else {
            // No garden manager - send silence
            (default_sample_rate, default_channels, default_format, vec![0.0f32; frames_per_request as usize * 2])
        };

        // Build packet: 8-byte header + PCM samples
        let mut packet = Vec::with_capacity(8 + samples.len() * 4);

        // Header: sample_rate (u32) + channels (u16) + format (u16)
        packet.extend_from_slice(&sample_rate.to_le_bytes());
        packet.extend_from_slice(&channels.to_le_bytes());
        packet.extend_from_slice(&format.to_le_bytes());

        // PCM samples (interleaved stereo f32)
        for sample in &samples {
            packet.extend_from_slice(&sample.to_le_bytes());
        }

        if sender.send(Message::Binary(packet.into())).await.is_err() {
            break;
        }

        // Check for stop command
        if let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout(tokio::time::Duration::from_millis(1), receiver.next()).await
        {
            if let Ok(cmd) = serde_json::from_str::<StreamCommand>(&text) {
                if cmd.r#type == "stop" {
                    tracing::info!("Live stream stopped by client");
                    break;
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct StreamCommand {
    r#type: String,
    buffer_ms: Option<u32>,
}

/// Get stream status
async fn stream_status(State(state): State<WebState>) -> impl IntoResponse {
    let connected = state.garden_manager.is_some();
    Json(serde_json::json!({
        "status": if connected { "available" } else { "no_backend" },
        "sample_rate": 48000,
        "channels": 2,
        "format": "f32le",
        "backend": if connected { "chaosgarden" } else { "none" }
    }))
}

/// HTML template for the artifact browser UI
const UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Hootenanny</title>
  <style>
    :root { --bg: #1a1a2e; --card: #16213e; --accent: #e94560; --text: #eee; --muted: #888; }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: system-ui, -apple-system, sans-serif; background: var(--bg); color: var(--text); padding: 1rem; min-height: 100vh; }
    .header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; flex-wrap: wrap; gap: 0.5rem; }
    h1 { font-size: 1.5rem; display: flex; align-items: center; gap: 0.5rem; }
    .live-toggle { background: var(--accent); border: none; color: white; padding: 0.5rem 1rem; border-radius: 4px; cursor: pointer; font-size: 0.9rem; }
    .live-toggle:hover { opacity: 0.9; }
    .live-toggle.active { background: #0c6; }
    .filters { display: flex; gap: 0.5rem; margin-bottom: 1rem; flex-wrap: wrap; }
    .filters input, .filters select { padding: 0.5rem; border: 1px solid #333; border-radius: 4px; background: var(--card); color: var(--text); font-size: 0.9rem; }
    .filters input { flex: 1; min-width: 150px; }
    .artifacts { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 1rem; }
    .artifact { background: var(--card); padding: 1rem; border-radius: 8px; }
    .artifact-type { font-size: 0.7rem; color: var(--accent); text-transform: uppercase; letter-spacing: 0.05em; }
    .artifact-id { font-weight: 600; margin: 0.25rem 0; word-break: break-all; font-size: 0.95rem; }
    .artifact-meta { font-size: 0.8rem; color: var(--muted); margin-bottom: 0.5rem; }
    .artifact audio { width: 100%; margin-top: 0.5rem; height: 36px; }
    .artifact a { color: var(--accent); text-decoration: none; font-size: 0.85rem; }
    .artifact a:hover { text-decoration: underline; }
    .live-section { background: var(--card); padding: 1rem; border-radius: 8px; margin-bottom: 1rem; }
    .live-section h3 { font-size: 1rem; margin-bottom: 0.5rem; }
    .visualizer { height: 60px; background: #0a0a15; border-radius: 4px; margin-bottom: 0.5rem; }
    .status { font-size: 0.8rem; color: var(--muted); }
    .empty { text-align: center; padding: 3rem; color: var(--muted); }
    .tag { display: inline-block; background: #2a2a4e; padding: 0.15rem 0.4rem; border-radius: 3px; font-size: 0.7rem; margin-right: 0.25rem; margin-top: 0.25rem; }
  </style>
</head>
<body>
  <div class="header">
    <h1>🎵 Hootenanny</h1>
    <button class="live-toggle" id="liveToggle" title="Stream live audio output">▶ Live</button>
  </div>

  <div class="live-section" id="liveSection" style="display:none;">
    <h3>Live Output</h3>
    <canvas class="visualizer" id="visualizer"></canvas>
    <div class="status" id="streamStatus">Click Live to connect...</div>
  </div>

  <div class="filters">
    <input type="search" id="search" placeholder="Search artifacts...">
    <select id="typeFilter">
      <option value="">All types</option>
      <option value="audio">Audio</option>
      <option value="midi">MIDI</option>
      <option value="soundfont">SoundFont</option>
    </select>
    <select id="creatorFilter">
      <option value="">All creators</option>
    </select>
  </div>

  <div class="artifacts" id="artifactList">
    <div class="empty">Loading artifacts...</div>
  </div>

  <script>
    const API_BASE = '';
    let allArtifacts = [];
    let ws = null;
    let audioCtx = null;
    let workletNode = null;

    async function loadArtifacts() {
      try {
        const res = await fetch(`${API_BASE}/artifacts?limit=200`);
        allArtifacts = await res.json();
        populateCreatorFilter();
        renderArtifacts();
      } catch (e) {
        document.getElementById('artifactList').innerHTML = '<div class="empty">Failed to load artifacts</div>';
      }
    }

    function populateCreatorFilter() {
      const creators = [...new Set(allArtifacts.map(a => a.creator))].sort();
      const select = document.getElementById('creatorFilter');
      creators.forEach(c => {
        const opt = document.createElement('option');
        opt.value = c;
        opt.textContent = c;
        select.appendChild(opt);
      });
    }

    function renderArtifacts() {
      const search = document.getElementById('search').value.toLowerCase();
      const typeFilter = document.getElementById('typeFilter').value;
      const creatorFilter = document.getElementById('creatorFilter').value;

      const filtered = allArtifacts.filter(a => {
        if (search && !a.id.toLowerCase().includes(search) && !a.tags.some(t => t.toLowerCase().includes(search))) return false;
        if (typeFilter && !a.tags.some(t => t === `type:${typeFilter}`)) return false;
        if (creatorFilter && a.creator !== creatorFilter) return false;
        return true;
      });

      const list = document.getElementById('artifactList');
      if (filtered.length === 0) {
        list.innerHTML = '<div class="empty">No artifacts found</div>';
        return;
      }

      list.innerHTML = filtered.map(a => `
        <div class="artifact">
          <div class="artifact-type">${getType(a.tags)}</div>
          <div class="artifact-id">${a.id}</div>
          <div class="artifact-meta">${a.creator} · ${new Date(a.created_at).toLocaleDateString()}</div>
          <div>${a.tags.map(t => `<span class="tag">${t}</span>`).join('')}</div>
          ${isAudio(a.tags) ? `<audio controls preload="none" src="${a.content_url}"></audio>` : `<a href="${a.content_url}" target="_blank">Download</a>`}
        </div>
      `).join('');
    }

    function getType(tags) {
      const typeTag = tags.find(t => t.startsWith('type:'));
      return typeTag ? typeTag.split(':')[1] : 'unknown';
    }

    function isAudio(tags) {
      return tags.some(t => t === 'type:audio' || t === 'type:wav' || t.includes('audio'));
    }

    // Live streaming with AudioWorklet
    async function startLive() {
      if (!audioCtx) {
        audioCtx = new AudioContext({ sampleRate: 48000 });
        const workletCode = `
          class PCMProcessor extends AudioWorkletProcessor {
            constructor() {
              super();
              this.buffer = [];
              this.port.onmessage = e => {
                this.buffer.push(...e.data);
                if (this.buffer.length > 48000) this.buffer.splice(0, this.buffer.length - 48000);
              };
            }
            process(inputs, outputs) {
              const out = outputs[0];
              const needed = out[0].length * 2;
              for (let ch = 0; ch < out.length; ch++) {
                for (let i = 0; i < out[ch].length; i++) {
                  const idx = i * 2 + ch;
                  out[ch][i] = idx < this.buffer.length ? this.buffer[idx] : 0;
                }
              }
              if (this.buffer.length >= needed) this.buffer.splice(0, needed);
              return true;
            }
          }
          registerProcessor('pcm-processor', PCMProcessor);
        `;
        const blob = new Blob([workletCode], { type: 'application/javascript' });
        await audioCtx.audioWorklet.addModule(URL.createObjectURL(blob));
        workletNode = new AudioWorkletNode(audioCtx, 'pcm-processor');
        workletNode.connect(audioCtx.destination);
      }

      const wsUrl = `wss://${location.host}/stream/live`;
      ws = new WebSocket(wsUrl);
      ws.binaryType = 'arraybuffer';
      document.getElementById('streamStatus').textContent = 'Connecting...';

      ws.onopen = () => {
        ws.send(JSON.stringify({ type: 'start', buffer_ms: 150 }));
        document.getElementById('streamStatus').textContent = 'Connected - streaming audio';
      };

      ws.onmessage = (e) => {
        if (e.data instanceof ArrayBuffer && e.data.byteLength > 8) {
          const samples = new Float32Array(e.data, 8);
          workletNode.port.postMessage(Array.from(samples));
        }
      };

      ws.onerror = () => {
        document.getElementById('streamStatus').textContent = 'Connection error';
      };

      ws.onclose = () => {
        document.getElementById('streamStatus').textContent = 'Disconnected';
      };
    }

    function stopLive() {
      if (ws) { ws.close(); ws = null; }
      document.getElementById('streamStatus').textContent = 'Click Live to connect...';
    }

    document.getElementById('liveToggle').onclick = function() {
      const section = document.getElementById('liveSection');
      if (this.classList.toggle('active')) {
        section.style.display = 'block';
        this.textContent = '⏹ Stop';
        startLive();
      } else {
        section.style.display = 'none';
        this.textContent = '▶ Live';
        stopLive();
      }
    };

    document.getElementById('search').oninput = renderArtifacts;
    document.getElementById('typeFilter').onchange = renderArtifacts;
    document.getElementById('creatorFilter').onchange = renderArtifacts;

    loadArtifacts();
  </script>
</body>
</html>
"##;

/// Download artifact content
///
/// Resolves artifact ID to CAS content and streams it with the correct MIME type.
/// Records access in the artifact for tracking.
#[tracing::instrument(
    name = "http.artifact.content",
    skip(state),
    fields(
        artifact.id = %id,
        artifact.content_hash = tracing::field::Empty,
        artifact.creator = tracing::field::Empty,
        artifact.access_count = tracing::field::Empty,
    )
)]
async fn download_artifact(State(state): State<WebState>, Path(id): Path<String>) -> Response {
    // Get artifact and update access
    let (content_hash, mime_type, path, access_count, artifact_id_str) = {
        let store = match state.artifact_store.write() {
            Ok(s) => s,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        let mut artifact = match store.get(&id) {
            Ok(Some(a)) => a,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };

        // Record access
        artifact.record_access();
        let access_count = artifact.access_count;
        let content_hash = artifact.content_hash.clone();
        let creator = artifact.creator.clone();
        let artifact_id_str = artifact.id.as_str().to_string();

        // Persist updated artifact
        if let Err(e) = store.put(artifact) {
            tracing::warn!("Failed to persist access update: {}", e);
        }
        if let Err(e) = store.flush() {
            tracing::warn!("Failed to flush artifact store: {}", e);
        }

        // Record in span
        let span = tracing::Span::current();
        span.record("artifact.content_hash", content_hash.as_str());
        span.record("artifact.creator", &creator);
        span.record("artifact.access_count", access_count);

        // Get CAS info
        let cas_hash: cas::ContentHash = match content_hash.as_str().parse() {
            Ok(h) => h,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let cas_ref = match state.cas.inspect(&cas_hash) {
            Ok(Some(r)) => r,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        let path = match state.cas.path(&cas_hash) {
            Some(p) => p,
            None => return StatusCode::NOT_FOUND.into_response(),
        };

        (
            content_hash,
            cas_ref.mime_type,
            path,
            access_count,
            artifact_id_str,
        )
    };

    // Stream content
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header("X-Artifact-Id", artifact_id_str)
        .header("X-Content-Hash", content_hash.as_str())
        .header("X-Access-Count", access_count.to_string())
        .body(body)
        .map_err(|e| {
            tracing::error!("Failed to build response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .unwrap_or_else(|status| status.into_response())
}

/// Artifact metadata response
#[derive(Serialize)]
struct ArtifactMetaResponse {
    id: String,
    content_hash: String,
    content_url: String,
    mime_type: Option<String>,
    size_bytes: Option<u64>,
    creator: String,
    created_at: String,
    tags: Vec<String>,
    variation_set_id: Option<String>,
    variation_index: Option<u32>,
    parent_id: Option<String>,
    access_count: u64,
    last_accessed: Option<String>,
    metadata: serde_json::Value,
}

/// Get artifact metadata as JSON
#[tracing::instrument(name = "http.artifact.meta", skip(state))]
async fn artifact_meta(State(state): State<WebState>, Path(id): Path<String>) -> impl IntoResponse {
    let store = match state.artifact_store.read() {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "lock poisoned"})),
            )
        }
    };

    match store.get(&id) {
        Ok(Some(artifact)) => {
            // Get CAS metadata for MIME type and size
            let (mime_type, size_bytes) = {
                let cas_hash: Result<cas::ContentHash, _> = artifact.content_hash.as_str().parse();
                match cas_hash.and_then(|h| {
                    state
                        .cas
                        .inspect(&h)
                        .map_err(|_| cas::HashError::InvalidLength(0))
                }) {
                    Ok(Some(r)) => (Some(r.mime_type), Some(r.size_bytes)),
                    _ => (None, None),
                }
            };

            let response = ArtifactMetaResponse {
                id: artifact.id.as_str().to_string(),
                content_hash: artifact.content_hash.as_str().to_string(),
                content_url: format!("/artifact/{}", artifact.id.as_str()),
                mime_type,
                size_bytes,
                creator: artifact.creator.clone(),
                created_at: artifact.created_at.to_rfc3339(),
                tags: artifact.tags.clone(),
                variation_set_id: artifact
                    .variation_set_id
                    .as_ref()
                    .map(|s| s.as_str().to_string()),
                variation_index: artifact.variation_index,
                parent_id: artifact.parent_id.as_ref().map(|s| s.as_str().to_string()),
                access_count: artifact.access_count,
                last_accessed: artifact.last_accessed.map(|t| t.to_rfc3339()),
                metadata: artifact.metadata.clone(),
            };

            (
                StatusCode::OK,
                Json(serde_json::to_value(response).unwrap_or_else(|e| {
                    tracing::error!("Failed to serialize artifact metadata: {}", e);
                    serde_json::json!({"error": "serialization failed"})
                })),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// Query parameters for listing artifacts
#[derive(Debug, Deserialize)]
struct ListQuery {
    tag: Option<String>,
    creator: Option<String>,
    limit: Option<usize>,
}

/// Artifact summary for list response
#[derive(Serialize)]
struct ArtifactSummary {
    id: String,
    content_hash: String,
    content_url: String,
    creator: String,
    created_at: String,
    tags: Vec<String>,
    access_count: u64,
}

/// List artifacts with optional filtering
#[tracing::instrument(name = "http.artifacts.list", skip(state))]
async fn list_artifacts(
    State(state): State<WebState>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let store = match state.artifact_store.read() {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "lock poisoned"})),
            )
        }
    };

    let all = match store.all() {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        }
    };

    let filtered: Vec<ArtifactSummary> = all
        .into_iter()
        .filter(|a| query.tag.as_ref().is_none_or(|t| a.has_tag(t)))
        .filter(|a| query.creator.as_ref().is_none_or(|c| &a.creator == c))
        .take(query.limit.unwrap_or(100))
        .map(|a| ArtifactSummary {
            id: a.id.as_str().to_string(),
            content_hash: a.content_hash.as_str().to_string(),
            content_url: format!("/artifact/{}", a.id.as_str()),
            creator: a.creator,
            created_at: a.created_at.to_rfc3339(),
            tags: a.tags,
            access_count: a.access_count,
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::to_value(filtered).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize artifact list: {}", e);
            serde_json::json!({"error": "serialization failed"})
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact_store::Artifact;
    use crate::types::{ArtifactId, ContentHash};
    use axum::body::to_bytes;
    use axum::http::Request;
    use tempfile::TempDir;
    use tower::ServiceExt;

    async fn setup_test_state() -> (WebState, TempDir) {
        let temp_dir = TempDir::new().unwrap();

        // Create CAS
        let cas_path = temp_dir.path().join("cas");
        let cas = CasFileStore::at_path(&cas_path).unwrap();

        // Store some content
        let content = b"Hello, artifact world!";
        let hash = cas.store(content, "text/plain").unwrap();

        // Create artifact store
        let artifact_path = temp_dir.path().join("artifacts.json");
        let artifact_store = FileStore::new(&artifact_path).unwrap();

        // Create an artifact pointing to the content
        let artifact = Artifact::new(
            ArtifactId::new("test_artifact"),
            ContentHash::new(hash.as_str()),
            "test_creator",
            serde_json::json!({"test": true}),
        )
        .with_tags(vec!["type:text", "test:yes"]);

        artifact_store.put(artifact).unwrap();
        artifact_store.flush().unwrap();

        let state = WebState {
            artifact_store: Arc::new(RwLock::new(FileStore::new(&artifact_path).unwrap())),
            cas: Arc::new(cas),
            garden_manager: None, // No chaosgarden connection in tests
        };

        (state, temp_dir)
    }

    #[tokio::test]
    async fn test_download_artifact() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifact/test_artifact")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain"
        );
        assert_eq!(
            response.headers().get("x-artifact-id").unwrap(),
            "test_artifact"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"Hello, artifact world!");
    }

    #[tokio::test]
    async fn test_artifact_meta() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifact/test_artifact/meta")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["id"], "test_artifact");
        assert_eq!(json["creator"], "test_creator");
        assert_eq!(json["mime_type"], "text/plain");
        assert!(json["content_url"]
            .as_str()
            .unwrap()
            .contains("test_artifact"));
    }

    #[tokio::test]
    async fn test_list_artifacts() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifacts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["id"], "test_artifact");
    }

    #[tokio::test]
    async fn test_list_artifacts_with_filter() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        // Filter by tag that exists
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/artifacts?tag=type:text")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 1);

        // Filter by tag that doesn't exist
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifacts?tag=type:audio")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 0);
    }

    #[tokio::test]
    async fn test_artifact_not_found() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifact/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_access_count_increments() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state.clone());

        // First access
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/artifact/test_artifact")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check access count
        let store = state.artifact_store.read().unwrap();
        let artifact = store.get("test_artifact").unwrap().unwrap();
        assert_eq!(artifact.access_count, 1);
        drop(store);

        // Second access
        let _ = app
            .oneshot(
                Request::builder()
                    .uri("/artifact/test_artifact")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let store = state.artifact_store.read().unwrap();
        let artifact = store.get("test_artifact").unwrap().unwrap();
        assert_eq!(artifact.access_count, 2);
    }
}
