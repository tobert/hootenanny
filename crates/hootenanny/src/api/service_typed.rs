//! Typed method implementations for EventDualityServer.
//!
//! These methods return typed ToolResponse variants directly.
//! They are used by the TypedDispatcher for the Cap'n Proto protocol.

use crate::api::service::EventDualityServer;
use crate::artifact_store::ArtifactStore;
use std::sync::Arc;
use hooteproto::{
    responses::{
        AbcParsedResponse, AbcTransposedResponse, AbcValidatedResponse, AbcValidationError,
        ConfigValue, ConfigValueResponse, GardenRegionsResponse, GardenStatusResponse, JobCounts,
        JobListResponse, JobState, JobStatusResponse, SoundfontInfoResponse, SoundfontPreset,
        SoundfontPresetInfoResponse, SoundfontRegion, TransportState,
    },
    ToolError,
};

impl EventDualityServer {
    // =========================================================================
    // ABC Notation - Typed
    // =========================================================================

    /// Parse ABC notation - typed response
    pub async fn abc_parse_typed(&self, abc_str: &str) -> Result<AbcParsedResponse, ToolError> {
        let parse_result = abc::parse(abc_str);

        if parse_result.has_errors() {
            // Return a valid response even with errors - just mark as invalid
            return Ok(AbcParsedResponse {
                valid: false,
                title: None,
                key: None,
                meter: None,
                tempo: None,
                notes_count: 0,
            });
        }

        let tune = &parse_result.value;

        // Format meter from enum
        let meter_str = tune.header.meter.as_ref().map(|m| match m {
            abc::Meter::Simple {
                numerator,
                denominator,
            } => format!("{}/{}", numerator, denominator),
            abc::Meter::Common => "4/4".to_string(),
            abc::Meter::Cut => "2/2".to_string(),
            abc::Meter::None => "free".to_string(),
        });

        // Format key
        let key_str = Some(format!(
            "{:?}{}",
            tune.header.key.root,
            tune.header
                .key
                .accidental
                .map(|a| format!("{:?}", a))
                .unwrap_or_default()
        ));

        // Extract tempo from Tempo struct if present
        let tempo_val = tune.header.tempo.as_ref().map(|t| t.bpm);

        // Count notes across all voices
        let notes_count: usize = tune
            .voices
            .iter()
            .flat_map(|v| v.elements.iter())
            .filter(|e| matches!(e, abc::Element::Note(_)))
            .count();

        // Title - use first one or empty
        let title = if tune.header.title.is_empty() {
            None
        } else {
            Some(tune.header.title.clone())
        };

        Ok(AbcParsedResponse {
            valid: true,
            title,
            key: key_str,
            meter: meter_str,
            tempo: tempo_val,
            notes_count,
        })
    }

    /// Validate ABC notation - typed response
    pub async fn abc_validate_typed(
        &self,
        abc_str: &str,
    ) -> Result<AbcValidatedResponse, ToolError> {
        let parse_result = abc::parse(abc_str);

        let errors: Vec<AbcValidationError> = parse_result
            .errors()
            .map(|e| AbcValidationError {
                line: e.line,
                column: e.column,
                message: e.message.clone(),
            })
            .collect();

        let warnings: Vec<String> = parse_result.warnings().map(|w| w.message.clone()).collect();

        Ok(AbcValidatedResponse {
            valid: errors.is_empty(),
            errors,
            warnings,
        })
    }

    /// Transpose ABC notation - typed response
    pub async fn abc_transpose_typed(
        &self,
        abc_str: &str,
        semitones: Option<i8>,
        target_key: Option<&str>,
    ) -> Result<AbcTransposedResponse, ToolError> {
        let parse_result = abc::parse(abc_str);

        if parse_result.has_errors() {
            return Err(ToolError::validation(
                "invalid_params",
                "ABC notation has parse errors",
            ));
        }

        // Format original key
        let original_key = Some(format!(
            "{:?}{}",
            parse_result.value.header.key.root,
            parse_result
                .value
                .header
                .key
                .accidental
                .map(|a| format!("{:?}", a))
                .unwrap_or_default()
        ));

        let semitones_actual = if let Some(s) = semitones {
            s
        } else if let Some(target) = target_key {
            abc::semitones_to_key(&parse_result.value.header.key, target)
                .map_err(|e| ToolError::validation("invalid_params", e))?
        } else {
            return Err(ToolError::validation(
                "invalid_params",
                "Must specify either semitones or target_key",
            ));
        };

        let transposed = abc::transpose(&parse_result.value, semitones_actual);
        let new_key = Some(format!(
            "{:?}{}",
            transposed.header.key.root,
            transposed
                .header
                .key
                .accidental
                .map(|a| format!("{:?}", a))
                .unwrap_or_default()
        ));
        let abc_out = abc::to_abc(&transposed);

        Ok(AbcTransposedResponse {
            abc: abc_out,
            original_key,
            new_key,
            semitones: semitones_actual,
        })
    }

    // =========================================================================
    // SoundFont - Typed
    // =========================================================================

    /// Inspect SoundFont presets - typed response
    pub async fn soundfont_inspect_typed(
        &self,
        soundfont_hash: &str,
        include_drum_map: bool,
    ) -> Result<SoundfontInfoResponse, ToolError> {
        use crate::mcp_tools::rustysynth::inspect_soundfont;

        let cas_ref = self
            .local_models
            .inspect_cas_content(soundfont_hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to get soundfont from CAS: {}", e)))?;

        let local_path = cas_ref
            .local_path
            .ok_or_else(|| ToolError::not_found("soundfont", soundfont_hash))?;

        // Read soundfont bytes from file
        let soundfont_bytes = tokio::fs::read(&local_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read soundfont file: {}", e)))?;

        let info = inspect_soundfont(&soundfont_bytes, include_drum_map)
            .map_err(|e| ToolError::internal(format!("Failed to inspect soundfont: {}", e)))?;

        let presets: Vec<SoundfontPreset> = info
            .presets
            .iter()
            .map(|p| SoundfontPreset {
                bank: p.bank as u16,
                program: p.program as u16,
                name: p.name.clone(),
            })
            .collect();

        Ok(SoundfontInfoResponse {
            name: info.info.name.clone(),
            preset_count: presets.len(),
            presets,
        })
    }

    /// Inspect specific SoundFont preset - typed response
    pub async fn soundfont_preset_inspect_typed(
        &self,
        soundfont_hash: &str,
        bank: u16,
        program: u16,
    ) -> Result<SoundfontPresetInfoResponse, ToolError> {
        use crate::mcp_tools::rustysynth::inspect_preset;

        let cas_ref = self
            .local_models
            .inspect_cas_content(soundfont_hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to get soundfont from CAS: {}", e)))?;

        let local_path = cas_ref
            .local_path
            .ok_or_else(|| ToolError::not_found("soundfont", soundfont_hash))?;

        // Read soundfont bytes from file
        let soundfont_bytes = tokio::fs::read(&local_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read soundfont file: {}", e)))?;

        let info = inspect_preset(&soundfont_bytes, bank as i32, program as i32)
            .map_err(|e| ToolError::internal(format!("Failed to inspect preset: {}", e)))?;

        // RegionDetail has different fields - we'll create a simplified mapping
        let regions: Vec<SoundfontRegion> = info
            .regions
            .iter()
            .map(|r| {
                // Parse keys range like "C4-C5" into low/high
                // For now just use placeholder values
                SoundfontRegion {
                    key_low: 0,
                    key_high: 127,
                    velocity_low: 1,
                    velocity_high: 127,
                    sample_name: Some(r.instrument.clone()),
                }
            })
            .collect();

        Ok(SoundfontPresetInfoResponse {
            bank,
            program,
            name: info.name.clone(),
            regions,
        })
    }

    // =========================================================================
    // Garden - Typed
    // =========================================================================

    /// Get garden status - typed response
    pub async fn garden_status_typed(&self) -> Result<GardenStatusResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::GardenStatus).await {
            Ok(ToolResponse::GardenStatus(response)) => Ok(response),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for GardenStatus: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service(
                "chaosgarden",
                "status_failed",
                e.to_string(),
            )),
        }
    }

    /// Get garden regions - typed response
    pub async fn garden_get_regions_typed(
        &self,
        start: Option<f64>,
        end: Option<f64>,
    ) -> Result<GardenRegionsResponse, ToolError> {
        use hooteproto::request::{GardenGetRegionsRequest, ToolRequest};
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        let request = ToolRequest::GardenGetRegions(GardenGetRegionsRequest { start, end });

        match manager.tool_request(request).await {
            Ok(ToolResponse::GardenRegions(response)) => Ok(response),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for GetRegions: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service(
                "chaosgarden",
                "get_regions_failed",
                e.to_string(),
            )),
        }
    }

    // =========================================================================
    // Garden - Fire and Forget helpers
    //
    // These methods accept an optional job_id for correlation. The job_id is
    // passed to chaosgarden in message metadata, allowing hootenanny to track
    // async results back to jobs.
    // =========================================================================

    pub async fn garden_play_fire(&self, _job_id: Option<&str>) -> Result<(), ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::GardenPlay).await {
            Ok(ToolResponse::Ack(_)) => Ok(()),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for Play: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service("chaosgarden", "play_failed", e.to_string())),
        }
    }

    pub async fn garden_pause_fire(&self, _job_id: Option<&str>) -> Result<(), ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::GardenPause).await {
            Ok(ToolResponse::Ack(_)) => Ok(()),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for Pause: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service("chaosgarden", "pause_failed", e.to_string())),
        }
    }

    pub async fn garden_stop_fire(&self, _job_id: Option<&str>) -> Result<(), ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::GardenStop).await {
            Ok(ToolResponse::Ack(_)) => Ok(()),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for Stop: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service("chaosgarden", "stop_failed", e.to_string())),
        }
    }

    pub async fn garden_seek_fire(&self, beat: f64, _job_id: Option<&str>) -> Result<(), ToolError> {
        use hooteproto::request::{GardenSeekRequest, ToolRequest};
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        let request = ToolRequest::GardenSeek(GardenSeekRequest { beat });

        match manager.tool_request(request).await {
            Ok(ToolResponse::Ack(_)) => Ok(()),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for Seek: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service("chaosgarden", "seek_failed", e.to_string())),
        }
    }

    pub async fn garden_set_tempo_fire(
        &self,
        bpm: f64,
        _job_id: Option<&str>,
    ) -> Result<(), ToolError> {
        use hooteproto::request::{GardenSetTempoRequest, ToolRequest};
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        let request = ToolRequest::GardenSetTempo(GardenSetTempoRequest { bpm });

        match manager.tool_request(request).await {
            Ok(ToolResponse::Ack(_)) => Ok(()),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for SetTempo: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service("chaosgarden", "set_tempo_failed", e.to_string())),
        }
    }

    pub async fn garden_emergency_pause_fire(&self, _job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::ControlRequest;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;
        // Emergency pause goes on control channel (no job_id support there yet)
        manager
            .control(ControlRequest::EmergencyPause)
            .await
            .map_err(|e| {
                ToolError::service("chaosgarden", "emergency_pause_failed", e.to_string())
            })?;
        Ok(())
    }

    pub async fn garden_create_region_fire(
        &self,
        position: f64,
        duration: f64,
        behavior_type: &str,
        content_id: &str,
        _job_id: Option<&str>,
    ) -> Result<String, ToolError> {
        use hooteproto::request::{GardenCreateRegionRequest, ToolRequest};
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        let request = ToolRequest::GardenCreateRegion(GardenCreateRegionRequest {
            position,
            duration,
            behavior_type: behavior_type.to_string(),
            content_id: content_id.to_string(),
        });

        match manager.tool_request(request).await {
            Ok(ToolResponse::GardenRegionCreated(response)) => Ok(response.region_id),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for CreateRegion: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service(
                "chaosgarden",
                "create_region_failed",
                e.to_string(),
            )),
        }
    }

    pub async fn garden_delete_region_fire(
        &self,
        region_id: &str,
        _job_id: Option<&str>,
    ) -> Result<(), ToolError> {
        use hooteproto::request::{GardenDeleteRegionRequest, ToolRequest};
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        let request = ToolRequest::GardenDeleteRegion(GardenDeleteRegionRequest {
            region_id: region_id.to_string(),
        });

        match manager.tool_request(request).await {
            Ok(ToolResponse::Ack(_)) => Ok(()),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for DeleteRegion: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service(
                "chaosgarden",
                "delete_region_failed",
                e.to_string(),
            )),
        }
    }

    pub async fn garden_move_region_fire(
        &self,
        region_id: &str,
        new_position: f64,
        _job_id: Option<&str>,
    ) -> Result<(), ToolError> {
        use hooteproto::request::{GardenMoveRegionRequest, ToolRequest};
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        let request = ToolRequest::GardenMoveRegion(GardenMoveRegionRequest {
            region_id: region_id.to_string(),
            new_position,
        });

        match manager.tool_request(request).await {
            Ok(ToolResponse::Ack(_)) => Ok(()),
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for MoveRegion: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service(
                "chaosgarden",
                "move_region_failed",
                e.to_string(),
            )),
        }
    }

    pub async fn garden_clear_regions_fire(
        &self,
        _job_id: Option<&str>,
    ) -> Result<usize, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        let request = ToolRequest::GardenClearRegions;

        match manager.tool_request(request).await {
            Ok(ToolResponse::Ack(ack)) => {
                // Parse count from ack message if needed, default to 0
                let count = ack.message.split_whitespace()
                    .find_map(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                Ok(count)
            }
            Ok(other) => Err(ToolError::internal(format!(
                "Unexpected response for ClearRegions: {:?}",
                other
            ))),
            Err(e) => Err(ToolError::service(
                "chaosgarden",
                "clear_regions_failed",
                e.to_string(),
            )),
        }
    }

    // =========================================================================
    // Jobs - Typed
    // =========================================================================

    /// Get job status - typed response
    pub async fn job_status_typed(&self, job_id: &str) -> Result<JobStatusResponse, ToolError> {
        use hooteproto::JobStatus;

        let job_id_typed = hooteproto::JobId::from(job_id);
        let info = self
            .job_store
            .get_job(&job_id_typed)
            .map_err(|e| ToolError::not_found("job", e.to_string()))?;

        let status = match info.status {
            JobStatus::Pending => JobState::Pending,
            JobStatus::Running => JobState::Running,
            JobStatus::Complete => JobState::Complete,
            JobStatus::Failed => JobState::Failed,
            JobStatus::Cancelled => JobState::Cancelled,
        };

        Ok(JobStatusResponse {
            job_id: info.job_id.to_string(),
            status,
            source: info.source,
            result: info.result.map(Box::new),
            error: info.error,
            created_at: info.created_at,
            started_at: info.started_at,
            completed_at: info.completed_at,
        })
    }

    /// List jobs - typed response
    pub async fn job_list_typed(
        &self,
        status_filter: Option<&str>,
    ) -> Result<JobListResponse, ToolError> {
        use hooteproto::JobStatus;

        let jobs_raw = self.job_store.list_jobs();

        let mut counts = JobCounts::default();
        let jobs: Vec<JobStatusResponse> = jobs_raw
            .into_iter()
            .filter(|info| {
                // Apply filter if provided
                match status_filter {
                    Some("pending") => matches!(info.status, JobStatus::Pending),
                    Some("running") => matches!(info.status, JobStatus::Running),
                    Some("complete") => matches!(info.status, JobStatus::Complete),
                    Some("failed") => matches!(info.status, JobStatus::Failed),
                    Some("cancelled") => matches!(info.status, JobStatus::Cancelled),
                    Some(_) | None => true,
                }
            })
            .map(|info| {
                let status = match info.status {
                    JobStatus::Pending => {
                        counts.pending += 1;
                        JobState::Pending
                    }
                    JobStatus::Running => {
                        counts.running += 1;
                        JobState::Running
                    }
                    JobStatus::Complete => {
                        counts.complete += 1;
                        JobState::Complete
                    }
                    JobStatus::Failed => {
                        counts.failed += 1;
                        JobState::Failed
                    }
                    JobStatus::Cancelled => {
                        counts.cancelled += 1;
                        JobState::Cancelled
                    }
                };

                JobStatusResponse {
                    job_id: info.job_id.to_string(),
                    status,
                    source: info.source,
                    result: info.result.map(Box::new),
                    error: info.error,
                    created_at: info.created_at,
                    started_at: info.started_at,
                    completed_at: info.completed_at,
                }
            })
            .collect();

        let total = jobs.len();
        Ok(JobListResponse {
            jobs,
            total,
            by_status: counts,
        })
    }

    // =========================================================================
    // Config - Typed
    // =========================================================================

    /// Get config value - typed response
    pub async fn config_get_typed(
        &self,
        section: Option<&str>,
        key: Option<&str>,
    ) -> Result<ConfigValueResponse, ToolError> {
        use hooteconf::HootConfig;

        let config = HootConfig::load()
            .map_err(|e| ToolError::internal(format!("Failed to load config: {}", e)))?;

        // Discover RAVE models for "models" section
        // Models live in ~/.hootenanny/models/rave/ (sibling to cas_dir)
        let hootenanny_dir = config.infra.paths.cas_dir.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| config.infra.paths.cas_dir.clone());

        let discover_rave_models = || -> Vec<String> {
            let rave_dir = hootenanny_dir.join("models").join("rave");
            if rave_dir.exists() {
                std::fs::read_dir(&rave_dir)
                    .map(|entries| {
                        entries
                            .filter_map(|e| e.ok())
                            .filter(|e| e.path().extension().map(|ext| ext == "ts").unwrap_or(false))
                            .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_string()))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        };

        let value = match (section, key) {
            (None, None) => {
                // Return full config as nested object
                let rave_models = discover_rave_models();
                ConfigValue::Object(std::collections::HashMap::from([
                    (
                        "paths".to_string(),
                        ConfigValue::Object(std::collections::HashMap::from([
                            (
                                "state_dir".to_string(),
                                ConfigValue::String(
                                    config.infra.paths.state_dir.display().to_string(),
                                ),
                            ),
                            (
                                "cas_dir".to_string(),
                                ConfigValue::String(
                                    config.infra.paths.cas_dir.display().to_string(),
                                ),
                            ),
                        ])),
                    ),
                    (
                        "bind".to_string(),
                        ConfigValue::Object(std::collections::HashMap::from([(
                            "http_port".to_string(),
                            ConfigValue::Integer(config.infra.bind.http_port as i64),
                        )])),
                    ),
                    (
                        "models".to_string(),
                        ConfigValue::Object(std::collections::HashMap::from([(
                            "rave".to_string(),
                            ConfigValue::Array(rave_models.into_iter().map(ConfigValue::String).collect()),
                        )])),
                    ),
                ]))
            }
            (Some("paths"), None) => ConfigValue::Object(std::collections::HashMap::from([
                (
                    "state_dir".to_string(),
                    ConfigValue::String(config.infra.paths.state_dir.display().to_string()),
                ),
                (
                    "cas_dir".to_string(),
                    ConfigValue::String(config.infra.paths.cas_dir.display().to_string()),
                ),
                (
                    "socket_dir".to_string(),
                    ConfigValue::String(
                        config
                            .infra
                            .paths
                            .socket_dir
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "<not configured>".to_string()),
                    ),
                ),
            ])),
            (Some("paths"), Some("cas_dir")) => {
                ConfigValue::String(config.infra.paths.cas_dir.display().to_string())
            }
            (Some("bind"), Some("http_port")) => {
                ConfigValue::Integer(config.infra.bind.http_port as i64)
            }
            (Some("models"), None) => {
                let rave_models = discover_rave_models();
                ConfigValue::Object(std::collections::HashMap::from([(
                    "rave".to_string(),
                    ConfigValue::Array(rave_models.into_iter().map(ConfigValue::String).collect()),
                )]))
            }
            (Some("models"), Some("rave")) => {
                let rave_models = discover_rave_models();
                ConfigValue::Array(rave_models.into_iter().map(ConfigValue::String).collect())
            }
            _ => {
                return Err(ToolError::validation(
                    "invalid_params",
                    format!("Unknown config path: {:?}/{:?}", section, key),
                ));
            }
        };

        Ok(ConfigValueResponse {
            section: section.map(String::from),
            key: key.map(String::from),
            value,
        })
    }

    // =========================================================================
    // CAS - Typed (Phase 1)
    // =========================================================================

    /// Store content in CAS - typed response
    pub async fn cas_store_typed(
        &self,
        data: &[u8],
        mime_type: &str,
    ) -> Result<hooteproto::responses::CasStoredResponse, ToolError> {
        let hash = self
            .local_models
            .store_cas_content(data, mime_type)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to store in CAS: {}", e)))?;

        Ok(hooteproto::responses::CasStoredResponse {
            hash: hash.to_string(),
            size: data.len(),
            mime_type: mime_type.to_string(),
        })
    }

    /// Get content from CAS - typed response
    pub async fn cas_get_typed(
        &self,
        hash: &str,
    ) -> Result<hooteproto::responses::CasContentResponse, ToolError> {
        let cas_ref = self
            .local_models
            .inspect_cas_content(hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to get CAS content: {}", e)))?;

        let local_path = cas_ref
            .local_path
            .ok_or_else(|| ToolError::not_found("cas_content", hash))?;

        let data = tokio::fs::read(&local_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read CAS file: {}", e)))?;

        Ok(hooteproto::responses::CasContentResponse {
            hash: hash.to_string(),
            size: data.len(),
            data,
        })
    }

    /// Inspect CAS content - typed response
    pub async fn cas_inspect_typed(
        &self,
        hash: &str,
    ) -> Result<hooteproto::responses::CasInspectedResponse, ToolError> {
        let cas_ref = self
            .local_models
            .inspect_cas_content(hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to inspect CAS: {}", e)))?;

        Ok(hooteproto::responses::CasInspectedResponse {
            hash: cas_ref.hash.to_string(),
            exists: true,
            size: Some(cas_ref.size_bytes as usize),
            preview: None, // Could add preview logic if needed
        })
    }

    /// Get CAS storage statistics - typed response
    pub async fn cas_stats_typed(&self) -> Result<hooteproto::responses::CasStatsResponse, ToolError> {
        let cas_dir = self.local_models.cas_base_path();
        let metadata_dir = cas_dir.join("metadata");

        let mut total_items = 0u64;
        let mut total_bytes = 0u64;

        if metadata_dir.exists() {
            for entry in walkdir::WalkDir::new(&metadata_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "json")
                })
            {
                if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&contents) {
                        let size = meta.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                        total_items += 1;
                        total_bytes += size;
                    }
                }
            }
        }

        Ok(hooteproto::responses::CasStatsResponse {
            total_items,
            total_bytes,
            cas_dir: cas_dir.display().to_string(),
        })
    }

    // =========================================================================
    // Artifacts - Typed (Phase 1)
    // =========================================================================

    /// Get artifact by ID - typed response
    pub async fn artifact_get_typed(
        &self,
        id: &str,
    ) -> Result<hooteproto::responses::ArtifactInfoResponse, ToolError> {
        let store = self
            .artifact_store
            .read()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;

        let artifact = store
            .get(id)
            .map_err(|e| ToolError::internal(format!("Failed to get artifact: {}", e)))?
            .ok_or_else(|| ToolError::not_found("artifact", id))?;

        Ok(hooteproto::responses::ArtifactInfoResponse {
            id: artifact.id.as_str().to_string(),
            content_hash: artifact.content_hash.as_str().to_string(),
            mime_type: "application/octet-stream".to_string(), // Would need metadata lookup
            tags: artifact.tags.clone(),
            creator: artifact.creator.clone(),
            created_at: artifact.created_at.timestamp() as u64,
            parent_id: artifact.parent_id.as_ref().map(|p| p.as_str().to_string()),
            variation_set_id: artifact
                .variation_set_id
                .as_ref()
                .map(|v| v.as_str().to_string()),
            metadata: None, // Could extract from artifact.metadata
        })
    }

    /// List artifacts - typed response
    pub async fn artifact_list_typed(
        &self,
        tag: Option<&str>,
        creator: Option<&str>,
    ) -> Result<hooteproto::responses::ArtifactListResponse, ToolError> {
        let store = self
            .artifact_store
            .read()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;

        let all_artifacts = store
            .all()
            .map_err(|e| ToolError::internal(format!("Failed to list artifacts: {}", e)))?;

        let artifacts: Vec<hooteproto::responses::ArtifactInfoResponse> = all_artifacts
            .into_iter()
            .filter(|a| {
                let tag_match = tag.is_none_or(|t| a.tags.iter().any(|at| at == t));
                let creator_match = creator.is_none_or(|c| a.creator.as_str() == c);
                tag_match && creator_match
            })
            .map(|a| hooteproto::responses::ArtifactInfoResponse {
                id: a.id.as_str().to_string(),
                content_hash: a.content_hash.as_str().to_string(),
                mime_type: "application/octet-stream".to_string(),
                tags: a.tags.clone(),
                creator: a.creator.clone(),
                created_at: a.created_at.timestamp() as u64,
                parent_id: a.parent_id.as_ref().map(|p| p.as_str().to_string()),
                variation_set_id: a.variation_set_id.as_ref().map(|v| v.as_str().to_string()),
                metadata: None,
            })
            .collect();

        let count = artifacts.len();
        Ok(hooteproto::responses::ArtifactListResponse { artifacts, count })
    }

    // =========================================================================
    // Graph - Typed (Phase 1)
    // =========================================================================

    /// Find graph identities - typed response
    pub async fn graph_find_typed(
        &self,
        name: Option<&str>,
        tag_namespace: Option<&str>,
        tag_value: Option<&str>,
    ) -> Result<hooteproto::responses::GraphIdentitiesResponse, ToolError> {
        use audio_graph_mcp::graph_find;

        let identities = graph_find(&self.audio_graph_db, name, tag_namespace, tag_value)
            .map_err(|e| ToolError::internal(format!("Graph find failed: {}", e)))?;

        let converted: Vec<hooteproto::responses::GraphIdentityInfo> = identities
            .into_iter()
            .map(|id| hooteproto::responses::GraphIdentityInfo {
                id: id.id.clone(),
                name: id.name.clone(),
                tags: id.tags.iter().map(|t| format!("{}:{}", t.namespace, t.value)).collect(),
            })
            .collect();

        let count = converted.len();
        Ok(hooteproto::responses::GraphIdentitiesResponse {
            identities: converted,
            count,
        })
    }

    /// Get graph context for LLM - typed response
    pub async fn graph_context_typed(
        &self,
        limit: Option<usize>,
        tag: Option<&str>,
        creator: Option<&str>,
        vibe_search: Option<&str>,
        within_minutes: Option<i64>,
        include_annotations: bool,
        include_metadata: bool,
    ) -> Result<hooteproto::responses::GraphContextResponse, ToolError> {
        // Build context string from artifacts
        let store = self
            .artifact_store
            .read()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;

        let all_artifacts = store
            .all()
            .map_err(|e| ToolError::internal(format!("Failed to list artifacts: {}", e)))?;

        let now = chrono::Utc::now();
        let time_threshold = within_minutes.map(|mins| {
            now - chrono::Duration::minutes(mins)
        });

        let filtered: Vec<_> = all_artifacts
            .into_iter()
            .filter(|a| {
                let tag_match = tag.is_none_or(|t| a.tags.iter().any(|at| at == t));
                let creator_match = creator.is_none_or(|c| a.creator.as_str() == c);
                let time_match = time_threshold.is_none_or(|threshold| a.created_at >= threshold);
                let vibe_match = vibe_search.is_none_or(|v| {
                    // Search for tags like "vibe:jazzy" matching the search term
                    a.tags.iter().any(|t| {
                        t.starts_with("vibe:") && t[5..].contains(v)
                    })
                });
                tag_match && creator_match && time_match && vibe_match
            })
            .take(limit.unwrap_or(20))
            .collect();

        let artifact_count = filtered.len();

        // Build context string
        let mut context = String::new();
        for a in &filtered {
            context.push_str(&format!(
                "- {} ({}) [{}]\n",
                a.id.as_str(),
                a.creator,
                a.tags.join(", ")
            ));
            if include_metadata {
                context.push_str(&format!("  metadata: {:?}\n", a.metadata));
            }
            if include_annotations {
                // Would fetch annotations here
            }
        }

        // Count identities
        let identity_count = audio_graph_mcp::graph_find(&self.audio_graph_db, None, None, None)
            .map(|ids| ids.len())
            .unwrap_or(0);

        Ok(hooteproto::responses::GraphContextResponse {
            context,
            artifact_count,
            identity_count,
        })
    }

    /// Execute graph Trustfall query - typed response
    pub async fn graph_query_typed(
        &self,
        query: &str,
        limit: Option<usize>,
        variables: Option<&serde_json::Value>,
    ) -> Result<hooteproto::responses::GraphQueryResultResponse, ToolError> {
        use std::collections::BTreeMap;
        use std::sync::Arc;
        use trustfall::{execute_query, FieldValue};

        // Convert variables
        let vars: BTreeMap<Arc<str>, FieldValue> = variables
            .map(|v| {
                v.as_object()
                    .map(|obj| {
                        obj.iter()
                            .map(|(k, v)| {
                                let key: Arc<str> = Arc::from(k.as_str());
                                let val = match v {
                                    serde_json::Value::String(s) => FieldValue::String(s.clone().into()),
                                    serde_json::Value::Number(n) => {
                                        if let Some(i) = n.as_i64() {
                                            FieldValue::Int64(i)
                                        } else if let Some(f) = n.as_f64() {
                                            FieldValue::Float64(f)
                                        } else {
                                            FieldValue::Null
                                        }
                                    }
                                    serde_json::Value::Bool(b) => FieldValue::Boolean(*b),
                                    _ => FieldValue::Null,
                                };
                                (key, val)
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Use the pre-existing graph adapter from the server
        let results_iter = execute_query(
            self.graph_adapter.schema(),
            Arc::clone(&self.graph_adapter),
            query,
            vars,
        )
        .map_err(|e| ToolError::internal(format!("Query execution failed: {}", e)))?;

        let limit = limit.unwrap_or(100);
        let results: Vec<serde_json::Value> = results_iter
            .take(limit)
            .map(|row| {
                let obj: serde_json::Map<String, serde_json::Value> = row
                    .into_iter()
                    .map(|(k, v)| {
                        let val = match v {
                            FieldValue::String(s) => serde_json::Value::String(s.to_string()),
                            FieldValue::Int64(i) => serde_json::Value::Number(i.into()),
                            FieldValue::Float64(f) => serde_json::json!(f),
                            FieldValue::Boolean(b) => serde_json::Value::Bool(b),
                            FieldValue::Null => serde_json::Value::Null,
                            _ => serde_json::Value::Null,
                        };
                        (k.to_string(), val)
                    })
                    .collect();
                serde_json::Value::Object(obj)
            })
            .collect();

        let count = results.len();
        Ok(hooteproto::responses::GraphQueryResultResponse { results, count })
    }

    // =========================================================================
    // Garden Query - Typed (Phase 1)
    // =========================================================================

    /// Execute garden Trustfall query - typed response
    ///
    /// Queries are now evaluated locally in hootenanny using GardenStateAdapter.
    /// State is fetched from chaosgarden as a snapshot, eliminating JSON/GraphQL
    /// parsing from the real-time audio process.
    pub async fn garden_query_typed(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<hooteproto::responses::GardenQueryResultResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        // Convert variables to HashMap
        let vars: std::collections::HashMap<String, serde_json::Value> = variables
            .and_then(|v| v.as_object())
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        // Fetch snapshot from chaosgarden
        let snapshot = manager
            .get_snapshot()
            .await
            .map_err(|e| ToolError::service("chaosgarden", "snapshot_failed", e.to_string()))?;

        // Execute query locally using the adapter
        let rows = crate::api::garden_adapter::execute_query(snapshot, query, vars)
            .map_err(|e| ToolError::service("trustfall", "query_error", e.to_string()))?;

        let count = rows.len();
        Ok(hooteproto::responses::GardenQueryResultResponse { results: rows, count })
    }

    // =========================================================================
    // Orpheus Classify - Typed (Phase 1)
    // =========================================================================

    /// Classify MIDI content - typed response
    pub async fn orpheus_classify_typed(
        &self,
        midi_hash: &str,
    ) -> Result<hooteproto::responses::OrpheusClassifiedResponse, ToolError> {
        // Get MIDI from CAS
        let cas_ref = self
            .local_models
            .inspect_cas_content(midi_hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to get MIDI from CAS: {}", e)))?;

        let local_path = cas_ref
            .local_path
            .ok_or_else(|| ToolError::not_found("midi", midi_hash))?;

        // Call Orpheus classify service
        let client = reqwest::Client::new();
        let response = client
            .post("http://localhost:2001/classify")
            .json(&serde_json::json!({
                "midi_path": local_path,
            }))
            .send()
            .await
            .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

        if !response.status().is_success() {
            return Err(ToolError::service(
                "orpheus",
                "classify_failed",
                format!("HTTP {}", response.status()),
            ));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ToolError::service("orpheus", "parse_failed", e.to_string()))?;

        // Extract classifications from response
        let classifications: Vec<hooteproto::responses::MidiClassification> = result
            .get("classifications")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some(hooteproto::responses::MidiClassification {
                            label: v.get("label")?.as_str()?.to_string(),
                            confidence: v.get("confidence")?.as_f64()? as f32,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(hooteproto::responses::OrpheusClassifiedResponse { classifications })
    }

    // ============================================================
    // Job Tools (Poll, Cancel, Sleep)
    // ============================================================

    /// Poll for job completion - typed response
    pub async fn job_poll_typed(
        &self,
        job_ids: Vec<String>,
        timeout_ms: u64,
        mode: Option<String>,
    ) -> Result<hooteproto::responses::JobPollResponse, ToolError> {
        use hooteproto::JobStatus;
        use std::time::{Duration, Instant};

        let timeout_ms = timeout_ms.min(30000); // Cap at 30s
        let timeout = Duration::from_millis(timeout_ms);
        let mode = mode.as_deref().unwrap_or("any");

        if mode != "any" && mode != "all" {
            return Err(ToolError::validation(
                "invalid_params",
                format!("mode must be 'any' or 'all', got '{}'", mode),
            ));
        }

        let job_ids: Vec<hooteproto::JobId> =
            job_ids.into_iter().map(hooteproto::JobId::from).collect();

        let start = Instant::now();
        let poll_interval = Duration::from_millis(500);

        loop {
            let mut completed = Vec::new();
            let mut pending = Vec::new();
            let mut failed = Vec::new();

            for job_id in &job_ids {
                match self.job_store.get_job(job_id) {
                    Ok(job_info) => match job_info.status {
                        JobStatus::Complete => completed.push(job_id.as_str().to_string()),
                        JobStatus::Failed | JobStatus::Cancelled => {
                            failed.push(job_id.as_str().to_string())
                        }
                        JobStatus::Pending | JobStatus::Running => {
                            pending.push(job_id.as_str().to_string())
                        }
                    },
                    Err(_) => {
                        failed.push(job_id.as_str().to_string());
                    }
                }
            }

            let elapsed = start.elapsed();
            let elapsed_ms = elapsed.as_millis() as u64;

            let should_return = if job_ids.is_empty() {
                elapsed >= timeout
            } else if mode == "any" {
                !completed.is_empty() || !failed.is_empty()
            } else {
                pending.is_empty()
            };

            let timed_out = elapsed >= timeout;

            if should_return || timed_out {
                let reason = if !completed.is_empty() || !failed.is_empty() {
                    "job_complete".to_string()
                } else {
                    "timeout".to_string()
                };

                return Ok(hooteproto::responses::JobPollResponse {
                    completed,
                    failed,
                    pending,
                    reason,
                    elapsed_ms,
                });
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Cancel a job - typed response
    pub async fn job_cancel_typed(
        &self,
        job_id: &str,
    ) -> Result<hooteproto::responses::JobCancelResponse, ToolError> {
        let job_id = hooteproto::JobId::from(job_id.to_string());

        self.job_store
            .cancel_job(&job_id)
            .map_err(|e| ToolError::internal(e.to_string()))?;

        Ok(hooteproto::responses::JobCancelResponse {
            job_id: job_id.as_str().to_string(),
            cancelled: true,
        })
    }

    // ============================================================
    // Event Polling
    // ============================================================

    /// Poll for buffered broadcast events - typed response
    pub async fn event_poll_typed(
        &self,
        cursor: Option<u64>,
        since_ms: Option<u64>,
        types: Option<Vec<String>>,
        timeout_ms: Option<u64>,
        limit: Option<usize>,
    ) -> Result<hooteproto::responses::EventPollResponse, ToolError> {
        use crate::event_buffer::validate_poll_params;
        use std::time::{Duration, Instant};

        // Validate parameters
        let (timeout, limit) = validate_poll_params(timeout_ms, limit)
            .map_err(|e| ToolError::validation("invalid_params", e.to_string()))?;

        let event_buffer = self.event_buffer.as_ref().ok_or_else(|| {
            ToolError::service("event_poll", "not_configured", "Event buffer not initialized")
        })?;

        let poll_interval = Duration::from_millis(100);
        let timeout_duration = Duration::from_millis(timeout);
        let start = Instant::now();

        // Helper to build snapshot from buffer and job_store
        let build_snapshot = |buffer: &crate::event_buffer::EventBuffer| {
            let job_summary = self.job_store.summary();

            hooteproto::responses::Snapshot {
                transport: buffer.latest_transport().map(|t| {
                    hooteproto::responses::TransportInfo {
                        state: t.state.clone(),
                        position_beats: t.position_beats,
                        tempo_bpm: t.tempo_bpm,
                        timestamp_ms: t.timestamp_ms,
                    }
                }),
                latest_beat: buffer.latest_beat().map(|b| {
                    hooteproto::responses::BeatTickInfo {
                        beat: b.beat,
                        position_beats: b.position_beats,
                        tempo_bpm: b.tempo_bpm,
                        timestamp_ms: b.timestamp_ms,
                    }
                }),
                active_jobs: job_summary,
                device_count: buffer.device_count(),
            }
        };

        loop {
            // Try to get events
            let buffer = event_buffer.read().await;
            let types_ref = types.as_deref();

            let result = buffer.poll(cursor, since_ms, types_ref, limit).map_err(|e| {
                ToolError::validation("poll_error", e.to_string())
            })?;

            // If we have events or no cursor (initial poll), return immediately
            if !result.events.is_empty() || cursor.is_none() {
                let snapshot = build_snapshot(&buffer);

                return Ok(hooteproto::responses::EventPollResponse {
                    events: result.events.into_iter().map(|e| {
                        hooteproto::responses::BufferedEvent {
                            seq: e.seq,
                            timestamp_ms: e.timestamp_ms,
                            event_type: e.event_type,
                            data: e.data,
                        }
                    }).collect(),
                    cursor: result.cursor,
                    has_more: result.has_more,
                    snapshot,
                    buffer: hooteproto::responses::BufferStats {
                        oldest_cursor: result.buffer.oldest_cursor,
                        newest_cursor: result.buffer.newest_cursor,
                        total_events: result.buffer.total_events,
                        capacity: result.buffer.capacity,
                    },
                    server_time_ms: result.server_time_ms,
                });
            }

            drop(buffer); // Release read lock before sleeping

            // Check timeout
            if start.elapsed() >= timeout_duration {
                // Return empty result with current state
                let buffer = event_buffer.read().await;
                let stats = buffer.stats();
                let snapshot = build_snapshot(&buffer);

                return Ok(hooteproto::responses::EventPollResponse {
                    events: vec![],
                    cursor: cursor.unwrap_or(stats.newest_cursor),
                    has_more: false,
                    snapshot,
                    buffer: hooteproto::responses::BufferStats {
                        oldest_cursor: stats.oldest_cursor,
                        newest_cursor: stats.newest_cursor,
                        total_events: stats.total_events,
                        capacity: stats.capacity,
                    },
                    server_time_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                });
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    // ============================================================
    // CAS Upload Tools
    // ============================================================

    /// Upload file to CAS - typed response
    pub async fn cas_upload_file_typed(
        &self,
        file_path: &str,
        mime_type: &str,
    ) -> Result<hooteproto::responses::CasStoredResponse, ToolError> {
        let path = std::path::Path::new(file_path);

        if !path.exists() {
            return Err(ToolError::not_found("file", file_path));
        }

        let data = std::fs::read(path)
            .map_err(|e| ToolError::internal(format!("Failed to read file: {}", e)))?;

        self.cas_store_typed(&data, mime_type).await
    }

    // ============================================================
    // Graph Mutation Tools (Bind, Tag, Connect)
    // ============================================================

    /// Bind an identity to a device - typed response
    pub async fn graph_bind_typed(
        &self,
        id: &str,
        name: &str,
        hints: Vec<hooteproto::request::GraphHint>,
    ) -> Result<hooteproto::responses::GraphBindResponse, ToolError> {
        use audio_graph_mcp::{graph_bind, HintKind};

        let db = self.graph_adapter.db();

        let device_hints: Vec<(HintKind, String, f64)> = hints
            .into_iter()
            .map(|h| {
                let kind = match h.kind.as_str() {
                    "usb_device_id" => HintKind::UsbDeviceId,
                    "usb_serial" => HintKind::UsbSerial,
                    "usb_path" => HintKind::UsbPath,
                    "midi_name" => HintKind::MidiName,
                    "alsa_card" => HintKind::AlsaCard,
                    "alsa_hw" => HintKind::AlsaHw,
                    "pipewire_name" => HintKind::PipewireName,
                    "pipewire_alsa_path" => HintKind::PipewireAlsaPath,
                    _ => HintKind::MidiName, // Default
                };
                (kind, h.value, h.confidence)
            })
            .collect();

        let hints_count = device_hints.len();
        let identity = graph_bind(db, id, name, device_hints)
            .map_err(ToolError::internal)?;

        Ok(hooteproto::responses::GraphBindResponse {
            identity_id: identity.id.0,
            name: identity.name,
            hints_count,
        })
    }

    /// Tag an identity - typed response
    pub async fn graph_tag_typed(
        &self,
        identity_id: &str,
        namespace: &str,
        value: &str,
    ) -> Result<hooteproto::responses::GraphTagResponse, ToolError> {
        use audio_graph_mcp::graph_tag;

        let db = self.graph_adapter.db();

        // Add the single tag
        let _tags = graph_tag(db, identity_id, vec![(namespace.to_string(), value.to_string())], vec![])
            .map_err(ToolError::internal)?;

        Ok(hooteproto::responses::GraphTagResponse {
            identity_id: identity_id.to_string(),
            tag: format!("{}:{}", namespace, value),
        })
    }

    /// Connect two identities - typed response
    pub async fn graph_connect_typed(
        &self,
        from_identity: &str,
        from_port: &str,
        to_identity: &str,
        to_port: &str,
        transport: Option<String>,
    ) -> Result<hooteproto::responses::GraphConnectResponse, ToolError> {
        use audio_graph_mcp::graph_connect;

        let db = self.graph_adapter.db();

        let _conn = graph_connect(db, from_identity, from_port, to_identity, to_port, transport.as_deref())
            .map_err(ToolError::internal)?;

        Ok(hooteproto::responses::GraphConnectResponse {
            from_identity: from_identity.to_string(),
            from_port: from_port.to_string(),
            to_identity: to_identity.to_string(),
            to_port: to_port.to_string(),
        })
    }

    // ============================================================
    // ABC to MIDI Conversion
    // ============================================================

    /// Convert ABC notation to MIDI - typed response
    pub async fn abc_to_midi_typed(
        &self,
        abc: &str,
        tempo_override: Option<u16>,
        transpose: Option<i8>,
        velocity: Option<u8>,
        channel: Option<u8>,
        tags: Vec<String>,
        creator: Option<String>,
    ) -> Result<hooteproto::responses::AbcToMidiResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash};

        // Parse ABC notation
        let mut result = abc::parse(abc);
        if result.has_errors() {
            let errors: Vec<String> = result.feedback.iter()
                .filter(|f| matches!(f.level, abc::FeedbackLevel::Error))
                .map(|f| f.message.clone())
                .collect();
            return Err(ToolError::validation("abc_parse_error", errors.join("; ")));
        }

        // Apply transpose if requested
        if let Some(semitones) = transpose {
            result.value = abc::transpose(&result.value, semitones);
        }

        // Build MIDI params
        let params = abc::MidiParams {
            velocity: velocity.unwrap_or(80),
            ticks_per_beat: 480,
            channel: channel.unwrap_or(0),
            program: None, // Use default (piano) - abc_to_midi doesn't have program param yet
        };

        // Generate MIDI bytes
        let midi_bytes = abc::to_midi(&result.value, &params);

        // Store in CAS
        let cas_result = self.cas_store_typed(&midi_bytes, "audio/midi").await?;

        // Create artifact
        let content_hash = ContentHash::new(&cas_result.hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

        let mut artifact_tags = tags;
        artifact_tags.push("type:midi".to_string());
        artifact_tags.push("source:abc".to_string());

        let metadata = serde_json::json!({
            "mime_type": "audio/midi",
            "source": "abc",
            "tempo_override": tempo_override,
            "transpose": transpose,
        });

        let artifact = Artifact::new(
            artifact_id.clone(),
            content_hash.clone(),
            creator.unwrap_or_else(|| "mcp".to_string()),
            metadata,
        ).with_tags(artifact_tags);

        {
            let mut store = self.artifact_store.write().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.put(artifact).map_err(|e| {
                ToolError::internal(format!("Failed to store artifact: {}", e))
            })?;
        }

        Ok(hooteproto::responses::AbcToMidiResponse {
            artifact_id: artifact_id.as_str().to_string(),
            content_hash: content_hash.as_str().to_string(),
        })
    }

    // ============================================================
    // Artifact Upload
    // ============================================================

    /// Upload file and create artifact - typed response
    pub async fn artifact_upload_typed(
        &self,
        file_path: &str,
        mime_type: &str,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::ArtifactCreatedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};

        // First upload to CAS
        let cas_result = self.cas_upload_file_typed(file_path, mime_type).await?;

        // Create artifact
        let content_hash = ContentHash::new(&cas_result.hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
        let creator_str = creator.clone().unwrap_or_else(|| "mcp".to_string());

        let metadata = serde_json::json!({
            "mime_type": mime_type,
            "source_path": file_path,
        });

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            content_hash.clone(),
            &creator_str,
            metadata,
        ).with_tags(tags.clone());

        // Set optional parent and variation set
        if let Some(ref parent) = parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent.clone()));
        }
        if let Some(ref var_set) = variation_set_id {
            artifact.variation_set_id = Some(VariationSetId::new(var_set.clone()));
        }

        {
            let mut store = self.artifact_store.write().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.put(artifact).map_err(|e| {
                ToolError::internal(format!("Failed to store artifact: {}", e))
            })?;
        }

        Ok(hooteproto::responses::ArtifactCreatedResponse {
            artifact_id: artifact_id.as_str().to_string(),
            content_hash: content_hash.as_str().to_string(),
            tags,
            creator: creator_str,
        })
    }

    // ============================================================
    // Annotations
    // ============================================================

    /// Add annotation to artifact - typed response
    pub async fn add_annotation_typed(
        &self,
        artifact_id: &str,
        message: &str,
        source: Option<String>,
        vibe: Option<String>,
    ) -> Result<hooteproto::responses::AnnotationAddedResponse, ToolError> {
        use audio_graph_mcp::sources::{ArtifactSource, AnnotationData};

        // Create annotation
        let annotation = AnnotationData::new(
            artifact_id.to_string(),
            message.to_string(),
            vibe,
            source.unwrap_or_else(|| "mcp".to_string()),
        );
        let annotation_id = annotation.id.clone();

        // Add to store
        {
            let store = self.artifact_store.read().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.add_annotation(annotation).map_err(|e| {
                ToolError::internal(format!("Failed to add annotation: {}", e))
            })?;
        }

        Ok(hooteproto::responses::AnnotationAddedResponse {
            artifact_id: artifact_id.to_string(),
            annotation_id,
        })
    }

    // ============================================================
    // MIDI to WAV Conversion
    // ============================================================

    /// Convert MIDI to WAV using rustysynth - typed response
    pub async fn midi_to_wav_typed(
        &self,
        input_hash: &str,
        soundfont_hash: &str,
        sample_rate: Option<u32>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::MidiToWavResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::mcp_tools::rustysynth::render_midi_to_wav;
        use crate::types::{ArtifactId, ContentHash, VariationSetId};

        let sample_rate = sample_rate.unwrap_or(44100);

        // Get MIDI content from CAS
        let midi_cas = self
            .local_models
            .inspect_cas_content(input_hash)
            .await
            .map_err(|e| ToolError::not_found("midi", e.to_string()))?;

        let midi_path = midi_cas
            .local_path
            .ok_or_else(|| ToolError::not_found("midi", input_hash))?;

        let midi_bytes = std::fs::read(&midi_path)
            .map_err(|e| ToolError::internal(format!("Failed to read MIDI: {}", e)))?;

        // Get SoundFont content from CAS
        let sf_cas = self
            .local_models
            .inspect_cas_content(soundfont_hash)
            .await
            .map_err(|e| ToolError::not_found("soundfont", e.to_string()))?;

        let sf_path = sf_cas
            .local_path
            .ok_or_else(|| ToolError::not_found("soundfont", soundfont_hash))?;

        let sf_bytes = std::fs::read(&sf_path)
            .map_err(|e| ToolError::internal(format!("Failed to read soundfont: {}", e)))?;

        // Render MIDI to WAV
        let wav_bytes = render_midi_to_wav(&midi_bytes, &sf_bytes, sample_rate)
            .map_err(|e| ToolError::internal(format!("Render failed: {}", e)))?;

        // Calculate duration
        let duration_secs = Some((wav_bytes.len() as f64) / (sample_rate as f64 * 2.0 * 2.0)); // stereo, 16-bit

        // Store WAV in CAS
        let cas_result = self.cas_store_typed(&wav_bytes, "audio/wav").await?;

        // Create artifact
        let content_hash = ContentHash::new(&cas_result.hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
        let creator_str = creator.clone().unwrap_or_else(|| "mcp".to_string());

        let mut artifact_tags = tags;
        artifact_tags.push("type:audio".to_string());
        artifact_tags.push("source:render".to_string());

        let metadata = serde_json::json!({
            "mime_type": "audio/wav",
            "source": "midi_render",
            "sample_rate": sample_rate,
            "midi_hash": input_hash,
            "soundfont_hash": soundfont_hash,
        });

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            content_hash.clone(),
            &creator_str,
            metadata,
        ).with_tags(artifact_tags);

        if let Some(ref parent) = parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent.clone()));
        }
        if let Some(ref var_set) = variation_set_id {
            artifact.variation_set_id = Some(VariationSetId::new(var_set.clone()));
        }

        {
            let mut store = self.artifact_store.write().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.put(artifact).map_err(|e| {
                ToolError::internal(format!("Failed to store artifact: {}", e))
            })?;
        }

        Ok(hooteproto::responses::MidiToWavResponse {
            artifact_id: artifact_id.as_str().to_string(),
            content_hash: content_hash.as_str().to_string(),
            sample_rate,
            duration_secs,
        })
    }

    // ============================================================
    // Orpheus Generation Tools
    // ============================================================

    /// Generate MIDI from scratch using Orpheus - typed response
    pub async fn orpheus_generate_typed(
        &self,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        model: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::OrpheusGeneratedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};

        let client = reqwest::Client::new();
        let response = client
            .post("http://localhost:2000/predict")
            .json(&serde_json::json!({
                "max_tokens": max_tokens.unwrap_or(512),
                "num_variations": num_variations.unwrap_or(1),
                "temperature": temperature.unwrap_or(1.0),
                "top_p": top_p.unwrap_or(0.95),
                "model": model,
            }))
            .send()
            .await
            .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

        if !response.status().is_success() {
            return Err(ToolError::service(
                "orpheus",
                "generate_failed",
                format!("HTTP {}", response.status()),
            ));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ToolError::service("orpheus", "parse_failed", e.to_string()))?;

        // Parse variations array from response
        let variations = result
            .get("variations")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::service("orpheus", "invalid_response", "Missing variations array"))?;

        // Generate variation set ID if multiple variations
        let var_set_id = if variations.len() > 1 {
            variation_set_id.clone().or_else(|| Some(VariationSetId::new(uuid::Uuid::new_v4().to_string()).as_str().to_string()))
        } else {
            variation_set_id.clone()
        };

        // Store each variation as artifact
        let mut output_hashes = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut tokens_per_variation = Vec::new();
        let mut total_tokens: u64 = 0;
        let creator_str = creator.clone().unwrap_or_else(|| "orpheus".to_string());

        for (idx, variation) in variations.iter().enumerate() {
            // Decode base64 MIDI
            let midi_base64 = variation
                .get("midi_base64")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::service("orpheus", "invalid_response", "Missing midi_base64"))?;

            use base64::Engine;
            let midi_bytes = base64::engine::general_purpose::STANDARD
                .decode(midi_base64)
                .map_err(|e| ToolError::service("orpheus", "decode_failed", e.to_string()))?;

            let num_tokens = variation
                .get("num_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            tokens_per_variation.push(num_tokens);
            total_tokens += num_tokens;

            // Store in CAS
            let cas_result = self.cas_store_typed(&midi_bytes, "audio/midi").await?;
            let content_hash = ContentHash::new(&cas_result.hash);
            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

            output_hashes.push(cas_result.hash.clone());
            artifact_ids.push(artifact_id.as_str().to_string());

            // Create artifact
            let mut artifact_tags = tags.clone();
            artifact_tags.push("type:midi".to_string());
            artifact_tags.push("source:orpheus".to_string());

            let metadata = serde_json::json!({
                "mime_type": "audio/midi",
                "source": "orpheus_generate",
                "variation_index": idx,
                "tokens": num_tokens,
            });

            let mut artifact = Artifact::new(
                artifact_id.clone(),
                content_hash,
                &creator_str,
                metadata,
            ).with_tags(artifact_tags);

            if let Some(ref parent) = parent_id {
                artifact = artifact.with_parent(ArtifactId::new(parent.clone()));
            }
            if let Some(ref var_set) = var_set_id {
                artifact.variation_set_id = Some(VariationSetId::new(var_set.clone()));
                artifact.variation_index = Some(idx as u32);
            }

            let mut store = self.artifact_store.write().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.put(artifact).map_err(|e| {
                ToolError::internal(format!("Failed to store artifact: {}", e))
            })?;
        }

        let summary = format!(
            "Generated {} MIDI variation(s), {} total tokens",
            artifact_ids.len(),
            total_tokens
        );

        Ok(hooteproto::responses::OrpheusGeneratedResponse {
            output_hashes,
            artifact_ids,
            tokens_per_variation,
            total_tokens,
            variation_set_id: var_set_id,
            summary,
        })
    }

    /// Generate MIDI from seed using Orpheus - typed response
    pub async fn orpheus_generate_seeded_typed(
        &self,
        seed_hash: &str,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        model: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::OrpheusGeneratedResponse, ToolError> {
        // Get seed MIDI from CAS
        let seed_cas = self
            .local_models
            .inspect_cas_content(seed_hash)
            .await
            .map_err(|e| ToolError::not_found("seed_midi", e.to_string()))?;

        let seed_path = seed_cas
            .local_path
            .ok_or_else(|| ToolError::not_found("seed_midi", seed_hash))?;

        // Generate from seed
        self.call_orpheus_generate_service(
            "generate_seeded",
            &seed_path,
            max_tokens,
            num_variations,
            temperature,
            top_p,
            model,
            tags,
            creator,
            parent_id.or(Some(seed_hash.to_string())),
            variation_set_id,
        ).await
    }

    /// Continue existing MIDI using Orpheus - typed response
    pub async fn orpheus_continue_typed(
        &self,
        input_hash: &str,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        model: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::OrpheusGeneratedResponse, ToolError> {
        // Get input MIDI from CAS
        let input_cas = self
            .local_models
            .inspect_cas_content(input_hash)
            .await
            .map_err(|e| ToolError::not_found("input_midi", e.to_string()))?;

        let input_path = input_cas
            .local_path
            .ok_or_else(|| ToolError::not_found("input_midi", input_hash))?;

        // Continue from input
        self.call_orpheus_generate_service(
            "continue",
            &input_path,
            max_tokens,
            num_variations,
            temperature,
            top_p,
            model,
            tags,
            creator,
            parent_id.or(Some(input_hash.to_string())),
            variation_set_id,
        ).await
    }

    /// Generate bridge between sections using Orpheus - typed response
    pub async fn orpheus_bridge_typed(
        &self,
        section_a_hash: &str,
        section_b_hash: Option<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        _model: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::OrpheusGeneratedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use base64::Engine;

        // Get section A from CAS and encode as base64
        let section_a_cas = self
            .local_models
            .inspect_cas_content(section_a_hash)
            .await
            .map_err(|e| ToolError::not_found("section_a", e.to_string()))?;

        let section_a_path = section_a_cas
            .local_path
            .ok_or_else(|| ToolError::not_found("section_a", section_a_hash))?;

        let section_a_bytes = std::fs::read(&section_a_path)
            .map_err(|e| ToolError::internal(format!("Failed to read section_a: {}", e)))?;
        let section_a_base64 = base64::engine::general_purpose::STANDARD.encode(&section_a_bytes);

        // Get section B if provided
        let section_b_base64 = if let Some(ref hash) = section_b_hash {
            let cas = self.local_models.inspect_cas_content(hash).await
                .map_err(|e| ToolError::not_found("section_b", e.to_string()))?;
            if let Some(path) = cas.local_path {
                let bytes = std::fs::read(&path)
                    .map_err(|e| ToolError::internal(format!("Failed to read section_b: {}", e)))?;
                Some(base64::engine::general_purpose::STANDARD.encode(&bytes))
            } else {
                None
            }
        } else {
            None
        };

        // Call bridge service with base64 data
        let client = reqwest::Client::new();
        let response = client
            .post("http://localhost:2002/predict")
            .json(&serde_json::json!({
                "section_a": section_a_base64,
                "section_b": section_b_base64,
                "max_tokens": max_tokens.unwrap_or(256),
                "temperature": temperature.unwrap_or(1.0),
                "top_p": top_p.unwrap_or(0.95),
            }))
            .send()
            .await
            .map_err(|e| ToolError::service("orpheus_bridge", "request_failed", e.to_string()))?;

        if !response.status().is_success() {
            return Err(ToolError::service(
                "orpheus_bridge",
                "bridge_failed",
                format!("HTTP {}", response.status()),
            ));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ToolError::service("orpheus_bridge", "parse_failed", e.to_string()))?;

        // Parse variations array
        let variations = result
            .get("variations")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::service("orpheus_bridge", "invalid_response", "Missing variations array"))?;

        let mut output_hashes = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut tokens_per_variation = Vec::new();
        let mut total_tokens: u64 = 0;
        let creator_str = creator.unwrap_or_else(|| "orpheus_bridge".to_string());

        for (idx, variation) in variations.iter().enumerate() {
            let midi_base64 = variation
                .get("midi_base64")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::service("orpheus_bridge", "invalid_response", "Missing midi_base64"))?;

            let midi_bytes = base64::engine::general_purpose::STANDARD
                .decode(midi_base64)
                .map_err(|e| ToolError::service("orpheus_bridge", "decode_failed", e.to_string()))?;

            let num_tokens = variation
                .get("num_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            tokens_per_variation.push(num_tokens);
            total_tokens += num_tokens;

            let cas_result = self.cas_store_typed(&midi_bytes, "audio/midi").await?;
            let content_hash = ContentHash::new(&cas_result.hash);
            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

            output_hashes.push(cas_result.hash.clone());
            artifact_ids.push(artifact_id.as_str().to_string());

            let mut artifact_tags = tags.clone();
            artifact_tags.push("type:midi".to_string());
            artifact_tags.push("source:orpheus_bridge".to_string());

            let metadata = serde_json::json!({
                "mime_type": "audio/midi",
                "source": "orpheus_bridge",
                "section_a_hash": section_a_hash,
                "section_b_hash": section_b_hash,
                "tokens": num_tokens,
                "variation_index": idx,
            });

            let mut artifact = Artifact::new(
                artifact_id.clone(),
                content_hash,
                &creator_str,
                metadata,
            ).with_tags(artifact_tags);

            if let Some(ref parent) = parent_id {
                artifact = artifact.with_parent(ArtifactId::new(parent.clone()));
            }
            if let Some(ref var_set) = variation_set_id {
                artifact.variation_set_id = Some(VariationSetId::new(var_set.clone()));
                artifact.variation_index = Some(idx as u32);
            }

            let mut store = self.artifact_store.write().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.put(artifact).map_err(|e| {
                ToolError::internal(format!("Failed to store artifact: {}", e))
            })?;
        }

        let summary = format!("Generated {} bridge variation(s), {} total tokens", artifact_ids.len(), total_tokens);

        Ok(hooteproto::responses::OrpheusGeneratedResponse {
            output_hashes,
            artifact_ids,
            tokens_per_variation,
            total_tokens,
            variation_set_id,
            summary,
        })
    }

    /// Generate loopable MIDI using Orpheus loops model - typed response
    pub async fn orpheus_loops_typed(
        &self,
        seed_hash: Option<String>,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::OrpheusGeneratedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use base64::Engine;

        // Get seed MIDI and encode as base64 if provided
        let seed_base64 = if let Some(ref hash) = seed_hash {
            let cas = self.local_models.inspect_cas_content(hash).await
                .map_err(|e| ToolError::not_found("seed", e.to_string()))?;
            if let Some(path) = cas.local_path {
                let bytes = std::fs::read(&path)
                    .map_err(|e| ToolError::internal(format!("Failed to read seed: {}", e)))?;
                Some(base64::engine::general_purpose::STANDARD.encode(&bytes))
            } else {
                None
            }
        } else {
            None
        };

        // Call loops service with base64 data
        let client = reqwest::Client::new();
        let response = client
            .post("http://localhost:2003/predict")
            .json(&serde_json::json!({
                "seed_midi": seed_base64,
                "max_tokens": max_tokens.unwrap_or(512),
                "num_variations": num_variations.unwrap_or(1),
                "temperature": temperature.unwrap_or(1.0),
                "top_p": top_p.unwrap_or(0.95),
            }))
            .send()
            .await
            .map_err(|e| ToolError::service("orpheus_loops", "request_failed", e.to_string()))?;

        if !response.status().is_success() {
            return Err(ToolError::service(
                "orpheus_loops",
                "loops_failed",
                format!("HTTP {}", response.status()),
            ));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ToolError::service("orpheus_loops", "parse_failed", e.to_string()))?;

        // Parse variations array
        let variations = result
            .get("variations")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::service("orpheus_loops", "invalid_response", "Missing variations array"))?;

        // Generate variation set ID if multiple variations
        let var_set_id = if variations.len() > 1 {
            variation_set_id.clone().or_else(|| Some(VariationSetId::new(uuid::Uuid::new_v4().to_string()).as_str().to_string()))
        } else {
            variation_set_id.clone()
        };

        // Store each variation as artifact
        let mut output_hashes = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut tokens_per_variation = Vec::new();
        let mut total_tokens: u64 = 0;
        let creator_str = creator.clone().unwrap_or_else(|| "orpheus_loops".to_string());

        for (idx, variation) in variations.iter().enumerate() {
            let midi_base64 = variation
                .get("midi_base64")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::service("orpheus_loops", "invalid_response", "Missing midi_base64"))?;

            let midi_bytes = base64::engine::general_purpose::STANDARD
                .decode(midi_base64)
                .map_err(|e| ToolError::service("orpheus_loops", "decode_failed", e.to_string()))?;

            let num_tokens = variation
                .get("num_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            tokens_per_variation.push(num_tokens);
            total_tokens += num_tokens;

            let cas_result = self.cas_store_typed(&midi_bytes, "audio/midi").await?;
            let content_hash = ContentHash::new(&cas_result.hash);
            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

            output_hashes.push(cas_result.hash.clone());
            artifact_ids.push(artifact_id.as_str().to_string());

            let mut artifact_tags = tags.clone();
            artifact_tags.push("type:midi".to_string());
            artifact_tags.push("source:orpheus_loops".to_string());
            artifact_tags.push("loopable:true".to_string());

            let metadata = serde_json::json!({
                "mime_type": "audio/midi",
                "source": "orpheus_loops",
                "variation_index": idx,
                "tokens": num_tokens,
            });

            let mut artifact = Artifact::new(
                artifact_id.clone(),
                content_hash,
                &creator_str,
                metadata,
            ).with_tags(artifact_tags);

            if let Some(ref parent) = parent_id.clone().or(seed_hash.clone()) {
                artifact = artifact.with_parent(ArtifactId::new(parent.clone()));
            }
            if let Some(ref var_set) = var_set_id {
                artifact.variation_set_id = Some(VariationSetId::new(var_set.clone()));
                artifact.variation_index = Some(idx as u32);
            }

            let mut store = self.artifact_store.write().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.put(artifact).map_err(|e| {
                ToolError::internal(format!("Failed to store artifact: {}", e))
            })?;
        }

        let summary = format!(
            "Generated {} loopable MIDI variation(s), {} total tokens",
            artifact_ids.len(),
            total_tokens
        );

        Ok(hooteproto::responses::OrpheusGeneratedResponse {
            output_hashes,
            artifact_ids,
            tokens_per_variation,
            total_tokens,
            variation_set_id: var_set_id,
            summary,
        })
    }

    // Helper for seeded/continue generation
    /// Helper for seeded/continue generation - uses task and midi_input (base64)
    async fn call_orpheus_generate_service(
        &self,
        task: &str,  // "generate_seeded" or "continue"
        midi_input_path: &str,  // Path to read MIDI from
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        _model: Option<String>,  // Not used by service currently
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::OrpheusGeneratedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use base64::Engine;

        // Read and encode the input MIDI
        let midi_bytes = std::fs::read(midi_input_path)
            .map_err(|e| ToolError::internal(format!("Failed to read input MIDI: {}", e)))?;
        let midi_base64 = base64::engine::general_purpose::STANDARD.encode(&midi_bytes);

        let client = reqwest::Client::new();
        let request_body = serde_json::json!({
            "task": task,
            "midi_input": midi_base64,
            "max_tokens": max_tokens.unwrap_or(512),
            "num_variations": num_variations.unwrap_or(1),
            "temperature": temperature.unwrap_or(1.0),
            "top_p": top_p.unwrap_or(0.95),
        });

        let response = client
            .post("http://localhost:2000/predict")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

        if !response.status().is_success() {
            return Err(ToolError::service(
                "orpheus",
                "generate_failed",
                format!("HTTP {}", response.status()),
            ));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ToolError::service("orpheus", "parse_failed", e.to_string()))?;

        // Parse variations array
        let variations = result
            .get("variations")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ToolError::service("orpheus", "invalid_response", "Missing variations array"))?;

        let var_set_id = if variations.len() > 1 {
            variation_set_id.clone().or_else(|| Some(VariationSetId::new(uuid::Uuid::new_v4().to_string()).as_str().to_string()))
        } else {
            variation_set_id.clone()
        };

        let mut output_hashes = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut tokens_per_variation = Vec::new();
        let mut total_tokens: u64 = 0;
        let creator_str = creator.clone().unwrap_or_else(|| "orpheus".to_string());

        for (idx, variation) in variations.iter().enumerate() {
            // Decode base64 MIDI
            let midi_base64 = variation
                .get("midi_base64")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::service("orpheus", "invalid_response", "Missing midi_base64"))?;

            let midi_bytes = base64::engine::general_purpose::STANDARD
                .decode(midi_base64)
                .map_err(|e| ToolError::service("orpheus", "decode_failed", e.to_string()))?;

            let num_tokens = variation
                .get("num_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            tokens_per_variation.push(num_tokens);
            total_tokens += num_tokens;

            let cas_result = self.cas_store_typed(&midi_bytes, "audio/midi").await?;
            let content_hash = ContentHash::new(&cas_result.hash);
            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

            output_hashes.push(cas_result.hash.clone());
            artifact_ids.push(artifact_id.as_str().to_string());

            let mut artifact_tags = tags.clone();
            artifact_tags.push("type:midi".to_string());
            artifact_tags.push("source:orpheus".to_string());

            let metadata = serde_json::json!({
                "mime_type": "audio/midi",
                "source": format!("orpheus_{}", task),
                "variation_index": idx,
                "tokens": num_tokens,
            });

            let mut artifact = Artifact::new(
                artifact_id.clone(),
                content_hash,
                &creator_str,
                metadata,
            ).with_tags(artifact_tags);

            if let Some(ref parent) = parent_id {
                artifact = artifact.with_parent(ArtifactId::new(parent.clone()));
            }
            if let Some(ref var_set) = var_set_id {
                artifact.variation_set_id = Some(VariationSetId::new(var_set.clone()));
                artifact.variation_index = Some(idx as u32);
            }

            let mut store = self.artifact_store.write().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.put(artifact).map_err(|e| {
                ToolError::internal(format!("Failed to store artifact: {}", e))
            })?;
        }

        let summary = format!(
            "Generated {} MIDI variation(s), {} total tokens",
            artifact_ids.len(),
            total_tokens
        );

        Ok(hooteproto::responses::OrpheusGeneratedResponse {
            output_hashes,
            artifact_ids,
            tokens_per_variation,
            total_tokens,
            variation_set_id: var_set_id,
            summary,
        })
    }

    // ============================================================
    // AsyncLong Tools - Background job spawning
    // ============================================================

    /// Generate audio with MusicGen - spawns background job
    pub async fn musicgen_generate_typed(
        &self,
        prompt: Option<String>,
        duration: Option<f32>,
        temperature: Option<f32>,
        top_k: Option<u32>,
        top_p: Option<f32>,
        guidance_scale: Option<f32>,
        do_sample: Option<bool>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};

        let job_id = self.job_store.create_job("musicgen_generate".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let prompt_str = prompt.unwrap_or_else(|| "ambient electronic music".to_string());
        let duration_val = duration.unwrap_or(8.0);
        let temperature_val = temperature.unwrap_or(1.0);
        let top_k_val = top_k.unwrap_or(250);
        let top_p_val = top_p.unwrap_or(0.0);
        let guidance_scale_val = guidance_scale.unwrap_or(3.0);
        let do_sample_val = do_sample.unwrap_or(true);

        tokio::spawn(async move {
            let result: anyhow::Result<hooteproto::responses::ToolResponse> = (async {
                let response = local_models
                    .run_musicgen_generate(
                        prompt_str.clone(),
                        duration_val,
                        temperature_val,
                        top_k_val,
                        top_p_val,
                        guidance_scale_val,
                        do_sample_val,
                        Some(job_id_clone.as_str().to_string()),
                    )
                    .await?;

                // Decode audio_base64 from response
                use base64::Engine;
                let audio_base64 = response
                    .get("audio_base64")
                    .and_then(|p| p.as_str())
                    .ok_or_else(|| anyhow::anyhow!("No audio_base64 in MusicGen response"))?;

                let audio_bytes = base64::engine::general_purpose::STANDARD
                    .decode(audio_base64)
                    .map_err(|e| anyhow::anyhow!("Failed to decode audio_base64: {}", e))?;

                let sample_rate = response.get("sample_rate").and_then(|s| s.as_u64()).unwrap_or(32000) as u32;
                let duration_seconds = response.get("duration").and_then(|d| d.as_f64()).unwrap_or(duration_val as f64);
                let hash = local_models.store_cas_content(&audio_bytes, "audio/wav").await?;
                let content_hash = ContentHash::new(&hash);
                let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

                let creator_str = creator.unwrap_or_else(|| "musicgen".to_string());
                let mut artifact_tags = tags;
                artifact_tags.push("type:audio".to_string());
                artifact_tags.push("source:musicgen".to_string());

                let metadata = serde_json::json!({
                    "mime_type": "audio/wav",
                    "source": "musicgen",
                    "prompt": prompt_str,
                    "duration_seconds": duration_seconds,
                    "sample_rate": sample_rate,
                });

                let mut artifact = Artifact::new(
                    artifact_id.clone(),
                    content_hash,
                    &creator_str,
                    metadata,
                ).with_tags(artifact_tags);

                if let Some(ref parent) = parent_id {
                    artifact = artifact.with_parent(ArtifactId::new(parent.clone()));
                }
                if let Some(ref var_set) = variation_set_id {
                    artifact.variation_set_id = Some(VariationSetId::new(var_set.clone()));
                }

                let mut store = artifact_store.write().map_err(|_| anyhow::anyhow!("Artifact store lock poisoned"))?;
                store.put(artifact)?;

                Ok(hooteproto::responses::ToolResponse::AudioGenerated(
                    hooteproto::responses::AudioGeneratedResponse {
                        artifact_id: artifact_id.as_str().to_string(),
                        content_hash: hash,
                        duration_seconds,
                        sample_rate,
                        format: hooteproto::responses::AudioFormat::Wav,
                        genre: None,
                    },
                ))
            })
            .await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, response);
                }
                Err(e) => {
                    tracing::error!(error = %e, "MusicGen generation failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "musicgen_generate".to_string(),
        })
    }

    /// Generate song with YuE - spawns background job
    pub async fn yue_generate_typed(
        &self,
        lyrics: String,
        genre: Option<String>,
        max_new_tokens: Option<u32>,
        run_n_segments: Option<u32>,
        seed: Option<u64>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};

        let job_id = self.job_store.create_job("yue_generate".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let genre_str = genre.clone().unwrap_or_else(|| "pop".to_string());
        let max_tokens = max_new_tokens.unwrap_or(3000);
        let segments = run_n_segments.unwrap_or(2);
        let seed_val = seed.unwrap_or(0);

        tokio::spawn(async move {
            let result: anyhow::Result<hooteproto::responses::ToolResponse> = (async {
                let response = local_models
                    .run_yue_generate(
                        lyrics.clone(),
                        genre_str.clone(),
                        max_tokens,
                        segments,
                        seed_val,
                        Some(job_id_clone.as_str().to_string()),
                    )
                    .await?;

                // Decode audio_base64 from response
                use base64::Engine;
                let audio_base64 = response
                    .get("audio_base64")
                    .and_then(|p| p.as_str())
                    .ok_or_else(|| anyhow::anyhow!("No audio_base64 in YuE response"))?;

                let audio_bytes = base64::engine::general_purpose::STANDARD
                    .decode(audio_base64)
                    .map_err(|e| anyhow::anyhow!("Failed to decode audio_base64: {}", e))?;

                let sample_rate = response.get("sample_rate").and_then(|s| s.as_u64()).unwrap_or(44100) as u32;
                let duration_seconds = response.get("duration").and_then(|d| d.as_f64()).unwrap_or(60.0);
                let hash = local_models.store_cas_content(&audio_bytes, "audio/wav").await?;
                let content_hash = ContentHash::new(&hash);
                let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

                let creator_str = creator.unwrap_or_else(|| "yue".to_string());
                let mut artifact_tags = tags;
                artifact_tags.push("type:audio".to_string());
                artifact_tags.push("source:yue".to_string());
                artifact_tags.push(format!("genre:{}", genre_str));

                let metadata = serde_json::json!({
                    "mime_type": "audio/wav",
                    "source": "yue",
                    "lyrics": lyrics,
                    "genre": genre_str,
                    "duration_seconds": duration_seconds,
                    "sample_rate": sample_rate,
                });

                let mut artifact = Artifact::new(
                    artifact_id.clone(),
                    content_hash,
                    &creator_str,
                    metadata,
                ).with_tags(artifact_tags);

                if let Some(ref parent) = parent_id {
                    artifact = artifact.with_parent(ArtifactId::new(parent.clone()));
                }
                if let Some(ref var_set) = variation_set_id {
                    artifact.variation_set_id = Some(VariationSetId::new(var_set.clone()));
                }

                let mut store = artifact_store.write().map_err(|_| anyhow::anyhow!("Artifact store lock poisoned"))?;
                store.put(artifact)?;

                Ok(hooteproto::responses::ToolResponse::AudioGenerated(
                    hooteproto::responses::AudioGeneratedResponse {
                        artifact_id: artifact_id.as_str().to_string(),
                        content_hash: hash,
                        duration_seconds,
                        sample_rate,
                        format: hooteproto::responses::AudioFormat::Wav,
                        genre: Some(genre_str),
                    },
                ))
            })
            .await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, response);
                }
                Err(e) => {
                    tracing::error!(error = %e, "YuE generation failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "yue_generate".to_string(),
        })
    }

    /// Analyze audio with BeatThis - spawns background job
    pub async fn beatthis_analyze_typed(
        &self,
        audio_hash: Option<String>,
        audio_path: Option<String>,
        _include_frames: bool,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use crate::api::schema::BeatThisServiceRequest;
        use crate::api::tools::beat_this::prepare_audio_for_beatthis;

        let job_id = self.job_store.create_job("beatthis_analyze".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let local_models = Arc::clone(&self.local_models);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        // Get audio bytes - need to do this before spawn since we need async local_models
        let audio_bytes = if let Some(ref hash) = audio_hash {
            let content = self.local_models
                .inspect_cas_content(hash)
                .await
                .map_err(|e| ToolError::not_found("audio", e.to_string()))?;
            let path = content.local_path.ok_or_else(|| ToolError::not_found("audio", hash.clone()))?;
            std::fs::read(&path).map_err(|e| ToolError::internal(format!("Failed to read audio: {}", e)))?
        } else if let Some(ref path) = audio_path {
            std::fs::read(path).map_err(|e| ToolError::internal(format!("Failed to read audio: {}", e)))?
        } else {
            return Err(ToolError::validation("invalid_params", "Either audio_hash or audio_path required"));
        };

        tokio::spawn(async move {
            let result: anyhow::Result<hooteproto::responses::ToolResponse> = (async {
                // Prepare audio for BeatThis (mono 22050 Hz)
                let prepared_audio = prepare_audio_for_beatthis(&audio_bytes)
                    .map_err(|e| anyhow::anyhow!("Audio preparation failed: {}", e.message()))?;

                // Encode as base64
                use base64::Engine;
                let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&prepared_audio);

                let service_request = BeatThisServiceRequest {
                    audio: audio_base64,
                    client_job_id: Some(job_id_clone.as_str().to_string()),
                };

                let response = reqwest::Client::new()
                    .post("http://127.0.0.1:2012/predict")
                    .json(&service_request)
                    .timeout(std::time::Duration::from_secs(120))
                    .send()
                    .await?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error = response.text().await.unwrap_or_default();
                    anyhow::bail!("BeatThis error {}: {}", status, error);
                }

                let beat_response: crate::api::schema::BeatThisServiceResponse = response.json().await?;

                // Compute confidence from detection ratio (heuristic)
                let confidence = if beat_response.duration > 0.0 {
                    let expected_beats = beat_response.duration * beat_response.bpm / 60.0;
                    let ratio = beat_response.num_beats as f64 / expected_beats;
                    (1.0 - (ratio - 1.0).abs().min(1.0)) as f32
                } else {
                    0.5
                };

                Ok(hooteproto::responses::ToolResponse::BeatsAnalyzed(
                    hooteproto::responses::BeatsAnalyzedResponse {
                        beats: beat_response.beats,
                        downbeats: beat_response.downbeats,
                        estimated_bpm: beat_response.bpm,
                        confidence,
                    },
                ))
            })
            .await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, response);
                }
                Err(e) => {
                    tracing::error!(error = %e, "BeatThis analysis failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "beatthis_analyze".to_string(),
        })
    }

    /// Analyze audio with CLAP - spawns background job
    pub async fn clap_analyze_typed(
        &self,
        audio_hash: String,
        audio_b_hash: Option<String>,
        tasks: Vec<String>,
        text_candidates: Vec<String>,
        _creator: Option<String>,
        _parent_id: Option<String>,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use base64::Engine;

        let job_id = self.job_store.create_job("clap_analyze".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let local_models = Arc::clone(&self.local_models);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        // Get audio content and encode as base64 before spawn
        let content = self.local_models
            .inspect_cas_content(&audio_hash)
            .await
            .map_err(|e| ToolError::not_found("audio", e.to_string()))?;
        let path = content.local_path.ok_or_else(|| ToolError::not_found("audio", audio_hash.clone()))?;
        let audio_bytes = std::fs::read(&path).map_err(|e| ToolError::internal(format!("Failed to read audio: {}", e)))?;
        let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

        // Optionally get audio_b content
        let audio_b_base64 = if let Some(ref hash) = audio_b_hash {
            let content_b = self.local_models
                .inspect_cas_content(hash)
                .await
                .map_err(|e| ToolError::not_found("audio_b", e.to_string()))?;
            if let Some(path_b) = content_b.local_path {
                let bytes_b = std::fs::read(&path_b).map_err(|e| ToolError::internal(format!("Failed to read audio_b: {}", e)))?;
                Some(base64::engine::general_purpose::STANDARD.encode(&bytes_b))
            } else {
                None
            }
        } else {
            None
        };

        let text_cands = if text_candidates.is_empty() { None } else { Some(text_candidates) };

        tokio::spawn(async move {
            let result: anyhow::Result<hooteproto::responses::ToolResponse> = (async {
                let response = local_models
                    .run_clap_analyze(
                        audio_base64,
                        tasks,
                        audio_b_base64,
                        text_cands,
                        Some(job_id_clone.as_str().to_string()),
                    )
                    .await?;

                // Parse CLAP response
                let embeddings = response
                    .get("embeddings")
                    .and_then(|e| e.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect());

                let genre = response
                    .get("genre")
                    .and_then(|g| serde_json::from_value(g.clone()).ok());

                let mood = response
                    .get("mood")
                    .and_then(|m| serde_json::from_value(m.clone()).ok());

                let zero_shot = response
                    .get("zero_shot")
                    .and_then(|z| serde_json::from_value(z.clone()).ok());

                let similarity = response
                    .get("similarity")
                    .and_then(|s| s.as_f64())
                    .map(|f| f as f32);

                Ok(hooteproto::responses::ToolResponse::ClapAnalyzed(
                    hooteproto::responses::ClapAnalyzedResponse {
                        embeddings,
                        genre,
                        mood,
                        zero_shot,
                        similarity,
                    },
                ))
            })
            .await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, response);
                }
                Err(e) => {
                    tracing::error!(error = %e, "CLAP analysis failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "clap_analyze".to_string(),
        })
    }

    /// Extract MIDI file information (tempo, time signature, duration, etc.)
    pub async fn midi_info_typed(
        &self,
        request: hooteproto::request::MidiInfoRequest,
    ) -> Result<hooteproto::responses::MidiInfoResponse, ToolError> {
        // Get MIDI content - either from artifact or direct hash
        let hash = if let Some(ref artifact_id) = request.artifact_id {
            // Look up artifact to get hash
            let store = self.artifact_store.read().map_err(|_| ToolError::internal("Lock poisoned"))?;
            let artifact = store.get(artifact_id)
                .map_err(|e| ToolError::internal(e.to_string()))?
                .ok_or_else(|| ToolError::not_found("artifact", artifact_id.clone()))?;
            artifact.content_hash.as_str().to_string()
        } else if let Some(ref h) = request.hash {
            h.clone()
        } else {
            return Err(ToolError::validation("missing_parameter", "Either artifact_id or hash must be provided"));
        };

        // Get MIDI bytes from CAS
        let content = self.local_models
            .inspect_cas_content(&hash)
            .await
            .map_err(|e| ToolError::not_found("content", e.to_string()))?;

        let path = content.local_path
            .ok_or_else(|| ToolError::not_found("content", hash.clone()))?;

        let midi_bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read MIDI file: {}", e)))?;

        // Parse MIDI with our midi_info module
        let info = crate::mcp_tools::midi_info::extract_midi_info(&midi_bytes)
            .map_err(|e| ToolError::internal(format!("Failed to parse MIDI: {}", e)))?;

        // Convert to response type
        Ok(hooteproto::responses::MidiInfoResponse {
            tempo_bpm: info.tempo_bpm,
            tempo_changes: info.tempo_changes.into_iter().map(|tc| {
                hooteproto::responses::MidiTempoChange {
                    tick: tc.tick,
                    bpm: tc.bpm,
                }
            }).collect(),
            time_signature: info.time_signature,
            duration_seconds: info.duration_seconds,
            track_count: info.track_count,
            ppq: info.ppq,
            note_count: info.note_count,
            format: info.format,
        })
    }

    // =========================================================================
    // Garden Audio Tools
    // =========================================================================

    /// Attach PipeWire audio output.
    pub async fn garden_attach_audio_typed(
        &self,
        request: hooteproto::request::GardenAttachAudioRequest,
    ) -> Result<hooteproto::responses::GardenAudioStatusResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden. Start hootenanny with --chaosgarden=local or --chaosgarden=tcp://host:port",
            )
        })?;

        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        match manager
            .tool_request(ToolRequest::GardenAttachAudio(request))
            .await
        {
            Ok(ToolResponse::Ack(_)) => {
                // After attach, get current status
                self.garden_audio_status_typed().await
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Attach audio failed: {}", e))),
        }
    }

    /// Detach PipeWire audio output.
    pub async fn garden_detach_audio_typed(
        &self,
    ) -> Result<hooteproto::responses::GardenAudioStatusResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden",
            )
        })?;

        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        match manager.tool_request(ToolRequest::GardenDetachAudio).await {
            Ok(ToolResponse::Ack(_)) => {
                self.garden_audio_status_typed().await
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Detach audio failed: {}", e))),
        }
    }

    /// Get PipeWire audio output status.
    pub async fn garden_audio_status_typed(
        &self,
    ) -> Result<hooteproto::responses::GardenAudioStatusResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden",
            )
        })?;

        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        match manager.tool_request(ToolRequest::GardenAudioStatus).await {
            Ok(ToolResponse::GardenAudioStatus(response)) => Ok(response),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Get audio status failed: {}", e))),
        }
    }

    /// Attach PipeWire audio input for monitoring.
    pub async fn garden_attach_input_typed(
        &self,
        request: hooteproto::request::GardenAttachInputRequest,
    ) -> Result<hooteproto::responses::GardenInputStatusResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden",
            )
        })?;

        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        match manager
            .tool_request(ToolRequest::GardenAttachInput(request))
            .await
        {
            Ok(ToolResponse::Ack(_)) => {
                self.garden_input_status_typed().await
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Attach input failed: {}", e))),
        }
    }

    /// Detach PipeWire audio input.
    pub async fn garden_detach_input_typed(
        &self,
    ) -> Result<hooteproto::responses::GardenInputStatusResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden",
            )
        })?;

        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        match manager.tool_request(ToolRequest::GardenDetachInput).await {
            Ok(ToolResponse::Ack(_)) => {
                self.garden_input_status_typed().await
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Detach input failed: {}", e))),
        }
    }

    /// Get PipeWire audio input status.
    pub async fn garden_input_status_typed(
        &self,
    ) -> Result<hooteproto::responses::GardenInputStatusResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden",
            )
        })?;

        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        match manager.tool_request(ToolRequest::GardenInputStatus).await {
            Ok(ToolResponse::GardenInputStatus(response)) => Ok(response),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Get input status failed: {}", e))),
        }
    }

    /// Set monitor control (input passthrough to output).
    pub async fn garden_set_monitor_typed(
        &self,
        request: hooteproto::request::GardenSetMonitorRequest,
    ) -> Result<hooteproto::responses::GardenMonitorStatusResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden",
            )
        })?;

        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        match manager
            .tool_request(ToolRequest::GardenSetMonitor(request))
            .await
        {
            Ok(ToolResponse::GardenMonitorStatus(response)) => Ok(response),
            Ok(ToolResponse::Ack(_)) => {
                // Fallback for ack response - return defaults
                Ok(hooteproto::responses::GardenMonitorStatusResponse {
                    enabled: false,
                    gain: 1.0,
                })
            }
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Set monitor failed: {}", e))),
        }
    }

    /// Get audio snapshot from the streaming tap.
    pub async fn garden_get_audio_snapshot_typed(
        &self,
        request: hooteproto::request::GardenGetAudioSnapshotRequest,
    ) -> Result<hooteproto::responses::GardenAudioSnapshotResponse, ToolError> {
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden",
            )
        })?;

        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        match manager
            .tool_request(ToolRequest::GardenGetAudioSnapshot(request))
            .await
        {
            Ok(ToolResponse::GardenAudioSnapshot(response)) => Ok(response),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Get audio snapshot failed: {}", e))),
        }
    }

    /// Capture audio from monitor input and store to CAS.
    pub async fn audio_capture_typed(
        &self,
        request: hooteproto::request::AudioCaptureRequest,
    ) -> Result<hooteproto::responses::AudioCapturedResponse, ToolError> {
        use std::io::Cursor;

        let sample_rate = 48000u32;
        let channels = 2u16;
        let total_samples_needed = (request.duration_seconds * sample_rate as f32) as usize * channels as usize;
        let chunk_frames = 4096u32; // Fetch ~85ms chunks

        let mut accumulated_samples: Vec<f32> = Vec::with_capacity(total_samples_needed);

        // Accumulate samples from streaming tap
        while accumulated_samples.len() < total_samples_needed {
            let snapshot_request = hooteproto::request::GardenGetAudioSnapshotRequest {
                frames: chunk_frames,
            };

            let snapshot = self.garden_get_audio_snapshot_typed(snapshot_request).await?;
            accumulated_samples.extend(&snapshot.samples);

            // Small delay to let buffer refill
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        // Truncate to exact duration
        accumulated_samples.truncate(total_samples_needed);
        let actual_duration = accumulated_samples.len() as f32 / (sample_rate as f32 * channels as f32);

        // Encode to WAV
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec)
                .map_err(|e| ToolError::internal(format!("Failed to create WAV writer: {}", e)))?;
            for sample in &accumulated_samples {
                writer.write_sample(*sample)
                    .map_err(|e| ToolError::internal(format!("Failed to write sample: {}", e)))?;
            }
            writer.finalize()
                .map_err(|e| ToolError::internal(format!("Failed to finalize WAV: {}", e)))?;
        }
        let wav_bytes = cursor.into_inner();

        // Store to CAS
        let cas_result = self.cas_store_typed(&wav_bytes, "audio/wav").await?;

        // Create artifact
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash};

        let content_hash = ContentHash::new(&cas_result.hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

        let mut tags = request.tags.clone();
        tags.push("type:audio".to_string());
        tags.push("source:capture".to_string());

        let artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            request.creator.clone().unwrap_or_else(|| "unknown".to_string()),
            serde_json::json!({
                "duration_seconds": actual_duration,
                "sample_rate": sample_rate,
                "channels": channels,
                "mime_type": "audio/wav"
            }),
        )
        .with_tags(tags);

        {
            let store = self.artifact_store.write().map_err(|e| {
                ToolError::internal(format!("Failed to lock artifact store: {}", e))
            })?;
            store.put(artifact).map_err(|e| {
                ToolError::internal(format!("Failed to store artifact: {}", e))
            })?;
        }

        Ok(hooteproto::responses::AudioCapturedResponse {
            artifact_id: artifact_id.as_str().to_string(),
            content_hash: cas_result.hash,
            duration_seconds: actual_duration,
            sample_rate,
        })
    }

    // =========================================================================
    // Utility Tools
    // =========================================================================

    /// Get tool help documentation.
    pub async fn get_tool_help_typed(
        &self,
        topic: Option<&str>,
    ) -> Result<hooteproto::responses::ToolHelpResponse, ToolError> {
        let help_text = crate::api::tools::help::get_help(topic);
        Ok(hooteproto::responses::ToolHelpResponse {
            help: help_text,
            topic: topic.map(|s| s.to_string()),
        })
    }

    /// Create artifact from CAS hash.
    pub async fn artifact_create_typed(
        &self,
        request: hooteproto::request::ArtifactCreateRequest,
    ) -> Result<hooteproto::responses::ArtifactCreatedResponse, ToolError> {
        use crate::artifact_store::Artifact;
        use crate::types::{ArtifactId, ContentHash};

        let content_hash = ContentHash::new(&request.cas_hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
        let creator = request.creator.unwrap_or_else(|| "unknown".to_string());

        let artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            &creator,
            request.metadata,
        )
        .with_tags(request.tags.clone());

        let store = self
            .artifact_store
            .write()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;
        store
            .put(artifact)
            .map_err(|e| ToolError::internal(e.to_string()))?;
        store
            .flush()
            .map_err(|e| ToolError::internal(e.to_string()))?;

        Ok(hooteproto::responses::ArtifactCreatedResponse {
            artifact_id: artifact_id.as_str().to_string(),
            content_hash: request.cas_hash,
            tags: request.tags,
            creator,
        })
    }
}

