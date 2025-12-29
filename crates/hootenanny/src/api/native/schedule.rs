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

        // Resolve encoding to artifact_id
        let artifact_id = match &request.encoding {
            Encoding::Midi { artifact_id } | Encoding::Audio { artifact_id } => {
                // Verify artifact exists
                let store = self
                    .artifact_store
                    .read()
                    .map_err(|_| ToolError::internal("Lock poisoned on artifact_store"))?;

                let _artifact = store
                    .get(artifact_id)
                    .map_err(|e| ToolError::internal(format!("Failed to query artifact store: {}", e)))?
                    .ok_or_else(|| {
                        ToolError::validation(
                            "not_found",
                            format!("Artifact not found: {}", artifact_id),
                        )
                    })?;

                artifact_id.clone()
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

        // Determine duration (default 4 beats = one measure at 4/4)
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
                Ok(ScheduledResponse {
                    success: true,
                    message: format!("Scheduled {} at beat {}", artifact_id, request.at),
                    region_id: region_id.to_string(),
                    position: request.at,
                    duration,
                    artifact_id,
                })
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
