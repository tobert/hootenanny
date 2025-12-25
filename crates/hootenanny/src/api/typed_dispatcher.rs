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

            // === CAS ===
            ToolRequest::CasInspect(req) => {
                match self.server.cas_inspect_typed(&req.hash).await {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::CasInspected(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Artifacts ===
            ToolRequest::ArtifactGet(req) => {
                match self.server.artifact_get_typed(&req.id).await {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::ArtifactInfo(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::ArtifactList(req) => {
                match self
                    .server
                    .artifact_list_typed(req.tag.as_deref(), req.creator.as_deref())
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::ArtifactList(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Graph ===
            ToolRequest::GraphFind(req) => {
                match self
                    .server
                    .graph_find_typed(
                        req.name.as_deref(),
                        req.tag_namespace.as_deref(),
                        req.tag_value.as_deref(),
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::GraphIdentities(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::GraphContext(req) => {
                match self
                    .server
                    .graph_context_typed(
                        req.limit.map(|l| l as usize),
                        req.tag.as_deref(),
                        req.creator.as_deref(),
                        req.vibe_search.as_deref(),
                        req.within_minutes,
                        req.include_annotations,
                        req.include_metadata,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::GraphContext(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::GraphQuery(req) => {
                match self
                    .server
                    .graph_query_typed(&req.query, req.limit.map(|l| l as usize), req.variables.as_ref())
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::GraphQueryResult(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Garden Query ===
            ToolRequest::GardenQuery(req) => {
                match self
                    .server
                    .garden_query_typed(&req.query, req.variables.as_ref())
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::GardenQueryResult(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Orpheus Classify ===
            ToolRequest::OrpheusClassify(req) => {
                match self.server.orpheus_classify_typed(&req.midi_hash).await {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::OrpheusClassified(resp)),
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

    /// Dispatch async tools - creates job, spawns task, returns job_id
    ///
    /// AsyncShort/AsyncMedium tools spawn a background task and return the job_id.
    /// The client can poll with job_status or job_poll to get results.
    async fn dispatch_async(&self, request: ToolRequest) -> ResponseEnvelope {
        let tool_name = request.name();
        let timing = request.timing();

        // Create job for tracking
        let job_id = self.server.job_store.create_job(tool_name.to_string());

        // Clone what the spawned task needs
        let server = Arc::clone(&self.server);
        let job_id_clone = job_id.clone();

        // Spawn background task to execute the tool
        tokio::spawn(async move {
            let _ = server.job_store.mark_running(&job_id_clone);

            // Execute the tool via JSON dispatch (existing infrastructure)
            let result = Self::execute_async_tool(&server, request).await;

            // Update job status based on result
            match result {
                Ok(value) => {
                    let _ = server.job_store.mark_complete(&job_id_clone, value);
                }
                Err(error) => {
                    tracing::error!(job_id = %job_id_clone, %error, "Async tool failed");
                    let _ = server.job_store.mark_failed(&job_id_clone, error);
                }
            }
        });

        // Return job_id immediately
        ResponseEnvelope::JobStarted {
            job_id: job_id.to_string(),
            tool: tool_name.to_string(),
            timing,
        }
    }

    /// Execute an async tool and return its result
    async fn execute_async_tool(
        server: &EventDualityServer,
        request: ToolRequest,
    ) -> Result<serde_json::Value, String> {
        use crate::api::dispatch::dispatch_tool;

        // Convert request back to JSON args for the existing dispatch infrastructure
        let (name, args) = Self::request_to_tool_args(&request)?;

        // Call the existing JSON dispatch (returns serde_json::Value directly)
        match dispatch_tool(server, &name, args).await {
            Ok(value) => Ok(value),
            Err(e) => Err(e.message),
        }
    }

    /// Convert a ToolRequest back to (name, args) for JSON dispatch
    ///
    /// Note: Some tools have different names in the typed vs JSON dispatch.
    /// The typed request uses `orpheus_generate` but JSON dispatch uses `sample`.
    fn request_to_tool_args(request: &ToolRequest) -> Result<(String, serde_json::Value), String> {
        // Map typed request to JSON dispatch name and args
        let (name, args) = match request {
            // Orpheus tools map to "sample" with space field
            ToolRequest::OrpheusGenerate(req) => {
                let mut args = serde_json::to_value(req).map_err(|e| e.to_string())?;
                args["space"] = serde_json::json!("orpheus");
                ("sample".to_string(), args)
            }
            ToolRequest::OrpheusGenerateSeeded(req) => {
                let mut args = serde_json::to_value(req).map_err(|e| e.to_string())?;
                args["space"] = serde_json::json!("orpheus");
                args["seed"] = serde_json::json!({"type": "midi", "artifact_id": req.seed_hash});
                ("sample".to_string(), args)
            }
            ToolRequest::OrpheusContinue(req) => {
                let args = serde_json::json!({
                    "encoding": {"type": "midi", "artifact_id": req.input_hash},
                    "space": "orpheus",
                    "inference": {
                        "max_tokens": req.max_tokens,
                        "temperature": req.temperature,
                        "top_p": req.top_p,
                    },
                    "num_variations": req.num_variations,
                    "tags": req.tags,
                    "creator": req.creator,
                    "parent_id": req.parent_id,
                });
                ("extend".to_string(), args)
            }
            ToolRequest::OrpheusBridge(req) => {
                let args = serde_json::json!({
                    "from": {"type": "midi", "artifact_id": req.section_a_hash},
                    "to": req.section_b_hash.as_ref().map(|h| serde_json::json!({"type": "midi", "artifact_id": h})),
                    "inference": {
                        "max_tokens": req.max_tokens,
                        "temperature": req.temperature,
                        "top_p": req.top_p,
                    },
                    "tags": req.tags,
                    "creator": req.creator,
                    "parent_id": req.parent_id,
                });
                ("bridge".to_string(), args)
            }
            ToolRequest::OrpheusLoops(req) => {
                let mut args = serde_json::to_value(req).map_err(|e| e.to_string())?;
                args["space"] = serde_json::json!("orpheus_loops");
                args["as_loop"] = serde_json::json!(true);
                ("sample".to_string(), args)
            }

            // MidiToWav maps to "project"
            ToolRequest::MidiToWav(req) => {
                let args = serde_json::json!({
                    "encoding": {"type": "midi", "artifact_id": req.input_hash},
                    "target": {
                        "type": "audio",
                        "soundfont_hash": req.soundfont_hash,
                        "sample_rate": req.sample_rate.unwrap_or(44100),
                    },
                    "tags": req.tags,
                    "creator": req.creator,
                    "parent_id": req.parent_id,
                });
                ("project".to_string(), args)
            }

            // MusicGen maps to "sample" with space
            ToolRequest::MusicgenGenerate(req) => {
                let mut args = serde_json::to_value(req).map_err(|e| e.to_string())?;
                args["space"] = serde_json::json!("music_gen");
                ("sample".to_string(), args)
            }

            // YuE maps to "sample" with space
            ToolRequest::YueGenerate(req) => {
                let mut args = serde_json::to_value(req).map_err(|e| e.to_string())?;
                args["space"] = serde_json::json!("yue");
                ("sample".to_string(), args)
            }

            // Analysis tools map to "analyze"
            ToolRequest::BeatthisAnalyze(req) => {
                let args = serde_json::json!({
                    "encoding": {"type": "audio", "artifact_id": req.audio_hash},
                    "tasks": ["beats"],
                });
                ("analyze".to_string(), args)
            }
            ToolRequest::ClapAnalyze(req) => {
                let args = serde_json::json!({
                    "encoding": {"type": "audio", "artifact_id": req.audio_hash},
                    "tasks": ["embeddings"],
                });
                ("analyze".to_string(), args)
            }

            // Direct mappings (name matches)
            ToolRequest::CasStore(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::CasGet(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::CasUploadFile(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::ArtifactUpload(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::AbcToMidi(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::GraphBind(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::GraphTag(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::GraphConnect(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::AddAnnotation(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::JobPoll(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::JobCancel(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            ToolRequest::JobSleep(req) => {
                (request.name().to_string(), serde_json::to_value(req).map_err(|e| e.to_string())?)
            }
            _ => return Err(format!("Tool {} not supported for async dispatch", request.name())),
        };

        Ok((name, args))
    }

    /// Dispatch long-running async tools - return job_id immediately
    ///
    /// AsyncLong tools (MusicGen, YuE, etc.) spawn a background task and return job_id.
    /// Same as dispatch_async but semantically different: clients expect longer waits.
    async fn dispatch_async_return_job_id(&self, request: ToolRequest) -> ResponseEnvelope {
        // Long-running tools use the same job-spawning pattern as medium async tools
        // The only difference is timing semantics (client expects longer wait)
        self.dispatch_async(request).await
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
