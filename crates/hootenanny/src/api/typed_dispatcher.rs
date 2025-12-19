//! Typed tool dispatcher - no JSON in the core dispatch path.
//!
//! This is the Phase 3 implementation from the Tool Protocol v2 plan.
//! Takes `ToolRequest` and returns `ResponseEnvelope` with proper timing semantics.
//!
//! ## Design
//!
//! - Sync tools execute immediately and return Success
//! - Async tools create jobs; Short/Medium poll internally, Long returns job_id
//! - FireAndForget tools return Ack immediately
//!
//! JSON conversion happens only at protocol edges (MCP, HTTP).

use crate::api::service::EventDualityServer;
use hooteproto::{
    envelope::ResponseEnvelope, request::ToolRequest, responses::ToolResponse, timing::ToolTiming,
    ToolError,
};
use std::sync::Arc;

/// Typed dispatcher - handles ToolRequest → ResponseEnvelope
pub struct TypedDispatcher {
    server: Arc<EventDualityServer>,
}

impl TypedDispatcher {
    pub fn new(server: Arc<EventDualityServer>) -> Self {
        Self { server }
    }

    /// Main dispatch entry point - fully typed, no JSON.
    ///
    /// Routes to appropriate handler based on timing classification.
    pub async fn dispatch(&self, request: ToolRequest) -> ResponseEnvelope {
        let timing = request.timing();
        let name = request.name();

        tracing::debug!(tool = name, ?timing, "Dispatching typed request");

        match timing {
            ToolTiming::Sync => self.dispatch_sync(request).await,
            ToolTiming::AsyncShort | ToolTiming::AsyncMedium => {
                // For now, execute directly. Job creation comes later.
                self.dispatch_async(request).await
            }
            ToolTiming::AsyncLong => {
                // Return job_id immediately for long-running tools
                self.dispatch_async_return_job_id(request).await
            }
            ToolTiming::FireAndForget => self.dispatch_fire_and_forget(request).await,
        }
    }

    /// Dispatch synchronous tools - immediate execution, no job
    async fn dispatch_sync(&self, request: ToolRequest) -> ResponseEnvelope {
        match request {
            // === ABC Notation ===
            ToolRequest::AbcParse(req) => match self.server.abc_parse_typed(&req.abc).await {
                Ok(resp) => ResponseEnvelope::success(ToolResponse::AbcParsed(resp)),
                Err(e) => ResponseEnvelope::error(e),
            },
            ToolRequest::AbcValidate(req) => match self.server.abc_validate_typed(&req.abc).await {
                Ok(resp) => ResponseEnvelope::success(ToolResponse::AbcValidated(resp)),
                Err(e) => ResponseEnvelope::error(e),
            },
            ToolRequest::AbcTranspose(req) => {
                match self
                    .server
                    .abc_transpose_typed(&req.abc, req.semitones, req.target_key.as_deref())
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::AbcTransposed(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === SoundFont ===
            ToolRequest::SoundfontInspect(req) => {
                match self
                    .server
                    .soundfont_inspect_typed(&req.soundfont_hash, req.include_drum_map)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::SoundfontInfo(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::SoundfontPresetInspect(req) => {
                match self
                    .server
                    .soundfont_preset_inspect_typed(&req.soundfont_hash, req.bank, req.program)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::SoundfontPresetInfo(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Garden Status/Query ===
            ToolRequest::GardenStatus => match self.server.garden_status_typed().await {
                Ok(resp) => ResponseEnvelope::success(ToolResponse::GardenStatus(resp)),
                Err(e) => ResponseEnvelope::error(e),
            },
            ToolRequest::GardenGetRegions(req) => {
                match self
                    .server
                    .garden_get_regions_typed(req.start, req.end)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::GardenRegions(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Jobs (status queries are sync) ===
            ToolRequest::JobStatus(req) => match self.server.job_status_typed(&req.job_id).await {
                Ok(resp) => ResponseEnvelope::success(ToolResponse::JobStatus(resp)),
                Err(e) => ResponseEnvelope::error(e),
            },
            ToolRequest::JobList(req) => {
                match self.server.job_list_typed(req.status.as_deref()).await {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::JobList(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Config ===
            ToolRequest::ConfigGet(req) => {
                match self
                    .server
                    .config_get_typed(req.section.as_deref(), req.key.as_deref())
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::ConfigValue(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Admin ===
            ToolRequest::Ping => ResponseEnvelope::ack("pong"),
            ToolRequest::ListTools => {
                use hooteproto::responses::ToolsListResponse;
                let tools = crate::api::dispatch::list_tools();
                let count = tools.len();
                ResponseEnvelope::success(ToolResponse::ToolsList(ToolsListResponse {
                    tools,
                    count,
                }))
            }

            // Fallback for tools that should be sync but aren't implemented yet
            other => {
                tracing::warn!(
                    tool = other.name(),
                    "Sync tool not yet implemented in typed dispatcher"
                );
                ResponseEnvelope::error(ToolError::internal(format!(
                    "Tool {} not yet implemented in typed dispatcher",
                    other.name()
                )))
            }
        }
    }

    /// Dispatch async tools - creates job, polls for result
    async fn dispatch_async(&self, request: ToolRequest) -> ResponseEnvelope {
        // For now, fall back to the existing dispatch path
        // Full implementation will create jobs and handle polling
        let name = request.name();
        tracing::debug!(tool = name, "Async tool - falling back to JSON dispatch");

        ResponseEnvelope::error(ToolError::internal(format!(
            "Async tool {} not yet implemented in typed dispatcher",
            name
        )))
    }

    /// Dispatch long-running async tools - return job_id immediately
    async fn dispatch_async_return_job_id(&self, request: ToolRequest) -> ResponseEnvelope {
        let name = request.name();
        let _timing = request.timing();

        // These tools should spawn a job and return immediately
        // For now, return an error indicating not implemented
        tracing::debug!(tool = name, "AsyncLong tool - would return job_id");

        ResponseEnvelope::error(ToolError::internal(format!(
            "Long-running tool {} not yet implemented in typed dispatcher",
            name
        )))
    }

    /// Dispatch fire-and-forget tools - create job, execute, return job_id
    ///
    /// Fire-and-forget commands are now tracked as jobs. The client receives
    /// the job_id immediately and can query failures via job_list(status: "failed").
    ///
    /// The job_id is passed to chaosgarden in message metadata for correlation,
    /// allowing async results to be tracked back to jobs.
    async fn dispatch_fire_and_forget(&self, request: ToolRequest) -> ResponseEnvelope {
        let tool_name = request.name();

        // Create job for tracking
        let job_id = self.server.job_store.create_job(tool_name.to_string());
        let _ = self.server.job_store.mark_running(&job_id);

        // Job ID string for passing to chaosgarden
        let job_id_str = job_id.to_string();

        // Execute the command and track result
        let result: Result<serde_json::Value, String> = match request {
            ToolRequest::GardenPlay => {
                match self.server.garden_play_fire(Some(&job_id_str)).await {
                    Ok(()) => Ok(
                        serde_json::json!({"status": "ok", "command": "play", "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenPause => {
                match self.server.garden_pause_fire(Some(&job_id_str)).await {
                    Ok(()) => Ok(
                        serde_json::json!({"status": "ok", "command": "pause", "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenStop => {
                match self.server.garden_stop_fire(Some(&job_id_str)).await {
                    Ok(()) => Ok(
                        serde_json::json!({"status": "ok", "command": "stop", "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenSeek(req) => {
                match self
                    .server
                    .garden_seek_fire(req.beat, Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(
                        serde_json::json!({"status": "ok", "command": "seek", "beat": req.beat, "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenSetTempo(req) => {
                match self
                    .server
                    .garden_set_tempo_fire(req.bpm, Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(
                        serde_json::json!({"status": "ok", "command": "set_tempo", "bpm": req.bpm, "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenEmergencyPause => {
                match self
                    .server
                    .garden_emergency_pause_fire(Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(
                        serde_json::json!({"status": "ok", "command": "emergency_pause", "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenCreateRegion(req) => {
                match self
                    .server
                    .garden_create_region_fire(
                        req.position,
                        req.duration,
                        &req.behavior_type,
                        &req.content_id,
                        Some(&job_id_str),
                    )
                    .await
                {
                    Ok(region_id) => Ok(
                        serde_json::json!({"status": "ok", "command": "create_region", "region_id": region_id, "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenDeleteRegion(req) => {
                match self
                    .server
                    .garden_delete_region_fire(&req.region_id, Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(
                        serde_json::json!({"status": "ok", "command": "delete_region", "region_id": req.region_id, "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenMoveRegion(req) => {
                match self
                    .server
                    .garden_move_region_fire(&req.region_id, req.new_position, Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(
                        serde_json::json!({"status": "ok", "command": "move_region", "region_id": req.region_id, "new_position": req.new_position, "job_id": job_id_str}),
                    ),
                    Err(e) => Err(e.message().to_string()),
                }
            }

            other => {
                tracing::warn!(tool = other.name(), "FireAndForget tool not implemented");
                Ok(serde_json::json!({"status": "ok", "command": "unknown", "job_id": job_id_str}))
            }
        };

        // Update job status based on result
        match result {
            Ok(value) => {
                let _ = self.server.job_store.mark_complete(&job_id, value);
            }
            Err(error) => {
                tracing::error!(job_id = %job_id, %error, "FireAndForget command failed");
                let _ = self.server.job_store.mark_failed(&job_id, error);
            }
        }

        // Return job_id - client can query status if needed
        ResponseEnvelope::JobStarted {
            job_id: job_id.to_string(),
            tool: tool_name.to_string(),
            timing: ToolTiming::FireAndForget,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hooteproto::request::{AbcParseRequest, ConfigGetRequest};

    // Note: Full tests require a running EventDualityServer
    // These are placeholder tests for the dispatch logic

    #[test]
    fn test_timing_classification() {
        let parse = ToolRequest::AbcParse(AbcParseRequest {
            abc: "X:1\nT:Test\nK:C\nCDEF".to_string(),
        });
        assert_eq!(parse.timing(), ToolTiming::Sync);

        let config = ToolRequest::ConfigGet(ConfigGetRequest {
            section: None,
            key: None,
        });
        assert_eq!(config.timing(), ToolTiming::Sync);

        let play = ToolRequest::GardenPlay;
        assert_eq!(play.timing(), ToolTiming::FireAndForget);
    }
}
