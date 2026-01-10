//! Schedule content on the chaosgarden timeline for playback.
//!
//! The `schedule()` tool places encodings on the timeline at specified positions,
//! handling automatic duration detection and format resolution.

use crate::api::service::EventDualityServer;
use crate::artifact_store::ArtifactStore;
use hooteproto::request::ScheduleRequest;
use hooteproto::responses::ScheduledResponse;
use hooteproto::{Encoding, ToolError};

impl EventDualityServer {
    #[tracing::instrument(name = "mcp.tool.schedule", skip(self, request))]
    pub async fn schedule_typed(&self, request: ScheduleRequest) -> Result<ScheduledResponse, ToolError> {
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

        // Resolve encoding to artifact_id, content_hash, and duration from metadata
        let (artifact_id, content_hash, metadata_duration) = match &request.encoding {
            Encoding::Midi { artifact_id } | Encoding::Audio { artifact_id } => {
                // Verify artifact exists and extract metadata
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

                // Try to extract duration from metadata
                // Check common locations: duration_seconds, output.duration_seconds
                let duration = artifact.metadata.get("duration_seconds")
                    .and_then(|v| v.as_f64())
                    .or_else(|| {
                        artifact.metadata.get("output")
                            .and_then(|o| o.get("duration_seconds"))
                            .and_then(|v| v.as_f64())
                    });

                // Need content_hash for chaosgarden to resolve from CAS
                let hash = artifact.content_hash.as_str().to_string();

                (artifact_id.clone(), hash, duration)
            }
            Encoding::Hash { .. } => {
                return Err(ToolError::validation(
                    "unsupported_encoding",
                    "Hash encoding not yet supported for schedule(). Use Midi or Audio encoding with artifact_id.",
                ));
            }
            Encoding::Abc { .. } => {
                return Err(ToolError::validation(
                    "unsupported_encoding",
                    "ABC encoding not yet supported for schedule(). Convert to MIDI first using abc_to_midi.",
                ));
            }
        };

        // Use explicit duration if provided, otherwise require it from metadata
        // Duration is critical for scheduling - we don't guess
        let duration = match (request.duration, metadata_duration) {
            (Some(d), _) => d,
            (None, Some(d)) => d,
            (None, None) => {
                return Err(ToolError::validation(
                    "missing_duration",
                    format!(
                        "Artifact {} has no duration_seconds metadata and no duration was provided. \
                         Either regenerate the artifact with updated tools, or explicitly pass duration.",
                        artifact_id
                    ),
                ));
            }
        };

        // Create region on timeline via Cap'n Proto
        use hooteproto::request::{GardenCreateRegionRequest, ToolRequest};
        use hooteproto::responses::ToolResponse;

        let create_req = ToolRequest::GardenCreateRegion(GardenCreateRegionRequest {
            position: request.at,
            duration,
            behavior_type: "play_content".to_string(),
            content_id: content_hash.clone(),
        });

        match manager.tool_request(create_req).await {
            Ok(ToolResponse::GardenRegionCreated(response)) => {
                Ok(ScheduledResponse {
                    success: true,
                    message: format!("Scheduled {} at beat {}", artifact_id, request.at),
                    region_id: response.region_id,
                    position: request.at,
                    duration,
                    artifact_id,
                })
            }
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::internal(format!("Schedule failed: {}", e))),
        }
    }
}
