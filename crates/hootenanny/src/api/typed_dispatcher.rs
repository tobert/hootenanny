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
    Payload, ToolError,
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
            // All short/medium operations go through dispatch_async
            ToolTiming::AsyncShort | ToolTiming::AsyncMedium => {
                self.dispatch_async(request).await
            }
            ToolTiming::AsyncLong => {
                // Return job_id immediately for long-running tools
                self.dispatch_async_return_job_id(request).await
            }
            ToolTiming::FireAndForget => self.dispatch_fire_and_forget(request).await,
        }
    }

    /// Dispatch all tools - unified handler for all timing classes except FireAndForget/AsyncLong
    async fn dispatch_async(&self, request: ToolRequest) -> ResponseEnvelope {
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
            ToolRequest::AbcToMidi(req) => {
                match self
                    .server
                    .abc_to_midi_typed(
                        &req.abc,
                        req.tempo_override,
                        req.transpose,
                        req.velocity,
                        req.channel,
                        req.tags,
                        req.creator,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::AbcToMidi(resp)),
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

            // === Garden Status/Query (ZMQ to chaosgarden) ===
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

            // === Jobs ===
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
            ToolRequest::JobPoll(req) => {
                match self
                    .server
                    .job_poll_typed(req.job_ids, req.timeout_ms, req.mode)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::JobPoll(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::JobCancel(req) => {
                match self.server.job_cancel_typed(&req.job_id).await {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::JobCancel(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::JobSleep(req) => {
                match self.server.job_sleep_typed(req.milliseconds).await {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::JobSleep(resp)),
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
            ToolRequest::CasStore(req) => {
                match self.server.cas_store_typed(&req.data, &req.mime_type).await {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::CasStored(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::CasGet(req) => match self.server.cas_get_typed(&req.hash).await {
                Ok(resp) => ResponseEnvelope::success(ToolResponse::CasContent(resp)),
                Err(e) => ResponseEnvelope::error(e),
            },
            ToolRequest::CasStats => match self.server.cas_stats_typed().await {
                Ok(resp) => ResponseEnvelope::success(ToolResponse::CasStats(resp)),
                Err(e) => ResponseEnvelope::error(e),
            },
            ToolRequest::CasUploadFile(req) => {
                match self
                    .server
                    .cas_upload_file_typed(&req.file_path, &req.mime_type)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::CasStored(resp)),
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
            ToolRequest::ArtifactUpload(req) => {
                match self
                    .server
                    .artifact_upload_typed(
                        &req.file_path,
                        &req.mime_type,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::ArtifactCreated(resp)),
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
            ToolRequest::GraphBind(req) => {
                match self
                    .server
                    .graph_bind_typed(&req.id, &req.name, req.hints)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::GraphBind(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::GraphTag(req) => {
                match self
                    .server
                    .graph_tag_typed(&req.identity_id, &req.namespace, &req.value)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::GraphTag(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::GraphConnect(req) => {
                match self
                    .server
                    .graph_connect_typed(
                        &req.from_identity,
                        &req.from_port,
                        &req.to_identity,
                        &req.to_port,
                        req.transport,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::GraphConnect(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Annotations ===
            ToolRequest::AddAnnotation(req) => {
                match self
                    .server
                    .add_annotation_typed(&req.artifact_id, &req.message, req.source, req.vibe)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::AnnotationAdded(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === MIDI to WAV ===
            ToolRequest::MidiToWav(req) => {
                match self
                    .server
                    .midi_to_wav_typed(
                        &req.input_hash,
                        &req.soundfont_hash,
                        req.sample_rate,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::MidiToWav(resp)),
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

            // === Orpheus Generation ===
            ToolRequest::OrpheusGenerate(req) => {
                match self
                    .server
                    .orpheus_generate_typed(
                        req.max_tokens,
                        req.num_variations,
                        req.temperature,
                        req.top_p,
                        req.model,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::OrpheusGenerated(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::OrpheusGenerateSeeded(req) => {
                match self
                    .server
                    .orpheus_generate_seeded_typed(
                        &req.seed_hash,
                        req.max_tokens,
                        req.num_variations,
                        req.temperature,
                        req.top_p,
                        req.model,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::OrpheusGenerated(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::OrpheusContinue(req) => {
                match self
                    .server
                    .orpheus_continue_typed(
                        &req.input_hash,
                        req.max_tokens,
                        req.num_variations,
                        req.temperature,
                        req.top_p,
                        req.model,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::OrpheusGenerated(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::OrpheusBridge(req) => {
                match self
                    .server
                    .orpheus_bridge_typed(
                        &req.section_a_hash,
                        req.section_b_hash,
                        req.max_tokens,
                        req.temperature,
                        req.top_p,
                        req.model,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::OrpheusGenerated(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::OrpheusLoops(req) => {
                match self
                    .server
                    .orpheus_loops_typed(
                        req.seed_hash,
                        req.max_tokens,
                        req.num_variations,
                        req.temperature,
                        req.top_p,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::OrpheusGenerated(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === AsyncLong Tools (return job_id immediately) ===
            ToolRequest::MusicgenGenerate(req) => {
                match self
                    .server
                    .musicgen_generate_typed(
                        req.prompt,
                        req.duration,
                        req.temperature,
                        req.top_k,
                        req.top_p,
                        req.guidance_scale,
                        req.do_sample,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::JobStarted(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::YueGenerate(req) => {
                match self
                    .server
                    .yue_generate_typed(
                        req.lyrics,
                        req.genre,
                        req.max_new_tokens,
                        req.run_n_segments,
                        req.seed,
                        req.tags,
                        req.creator,
                        req.parent_id,
                        req.variation_set_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::JobStarted(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::BeatthisAnalyze(req) => {
                match self
                    .server
                    .beatthis_analyze_typed(req.audio_hash, req.audio_path, req.include_frames)
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::JobStarted(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }
            ToolRequest::ClapAnalyze(req) => {
                match self
                    .server
                    .clap_analyze_typed(
                        req.audio_hash,
                        req.audio_b_hash,
                        req.tasks,
                        req.text_candidates,
                        req.creator,
                        req.parent_id,
                    )
                    .await
                {
                    Ok(resp) => ResponseEnvelope::success(ToolResponse::JobStarted(resp)),
                    Err(e) => ResponseEnvelope::error(e),
                }
            }

            // === Vibeweaver (Python kernel proxy) ===
            ToolRequest::WeaveEval(_)
            | ToolRequest::WeaveSession
            | ToolRequest::WeaveReset(_)
            | ToolRequest::WeaveHelp(_) => self.dispatch_vibeweaver(request).await,

            // === Admin ===
            ToolRequest::Ping => ResponseEnvelope::ack("pong"),
            ToolRequest::ListTools => {
                use hooteproto::responses::ToolsListResponse;
                let tools = crate::api::tools_registry::list_tools();
                let count = tools.len();
                ResponseEnvelope::success(ToolResponse::ToolsList(ToolsListResponse {
                    tools,
                    count,
                }))
            }

            // Fallback for tools not yet implemented
            other => {
                let tool_name = other.name();
                tracing::warn!(
                    tool = tool_name,
                    "Tool not yet implemented in typed dispatcher"
                );
                ResponseEnvelope::error(ToolError::internal(format!(
                    "Tool '{}' not yet implemented in typed dispatcher",
                    tool_name
                )))
            }
        }
    }

    /// Dispatch vibeweaver tools - proxy to Python kernel
    async fn dispatch_vibeweaver(&self, request: ToolRequest) -> ResponseEnvelope {
        let vibeweaver = match &self.server.vibeweaver {
            Some(v) => v,
            None => {
                return ResponseEnvelope::error(ToolError::internal(
                    "Python kernel requires vibeweaver connection. \
                     Configure bootstrap.connections.vibeweaver in config.",
                ));
            }
        };

        let tool_name = request.name();
        tracing::debug!(tool = tool_name, "Proxying to vibeweaver");

        // Convert to Payload for ZMQ transport
        let payload = Payload::ToolRequest(request);

        match vibeweaver.request(payload).await {
            Ok(Payload::TypedResponse(envelope)) => {
                // Pass through the envelope from vibeweaver
                envelope
            }
            Ok(Payload::Error { code, message, .. }) => {
                ResponseEnvelope::error(ToolError::internal(format!("{}: {}", code, message)))
            }
            Ok(other) => ResponseEnvelope::error(ToolError::internal(format!(
                "Unexpected response from vibeweaver: {:?}",
                std::mem::discriminant(&other)
            ))),
            Err(e) => {
                tracing::warn!(tool = tool_name, error = %e, "Vibeweaver proxy error");
                ResponseEnvelope::error(ToolError::internal(format!(
                    "Vibeweaver proxy error: {}",
                    e
                )))
            }
        }
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
        let result: Result<ToolResponse, String> = match request {
            ToolRequest::GardenPlay => {
                match self.server.garden_play_fire(Some(&job_id_str)).await {
                    Ok(()) => Ok(ToolResponse::ack("play")),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenPause => {
                match self.server.garden_pause_fire(Some(&job_id_str)).await {
                    Ok(()) => Ok(ToolResponse::ack("pause")),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenStop => {
                match self.server.garden_stop_fire(Some(&job_id_str)).await {
                    Ok(()) => Ok(ToolResponse::ack("stop")),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenSeek(req) => {
                match self
                    .server
                    .garden_seek_fire(req.beat, Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(ToolResponse::ack(format!("seek to beat {}", req.beat))),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenSetTempo(req) => {
                match self
                    .server
                    .garden_set_tempo_fire(req.bpm, Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(ToolResponse::ack(format!("set tempo to {} bpm", req.bpm))),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenEmergencyPause => {
                match self
                    .server
                    .garden_emergency_pause_fire(Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(ToolResponse::ack("emergency pause")),
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
                    Ok(region_id) => Ok(ToolResponse::GardenRegionCreated(
                        hooteproto::responses::GardenRegionCreatedResponse {
                            region_id,
                            position: req.position,
                            duration: req.duration,
                        },
                    )),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenDeleteRegion(req) => {
                match self
                    .server
                    .garden_delete_region_fire(&req.region_id, Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(ToolResponse::ack(format!("deleted region {}", req.region_id))),
                    Err(e) => Err(e.message().to_string()),
                }
            }
            ToolRequest::GardenMoveRegion(req) => {
                match self
                    .server
                    .garden_move_region_fire(&req.region_id, req.new_position, Some(&job_id_str))
                    .await
                {
                    Ok(()) => Ok(ToolResponse::ack(format!(
                        "moved region {} to position {}",
                        req.region_id, req.new_position
                    ))),
                    Err(e) => Err(e.message().to_string()),
                }
            }

            other => {
                tracing::warn!(tool = other.name(), "FireAndForget tool not implemented");
                Ok(ToolResponse::ack("unknown command"))
            }
        };

        // Update job status based on result
        match result {
            Ok(response) => {
                let _ = self.server.job_store.mark_complete(&job_id, response);
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
        assert_eq!(parse.timing(), ToolTiming::AsyncShort);

        let config = ToolRequest::ConfigGet(ConfigGetRequest {
            section: None,
            key: None,
        });
        assert_eq!(config.timing(), ToolTiming::AsyncShort);

        let play = ToolRequest::GardenPlay;
        assert_eq!(play.timing(), ToolTiming::FireAndForget);
    }
}
