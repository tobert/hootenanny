//! Schedule content on the chaosgarden timeline for playback.
//!
//! The `schedule()` tool places encodings on the timeline at specified positions,
//! handling automatic duration detection and format resolution.

use crate::api::native::types::Encoding;
use crate::api::service::EventDualityServer;
use crate::artifact_store::ArtifactStore;
use hooteproto::{ToolError, ToolOutput, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleRequest {
    #[schemars(description = "Content to schedule for playback")]
    pub encoding: Encoding,

    #[schemars(description = "Position on timeline (in beats)")]
    pub at: f64,

    #[schemars(description = "Duration in beats (auto-detected if omitted)")]
    pub duration: Option<f64>,

    #[schemars(description = "Playback gain (0.0-1.0)")]
    pub gain: Option<f64>,

    #[schemars(description = "Playback rate (1.0 = normal speed)")]
    pub rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleResponse {
    pub success: bool,
    pub message: String,
    pub region_id: String,
    pub position: f64,
    pub duration: f64,
    pub artifact_id: String,
}

impl EventDualityServer {
    #[tracing::instrument(name = "mcp.tool.schedule", skip(self, request))]
    pub async fn schedule(&self, request: ScheduleRequest) -> ToolResult {
        // Require garden connection
        if self.garden_manager.is_none() {
            return Err(ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden. Start hootenanny with --chaosgarden=local or --chaosgarden=tcp://host:port",
            ));
        }

        let manager = self.garden_manager.as_ref().unwrap();

        // Validate gain range
        if let Some(gain) = request.gain {
            if !(0.0..=1.0).contains(&gain) {
                return Err(ToolError::validation(
                    "invalid_params",
                    format!("Gain must be between 0.0 and 1.0, got {}", gain),
                ));
            }
        }

        // Validate rate
        if let Some(rate) = request.rate {
            if rate <= 0.0 {
                return Err(ToolError::validation(
                    "invalid_params",
                    format!("Rate must be positive, got {}", rate),
                ));
            }
        }

        // Resolve encoding to artifact_id and content information
        let (artifact_id, _content_hash, _output_type) = match &request.encoding {
            Encoding::Midi { artifact_id } | Encoding::Audio { artifact_id } => {
                // Look up artifact to get content_hash
                let store = self
                    .artifact_store
                    .read()
                    .map_err(|_| ToolError::internal("Lock poisoned on artifact_store"))?;

                let artifact = store
                    .get(artifact_id)
                    .map_err(|e| ToolError::internal(format!("Failed to query artifact store: {}", e)))?
                    .ok_or_else(|| {
                        ToolError::validation(
                            "not_found",
                            format!("Artifact not found: {}", artifact_id),
                        )
                    })?;

                (
                    artifact_id.clone(),
                    artifact.content_hash.as_str().to_string(),
                    request.encoding.output_type(),
                )
            }
            Encoding::Hash {
                content_hash: _,
                format: _,
            } => {
                // Create a synthetic artifact_id from the hash for region creation
                // Chaosgarden expects artifact_id, not raw hash
                return Err(ToolError::validation(
                    "unsupported_encoding",
                    "Hash encoding not yet supported for schedule(). Use Midi or Audio encoding with artifact_id.",
                ));
            }
            Encoding::Abc { notation: _ } => {
                // ABC notation needs to be converted to MIDI first
                return Err(ToolError::validation(
                    "unsupported_encoding",
                    "ABC encoding not yet supported for schedule(). Convert to MIDI first using project() or abc_to_midi.",
                ));
            }
        };

        // Determine duration
        // Auto-detect duration from artifact metadata
        // For now, use a default of 4 beats (one measure at 4/4)
        // TODO: Extract duration from MIDI/audio metadata
        let duration = request.duration.unwrap_or(4.0);

        // Create region on timeline
        use chaosgarden::ipc::{Beat, Behavior, ShellRequest};

        let behavior = Behavior::PlayContent { artifact_id: artifact_id.clone() };

        let shell_req = ShellRequest::CreateRegion {
            position: Beat(request.at),
            duration: Beat(duration),
            behavior,
        };

        match manager.request(shell_req).await {
            Ok(chaosgarden::ipc::ShellReply::RegionCreated { region_id }) => {
                let response = ScheduleResponse {
                    success: true,
                    message: format!("Scheduled {} at beat {}", artifact_id, request.at),
                    region_id: region_id.to_string(),
                    position: request.at,
                    duration,
                    artifact_id,
                };
                let json = serde_json::to_string_pretty(&response)
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(ToolOutput::text_only(json))
            }
            Ok(chaosgarden::ipc::ShellReply::Error { error, .. }) => {
                Err(ToolError::internal(error))
            }
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected reply: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::internal(format!("Schedule failed: {}", e))),
        }
    }
}
