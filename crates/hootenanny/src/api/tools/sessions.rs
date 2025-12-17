//! MCP tools for capture sessions.
//!
//! Exposes session lifecycle management, multi-stream coordination, and export.

use crate::api::service::EventDualityServer;
use crate::sessions::{SessionId, SessionMode};
use crate::streams::StreamUri;
use hooteproto::{ToolError, ToolOutput, ToolResult};
use serde::{Deserialize, Serialize};
use tracing::info;

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SessionCreateRequest {
    pub mode: SessionModeParam,
    pub streams: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SessionModeParam {
    Passive,
    RequestResponse {
        midi_out: String,
        audio_in: String,
    },
}

impl From<SessionModeParam> for SessionMode {
    fn from(param: SessionModeParam) -> Self {
        match param {
            SessionModeParam::Passive => SessionMode::Passive,
            SessionModeParam::RequestResponse { midi_out, audio_in } => {
                SessionMode::RequestResponse {
                    midi_out: StreamUri::from(midi_out),
                    audio_in: StreamUri::from(audio_in),
                }
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SessionCreateResponse {
    pub session_id: String,
    pub mode: String,
    pub streams: Vec<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionPlayRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionPlayResponse {
    pub session_id: String,
    pub segment_index: usize,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionPauseRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionPauseResponse {
    pub session_id: String,
    pub segments_count: usize,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionStopRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionStopResponse {
    pub session_id: String,
    pub session_hash: String,
    pub segments_count: usize,
    pub timeline_snapshots: usize,
}

#[derive(Debug, Deserialize)]
pub struct SessionStatusRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionStatusResponse {
    pub session_id: String,
    pub status: String,
    pub mode: String,
    pub streams: Vec<String>,
    pub segments_count: usize,
    pub current_segment_active: bool,
    pub timeline_snapshots: usize,
}

#[derive(Debug, Deserialize)]
pub struct SessionExportRequest {
    pub session_id: String,
    #[serde(default)]
    pub format: ExportFormatParam,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormatParam {
    Json,
    Archive, // Future: tarball of all streams
}

impl Default for ExportFormatParam {
    fn default() -> Self {
        ExportFormatParam::Json
    }
}

#[derive(Debug, Serialize)]
pub struct SessionExportResponse {
    pub session_id: String,
    pub content_hash: String,
    pub format: String,
}

// ============================================================================
// Tool Implementations
// ============================================================================

impl EventDualityServer {
    /// Create a new capture session
    #[tracing::instrument(
        name = "mcp.tool.session_create",
        skip(self, request),
        fields(
            session.mode = ?request.mode,
            session.streams = request.streams.len(),
        )
    )]
    pub async fn session_create(&self, request: SessionCreateRequest) -> ToolResult {
        let session_manager = self
            .session_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("session manager not available"))?;

        let mode: SessionMode = request.mode.into();
        let streams: Vec<StreamUri> = request
            .streams
            .iter()
            .map(|s| StreamUri::from(s.as_str()))
            .collect();

        let session_id = session_manager
            .create_session(mode.clone(), streams.clone())
            .map_err(|e| ToolError::internal(format!("failed to create session: {}", e)))?;

        info!(
            "created session: {} ({} streams)",
            session_id,
            streams.len()
        );

        let response = SessionCreateResponse {
            session_id: session_id.to_string(),
            mode: format!("{:?}", mode),
            streams: streams.iter().map(|s| s.to_string()).collect(),
            status: "recording".to_string(),
        };

        Ok(ToolOutput::new(
            format!("Session {} created with {} streams", session_id, streams.len()),
            &response,
        ))
    }

    /// Start recording (play) - begins a new segment
    #[tracing::instrument(
        name = "mcp.tool.session_play",
        skip(self, request),
        fields(session.id = %request.session_id)
    )]
    pub async fn session_play(&self, request: SessionPlayRequest) -> ToolResult {
        let session_manager = self
            .session_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("session manager not available"))?;

        let session_id = SessionId::new(request.session_id);

        session_manager
            .play(&session_id)
            .map_err(|e| ToolError::internal(format!("failed to play session: {}", e)))?;

        // Get updated session info
        let session = session_manager
            .get_session(&session_id)
            .map_err(|e| ToolError::internal(format!("failed to get session: {}", e)))?
            .ok_or_else(|| {
                ToolError::validation("not_found", format!("session not found: {}", session_id))
            })?;

        let segment_index = session.segments.len().saturating_sub(1);

        info!("started segment {} for session {}", segment_index, session_id);

        let response = SessionPlayResponse {
            session_id: session_id.to_string(),
            segment_index,
            status: format!("{:?}", session.status),
        };

        Ok(ToolOutput::new(
            format!(
                "Session {} playing (segment {})",
                session_id, segment_index
            ),
            &response,
        ))
    }

    /// Pause recording - ends the current segment
    #[tracing::instrument(
        name = "mcp.tool.session_pause",
        skip(self, request),
        fields(session.id = %request.session_id)
    )]
    pub async fn session_pause(&self, request: SessionPauseRequest) -> ToolResult {
        let session_manager = self
            .session_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("session manager not available"))?;

        let session_id = SessionId::new(request.session_id);

        session_manager
            .pause(&session_id)
            .map_err(|e| ToolError::internal(format!("failed to pause session: {}", e)))?;

        // Get updated session info
        let session = session_manager
            .get_session(&session_id)
            .map_err(|e| ToolError::internal(format!("failed to get session: {}", e)))?
            .ok_or_else(|| {
                ToolError::validation("not_found", format!("session not found: {}", session_id))
            })?;

        info!("paused session {} ({} segments)", session_id, session.segments.len());

        let response = SessionPauseResponse {
            session_id: session_id.to_string(),
            segments_count: session.segments.len(),
            status: format!("{:?}", session.status),
        };

        Ok(ToolOutput::new(
            format!(
                "Session {} paused ({} segments)",
                session_id,
                response.segments_count
            ),
            &response,
        ))
    }

    /// Stop the session - finalize and archive
    #[tracing::instrument(
        name = "mcp.tool.session_stop",
        skip(self, request),
        fields(session.id = %request.session_id)
    )]
    pub async fn session_stop(&self, request: SessionStopRequest) -> ToolResult {
        let session_manager = self
            .session_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("session manager not available"))?;

        let session_id = SessionId::new(request.session_id);

        // Get session info before stopping
        let session = session_manager
            .get_session(&session_id)
            .map_err(|e| ToolError::internal(format!("failed to get session: {}", e)))?
            .ok_or_else(|| {
                ToolError::validation("not_found", format!("session not found: {}", session_id))
            })?;

        let segments_count = session.segments.len();
        let timeline_snapshots = session.timeline.clock_snapshots.len();

        let session_hash = session_manager
            .stop(&session_id)
            .map_err(|e| ToolError::internal(format!("failed to stop session: {}", e)))?;

        info!(
            "stopped session {} (hash: {}, {} segments)",
            session_id, session_hash, segments_count
        );

        let response = SessionStopResponse {
            session_id: session_id.to_string(),
            session_hash: session_hash.to_string(),
            segments_count,
            timeline_snapshots,
        };

        Ok(ToolOutput::new(
            format!(
                "Session {} stopped (hash: {}, {} segments)",
                session_id, session_hash, segments_count
            ),
            &response,
        ))
    }

    /// Get session status
    #[tracing::instrument(
        name = "mcp.tool.session_status",
        skip(self, request),
        fields(session.id = %request.session_id)
    )]
    pub async fn session_status(&self, request: SessionStatusRequest) -> ToolResult {
        let session_manager = self
            .session_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("session manager not available"))?;

        let session_id = SessionId::new(request.session_id);

        let session = session_manager
            .get_session(&session_id)
            .map_err(|e| ToolError::internal(format!("failed to get session: {}", e)))?
            .ok_or_else(|| {
                ToolError::validation("not_found", format!("session not found: {}", session_id))
            })?;

        let current_segment_active = session.current_segment().map(|s| s.is_active()).unwrap_or(false);

        let response = SessionStatusResponse {
            session_id: session_id.to_string(),
            status: format!("{:?}", session.status),
            mode: format!("{:?}", session.mode),
            streams: session.streams.iter().map(|s| s.to_string()).collect(),
            segments_count: session.segments.len(),
            current_segment_active,
            timeline_snapshots: session.timeline.clock_snapshots.len(),
        };

        Ok(ToolOutput::new(
            format!(
                "Session {} status: {:?} ({} segments, {} streams)",
                session_id,
                session.status,
                response.segments_count,
                response.streams.len()
            ),
            &response,
        ))
    }

    /// Export session data
    #[tracing::instrument(
        name = "mcp.tool.session_export",
        skip(self, request),
        fields(
            session.id = %request.session_id,
            export.format = ?request.format,
        )
    )]
    pub async fn session_export(&self, request: SessionExportRequest) -> ToolResult {
        let session_manager = self
            .session_manager
            .as_ref()
            .ok_or_else(|| ToolError::internal("session manager not available"))?;

        let session_id = SessionId::new(request.session_id);

        let session = session_manager
            .get_session(&session_id)
            .map_err(|e| ToolError::internal(format!("failed to get session: {}", e)))?
            .ok_or_else(|| {
                ToolError::validation("not_found", format!("session not found: {}", session_id))
            })?;

        match request.format {
            ExportFormatParam::Json => {
                // Serialize session to JSON and store in CAS
                let session_json = serde_json::to_vec_pretty(&session)
                    .map_err(|e| ToolError::internal(format!("failed to serialize session: {}", e)))?;

                // Store in CAS
                let cas_hash = self.local_models
                    .store_cas_content(&session_json, "application/json")
                    .await
                    .map_err(|e| ToolError::internal(format!("failed to store session JSON in CAS: {}", e)))?;

                // Create artifact
                use crate::artifact_store::Artifact;
                use crate::types::{ArtifactId, ContentHash};

                let content_hash = ContentHash::new(&cas_hash);
                let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

                let artifact = Artifact::new(
                    artifact_id.clone(),
                    content_hash.clone(),
                    "hootenanny_session",
                    serde_json::json!({
                        "type": "capture_session",
                        "session_id": session_id.to_string(),
                        "mode": format!("{:?}", session.mode),
                        "streams": session.streams.iter().map(|u| u.to_string()).collect::<Vec<_>>(),
                    }),
                ).with_tag("type:session");

                // Store artifact
                use crate::artifact_store::ArtifactStore;
                {
                    let mut store = self.artifact_store
                        .write()
                        .map_err(|e| ToolError::internal(format!("failed to lock artifact store: {}", e)))?;
                    ArtifactStore::put(&*store, artifact)
                        .map_err(|e| ToolError::internal(format!("failed to store artifact: {}", e)))?;
                    ArtifactStore::flush(&*store)
                        .map_err(|e| ToolError::internal(format!("failed to flush artifact store: {}", e)))?;
                }

                info!(
                    "exported session {} as JSON (hash: {}, artifact: {})",
                    session_id, content_hash, artifact_id
                );

                let response = SessionExportResponse {
                    session_id: session_id.to_string(),
                    content_hash: content_hash.to_string(),
                    format: "json".to_string(),
                };

                Ok(ToolOutput::new(
                    format!("Exported session {} to artifact {}", session_id, artifact_id),
                    &response,
                ))
            }
            ExportFormatParam::Archive => {
                // Future: create tarball of all stream chunks and session metadata
                Err(ToolError::validation(
                    "not_supported",
                    "Archive export format not yet implemented. Use 'json' format for now.",
                ))
            }
        }
    }
}
