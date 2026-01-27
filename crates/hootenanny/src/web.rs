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
    routing::{get, post},
    Json, Router,
};
use hooteproto::request::{GardenSetMonitorRequest, ToolRequest};
use hooteproto::responses::ToolResponse;
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
        .route("/api/monitor", post(set_monitor))
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
        use hooteproto::request::{ToolRequest, GardenGetAudioSnapshotRequest};
        use hooteproto::responses::ToolResponse;

        let (sample_rate, channels, format, samples) = if let Some(ref manager) = garden_manager {
            let request = ToolRequest::GardenGetAudioSnapshot(GardenGetAudioSnapshotRequest {
                frames: frames_per_request,
            });
            match manager.tool_request(request).await {
                Ok(ToolResponse::GardenAudioSnapshot(response)) => {
                    (response.sample_rate, response.channels, response.format, response.samples)
                }
                Ok(other) => {
                    // Unexpected reply - send silence
                    tracing::warn!("Unexpected snapshot reply: {:?}", other);
                    (default_sample_rate, default_channels, default_format, vec![0.0f32; frames_per_request as usize * 2])
                }
                Err(e) => {
                    tracing::warn!("Audio snapshot error: {}", e);
                    (default_sample_rate, default_channels, default_format, vec![0.0f32; frames_per_request as usize * 2])
                }
            }
        } else {
            // No garden manager - send silence
            tracing::warn!("No garden manager - sending silence");
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

#[derive(Debug, Deserialize)]
struct MonitorRequest {
    enabled: Option<bool>,
    gain: Option<f32>,
}

/// Set monitor gain/enabled state
async fn set_monitor(
    State(state): State<WebState>,
    Json(body): Json<MonitorRequest>,
) -> impl IntoResponse {
    if let Some(ref manager) = state.garden_manager {
        let request = ToolRequest::GardenSetMonitor(GardenSetMonitorRequest {
            enabled: body.enabled,
            gain: body.gain,
        });
        match manager.tool_request(request).await {
            Ok(ToolResponse::GardenMonitorStatus(status)) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "ok": true,
                    "enabled": status.enabled,
                    "gain": status.gain
                })),
            ),
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({"ok": true})),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            ),
        }
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no garden backend"})),
        )
    }
}

/// HTML template for the Winamp-inspired player UI
const UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Hootenanny</title>
  <style>
    :root {
      --bg-dark: #0d0d1a;
      --bg-mid: #1a1a2e;
      --bg-panel: #12121f;
      --border-light: #3a3a5a;
      --border-dark: #0a0a12;
      --text: #e8e8f0;
      --text-muted: #888899;
      --accent-cyan: #0ff;
      --accent-pink: #f0a;
      --accent-green: #0f0;
      --spectrum-low: #22ff22;
      --spectrum-mid: #ffff22;
      --spectrum-high: #ff4422;
    }

    @font-face {
      font-family: 'Digital';
      src: local('Courier New'), local('monospace');
    }

    * { box-sizing: border-box; margin: 0; padding: 0; }

    html, body {
      height: 100%;
      overflow: hidden;
    }

    body {
      font-family: 'Segoe UI', system-ui, -apple-system, sans-serif;
      background: linear-gradient(180deg, var(--bg-mid) 0%, var(--bg-dark) 100%);
      color: var(--text);
      display: flex;
      flex-direction: column;
    }

    .app {
      display: flex;
      flex-direction: column;
      height: 100vh;
      max-width: 1400px;
      margin: 0 auto;
      padding: 0.5rem;
    }

    /* Title bar */
    .titlebar {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 0.5rem 1rem;
      background: linear-gradient(90deg, var(--bg-panel) 0%, var(--bg-mid) 50%, var(--bg-panel) 100%);
      border: 1px solid var(--border-light);
      border-bottom: 1px solid var(--border-dark);
      border-radius: 4px 4px 0 0;
    }

    .titlebar h1 {
      font-size: 1.1rem;
      font-weight: 600;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      background: linear-gradient(90deg, var(--accent-cyan), var(--accent-pink));
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }

    .tagline {
      font-size: 0.75rem;
      color: var(--text-muted);
      font-style: italic;
    }

    /* Player section */
    .player {
      background: var(--bg-panel);
      border: 1px solid var(--border-light);
      border-top: none;
      padding: 1rem;
      display: flex;
      flex-direction: column;
      gap: 0.75rem;
    }

    /* Spectrum analyzer */
    .spectrum-container {
      background: var(--bg-dark);
      border: 2px inset var(--border-dark);
      border-radius: 2px;
      padding: 4px;
    }

    #spectrum {
      width: 100%;
      height: 100px;
      display: block;
      background: #0a0a12;
      border-radius: 2px;
    }

    /* Controls row */
    .controls {
      display: flex;
      align-items: center;
      gap: 1.5rem;
      flex-wrap: wrap;
    }

    /* Time display */
    .time-block {
      background: #000;
      border: 2px inset var(--border-dark);
      padding: 0.5rem 0.75rem;
      border-radius: 2px;
      min-width: 80px;
      text-align: center;
    }

    .time {
      font-family: 'Courier New', monospace;
      font-size: 1.5rem;
      font-weight: bold;
      color: var(--accent-green);
      text-shadow: 0 0 8px var(--accent-green);
      letter-spacing: 0.1em;
    }

    .samplerate {
      font-size: 0.65rem;
      color: var(--text-muted);
      margin-top: 2px;
    }

    /* Transport buttons */
    .transport {
      display: flex;
      gap: 0.25rem;
    }

    .btn {
      width: 36px;
      height: 28px;
      background: linear-gradient(180deg, #3a3a4e 0%, #2a2a3e 50%, #1a1a2e 100%);
      border: 1px outset var(--border-light);
      border-radius: 3px;
      color: var(--text);
      cursor: pointer;
      font-size: 0.9rem;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: all 0.1s;
    }

    .btn:hover {
      background: linear-gradient(180deg, #4a4a5e 0%, #3a3a4e 50%, #2a2a3e 100%);
    }

    .btn:active {
      border-style: inset;
      background: linear-gradient(180deg, #1a1a2e 0%, #2a2a3e 50%, #3a3a4e 100%);
    }

    .btn.play {
      width: 44px;
      color: var(--accent-green);
    }

    .btn.play.active {
      color: var(--accent-pink);
      text-shadow: 0 0 6px var(--accent-pink);
    }

    /* Volume control */
    .volume-wrap {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      flex: 1;
      max-width: 200px;
    }

    .volume-icon {
      font-size: 1rem;
      color: var(--text-muted);
    }

    input[type="range"] {
      -webkit-appearance: none;
      flex: 1;
      height: 8px;
      background: linear-gradient(90deg, var(--spectrum-low), var(--spectrum-mid), var(--spectrum-high));
      border-radius: 4px;
      cursor: pointer;
    }

    input[type="range"]::-webkit-slider-thumb {
      -webkit-appearance: none;
      width: 14px;
      height: 18px;
      background: linear-gradient(180deg, #eee 0%, #888 100%);
      border: 1px solid #444;
      border-radius: 2px;
      cursor: pointer;
    }

    input[type="range"]::-moz-range-thumb {
      width: 14px;
      height: 18px;
      background: linear-gradient(180deg, #eee 0%, #888 100%);
      border: 1px solid #444;
      border-radius: 2px;
      cursor: pointer;
    }

    /* Status indicator */
    .status-wrap {
      display: flex;
      align-items: center;
      gap: 0.5rem;
      margin-left: auto;
    }

    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: #444;
      transition: all 0.3s;
    }

    .status-dot.connected {
      background: var(--accent-green);
      box-shadow: 0 0 6px var(--accent-green);
    }

    .status-text {
      font-size: 0.75rem;
      color: var(--text-muted);
    }

    /* Track info */
    .track-info {
      font-size: 0.8rem;
      color: var(--text-muted);
      padding: 0.5rem;
      background: var(--bg-dark);
      border: 1px inset var(--border-dark);
      border-radius: 2px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    /* Artifacts section */
    .artifacts {
      flex: 1;
      display: flex;
      flex-direction: column;
      background: var(--bg-panel);
      border: 1px solid var(--border-light);
      border-top: none;
      border-radius: 0 0 4px 4px;
      overflow: hidden;
    }

    .artifacts-header {
      display: flex;
      align-items: center;
      gap: 1rem;
      padding: 0.75rem 1rem;
      background: linear-gradient(180deg, var(--bg-mid) 0%, var(--bg-panel) 100%);
      border-bottom: 1px solid var(--border-dark);
    }

    .artifacts-header h2 {
      font-size: 0.85rem;
      font-weight: 600;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      color: var(--accent-cyan);
    }

    .artifacts-header input[type="search"] {
      flex: 1;
      max-width: 200px;
      padding: 0.35rem 0.5rem;
      background: var(--bg-dark);
      border: 1px inset var(--border-dark);
      border-radius: 3px;
      color: var(--text);
      font-size: 0.8rem;
    }

    .artifacts-header input::placeholder {
      color: var(--text-muted);
    }

    .artifacts-header select {
      padding: 0.35rem 0.5rem;
      background: var(--bg-dark);
      border: 1px inset var(--border-dark);
      border-radius: 3px;
      color: var(--text);
      font-size: 0.8rem;
    }

    /* Artifact grid */
    .artifact-grid {
      flex: 1;
      overflow-y: auto;
      padding: 0.75rem;
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
      gap: 0.75rem;
      align-content: start;
    }

    .artifact-card {
      background: var(--bg-dark);
      border: 1px solid var(--border-dark);
      border-radius: 4px;
      padding: 0.75rem;
      display: flex;
      flex-direction: column;
      gap: 0.35rem;
      transition: all 0.15s;
    }

    .artifact-card:hover {
      border-color: var(--accent-cyan);
      box-shadow: 0 0 8px rgba(0, 255, 255, 0.2);
    }

    .artifact-icon {
      font-size: 1.5rem;
      line-height: 1;
    }

    .artifact-name {
      font-size: 0.8rem;
      font-weight: 500;
      color: var(--text);
      word-break: break-all;
      overflow: hidden;
      text-overflow: ellipsis;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      -webkit-box-orient: vertical;
    }

    .artifact-creator {
      font-size: 0.7rem;
      color: var(--text-muted);
    }

    .artifact-actions {
      display: flex;
      gap: 0.5rem;
      margin-top: auto;
      padding-top: 0.35rem;
    }

    .artifact-actions button,
    .artifact-actions a {
      background: transparent;
      border: 1px solid var(--border-light);
      border-radius: 3px;
      color: var(--text-muted);
      padding: 0.25rem 0.5rem;
      font-size: 0.7rem;
      cursor: pointer;
      text-decoration: none;
      transition: all 0.1s;
    }

    .artifact-actions button:hover,
    .artifact-actions a:hover {
      border-color: var(--accent-cyan);
      color: var(--accent-cyan);
    }

    .empty {
      grid-column: 1 / -1;
      text-align: center;
      padding: 2rem;
      color: var(--text-muted);
    }

    /* Scrollbar */
    ::-webkit-scrollbar {
      width: 10px;
    }

    ::-webkit-scrollbar-track {
      background: var(--bg-dark);
    }

    ::-webkit-scrollbar-thumb {
      background: var(--border-light);
      border-radius: 5px;
    }

    ::-webkit-scrollbar-thumb:hover {
      background: var(--accent-cyan);
    }
  </style>
</head>
<body>
  <div class="app">
    <header class="titlebar">
      <h1>🎵 HOOTENANNY</h1>
      <span class="tagline">♪ every voice welcome</span>
    </header>

    <main class="player">
      <div class="spectrum-container">
        <canvas id="spectrum"></canvas>
      </div>

      <div class="controls">
        <div class="time-block">
          <div class="time" id="time">0:00</div>
          <div class="samplerate">48kHz</div>
        </div>

        <div class="transport">
          <button class="btn" id="btnPrev" title="Previous">◄◄</button>
          <button class="btn play" id="btnPlay" title="Play/Pause">▶</button>
          <button class="btn" id="btnStop" title="Stop">■</button>
          <button class="btn" id="btnNext" title="Next">►►</button>
        </div>

        <div class="volume-wrap">
          <span class="volume-icon">🔊</span>
          <input type="range" id="volume" min="0" max="100" value="80" title="Volume">
        </div>

        <div class="status-wrap">
          <div class="status-dot" id="statusDot"></div>
          <span class="status-text" id="statusText">Ready</span>
        </div>
      </div>

      <div class="track-info" id="trackInfo">
        ♫ Click ▶ to start live audio stream
      </div>
    </main>

    <section class="artifacts">
      <div class="artifacts-header">
        <h2>ARTIFACTS</h2>
        <input type="search" id="search" placeholder="🔍 search...">
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
      <div class="artifact-grid" id="artifactList">
        <div class="empty">Loading artifacts...</div>
      </div>
    </section>
  </div>

  <script>
    // State
    let allArtifacts = [];
    let ws = null;
    let audioCtx = null;
    let workletNode = null;
    let analyser = null;
    let isPlaying = false;
    let startTime = 0;

    // Spectrum analyzer state
    const peakHold = new Float32Array(64);
    const peakDecay = 0.97;

    // DOM elements
    const canvas = document.getElementById('spectrum');
    const ctx = canvas.getContext('2d');
    const timeDisplay = document.getElementById('time');
    const statusDot = document.getElementById('statusDot');
    const statusText = document.getElementById('statusText');
    const trackInfo = document.getElementById('trackInfo');
    const btnPlay = document.getElementById('btnPlay');
    const volumeSlider = document.getElementById('volume');

    // Resize canvas to match container
    function resizeCanvas() {
      const container = canvas.parentElement;
      canvas.width = container.clientWidth - 8;
      canvas.height = 100;
    }
    resizeCanvas();
    window.addEventListener('resize', resizeCanvas);

    // Format time as m:ss
    function formatTime(seconds) {
      const m = Math.floor(seconds / 60);
      const s = Math.floor(seconds % 60);
      return `${m}:${s.toString().padStart(2, '0')}`;
    }

    // Update time display
    function updateTime() {
      if (audioCtx && isPlaying) {
        const elapsed = audioCtx.currentTime - startTime;
        timeDisplay.textContent = formatTime(elapsed);
      }
      requestAnimationFrame(updateTime);
    }

    // Draw spectrum analyzer
    function drawSpectrum() {
      requestAnimationFrame(drawSpectrum);

      if (!analyser) {
        // Draw idle state
        ctx.fillStyle = '#0a0a12';
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        return;
      }

      const data = new Uint8Array(analyser.frequencyBinCount);
      analyser.getByteFrequencyData(data);

      const { width, height } = canvas;
      ctx.fillStyle = '#0a0a12';
      ctx.fillRect(0, 0, width, height);

      const barCount = 64;
      const gap = 2;
      const barWidth = (width - (barCount - 1) * gap) / barCount;
      const binSize = Math.floor(data.length / barCount);

      for (let i = 0; i < barCount; i++) {
        // Average frequency bins with slight weighting toward higher bins
        let sum = 0;
        for (let j = 0; j < binSize; j++) {
          sum += data[i * binSize + j];
        }
        const value = sum / binSize / 255;
        const barHeight = value * height * 0.95;

        // Update peak hold
        if (value > peakHold[i]) peakHold[i] = value;
        else peakHold[i] *= peakDecay;

        const x = i * (barWidth + gap);

        // Draw bar with gradient
        const gradient = ctx.createLinearGradient(0, height, 0, height - barHeight);
        gradient.addColorStop(0, '#22ff22');
        gradient.addColorStop(0.5, '#ffff22');
        gradient.addColorStop(1, '#ff4422');
        ctx.fillStyle = gradient;
        ctx.fillRect(x, height - barHeight, barWidth, barHeight);

        // Draw peak hold line
        const peakY = height - peakHold[i] * height * 0.95;
        ctx.fillStyle = '#ffffff';
        ctx.fillRect(x, peakY - 2, barWidth, 2);
      }
    }

    // Initialize audio context and worklet
    async function initAudio() {
      if (audioCtx) return;

      audioCtx = new AudioContext({ sampleRate: 48000 });

      // Create analyser
      analyser = audioCtx.createAnalyser();
      analyser.fftSize = 512;
      analyser.smoothingTimeConstant = 0.75;

      // Create worklet for PCM playback
      const workletCode = `
        class PCMProcessor extends AudioWorkletProcessor {
          constructor() {
            super();
            this.buffer = [];
            this.port.onmessage = e => {
              // Handle clear command
              if (e.data === 'clear') {
                this.buffer = [];
                return;
              }
              this.buffer.push(...e.data);
              // Keep max 1 second of audio buffered
              if (this.buffer.length > 96000) {
                this.buffer.splice(0, this.buffer.length - 48000);
              }
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
            if (this.buffer.length >= needed) {
              this.buffer.splice(0, needed);
            }
            return true;
          }
        }
        registerProcessor('pcm-processor', PCMProcessor);
      `;

      const blob = new Blob([workletCode], { type: 'application/javascript' });
      await audioCtx.audioWorklet.addModule(URL.createObjectURL(blob));
      workletNode = new AudioWorkletNode(audioCtx, 'pcm-processor');

      // Route through analyser
      workletNode.connect(analyser);
      analyser.connect(audioCtx.destination);
    }

    // Start live streaming
    async function startLive() {
      await initAudio();

      const wsUrl = `wss://${location.host}/stream/live`;
      ws = new WebSocket(wsUrl);
      ws.binaryType = 'arraybuffer';

      statusText.textContent = 'Connecting...';

      ws.onopen = () => {
        ws.send(JSON.stringify({ type: 'start', buffer_ms: 150 }));
        statusDot.classList.add('connected');
        statusText.textContent = 'Connected';
        trackInfo.textContent = '♫ Live audio stream ─ 48kHz stereo';
        isPlaying = true;
        startTime = audioCtx.currentTime;
        btnPlay.classList.add('active');
        btnPlay.textContent = '⏸';
      };

      ws.onmessage = (e) => {
        if (e.data instanceof ArrayBuffer && e.data.byteLength > 8) {
          const samples = new Float32Array(e.data, 8);
          workletNode.port.postMessage(Array.from(samples));
        }
      };

      ws.onerror = () => {
        statusDot.classList.remove('connected');
        statusText.textContent = 'Error';
      };

      ws.onclose = () => {
        statusDot.classList.remove('connected');
        statusText.textContent = 'Disconnected';
        isPlaying = false;
        btnPlay.classList.remove('active');
        btnPlay.textContent = '▶';
      };
    }

    // Stop streaming
    function stopLive() {
      if (ws) {
        ws.send(JSON.stringify({ type: 'stop' }));
        ws.close();
        ws = null;
      }
      // Clear the audio buffer to stop any looping samples
      if (workletNode) {
        workletNode.port.postMessage('clear');
      }
      statusDot.classList.remove('connected');
      statusText.textContent = 'Stopped';
      trackInfo.textContent = '♫ Click ▶ to start live audio stream';
      isPlaying = false;
      btnPlay.classList.remove('active');
      btnPlay.textContent = '▶';
    }

    // Volume control
    async function setVolume(value) {
      try {
        await fetch('/api/monitor', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ gain: value / 100 })
        });
      } catch (e) {
        console.warn('Failed to set volume:', e);
      }
    }

    // Transport controls
    btnPlay.onclick = () => {
      if (isPlaying) {
        stopLive();
      } else {
        startLive();
      }
    };

    document.getElementById('btnStop').onclick = () => {
      stopLive();
      startTime = 0;
      timeDisplay.textContent = '0:00';
    };

    volumeSlider.oninput = function() {
      setVolume(this.value);
    };

    // Artifact functions
    async function loadArtifacts() {
      try {
        const res = await fetch('/artifacts?limit=200');
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

    function getTypeIcon(tags) {
      if (tags.some(t => t.includes('midi'))) return '🎹';
      if (tags.some(t => t.includes('audio') || t.includes('wav'))) return '🔊';
      if (tags.some(t => t.includes('soundfont'))) return '🎸';
      return '📄';
    }

    function getType(tags) {
      const typeTag = tags.find(t => t.startsWith('type:'));
      return typeTag ? typeTag.split(':')[1] : 'unknown';
    }

    function isAudio(tags) {
      return tags.some(t => t === 'type:audio' || t === 'type:wav' || t.includes('audio'));
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
        <div class="artifact-card">
          <div class="artifact-icon">${getTypeIcon(a.tags)}</div>
          <div class="artifact-name" title="${a.id}">${a.id}</div>
          <div class="artifact-creator">${a.creator}</div>
          <div class="artifact-actions">
            ${isAudio(a.tags) ? `<button onclick="playArtifact('${a.content_url}')" title="Play">▶</button>` : ''}
            <a href="${a.content_url}" download title="Download">⬇</a>
          </div>
        </div>
      `).join('');
    }

    // Play artifact audio
    async function playArtifact(url) {
      await initAudio();
      const audio = new Audio(url);
      const source = audioCtx.createMediaElementSource(audio);
      source.connect(analyser);
      analyser.connect(audioCtx.destination);
      audio.play();
      isPlaying = true;
      startTime = audioCtx.currentTime;
      trackInfo.textContent = '♫ Playing artifact...';
      audio.onended = () => {
        isPlaying = false;
        trackInfo.textContent = '♫ Click ▶ to start live audio stream';
      };
    }

    // Status polling
    async function pollStatus() {
      try {
        const res = await fetch('/stream/live/status');
        const status = await res.json();
        if (status.status === 'available' && !ws && !isPlaying) {
          statusText.textContent = 'Ready';
        }
      } catch (e) {
        // Ignore
      }
    }

    // Event listeners
    document.getElementById('search').oninput = renderArtifacts;
    document.getElementById('typeFilter').onchange = renderArtifacts;
    document.getElementById('creatorFilter').onchange = renderArtifacts;

    // Init
    loadArtifacts();
    drawSpectrum();
    updateTime();
    setInterval(pollStatus, 5000);

    // Set initial volume from slider
    setVolume(volumeSlider.value);
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
