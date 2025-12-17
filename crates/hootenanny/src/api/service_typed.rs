//! Typed method implementations for EventDualityServer.
//!
//! These methods return typed response structs instead of `ToolResult`.
//! They are used by the TypedDispatcher for the new protocol.

use crate::api::service::EventDualityServer;
use hooteproto::{
    responses::{
        AbcParsedResponse, AbcValidatedResponse, AbcTransposedResponse, AbcValidationError,
        SoundfontInfoResponse, SoundfontPreset, SoundfontPresetInfoResponse, SoundfontRegion,
        GardenStatusResponse, GardenRegionsResponse, TransportState,
        JobStatusResponse, JobListResponse, JobState, JobCounts,
        ConfigValueResponse, ConfigValue,
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
            abc::Meter::Simple { numerator, denominator } => format!("{}/{}", numerator, denominator),
            abc::Meter::Common => "4/4".to_string(),
            abc::Meter::Cut => "2/2".to_string(),
            abc::Meter::None => "free".to_string(),
        });

        // Format key
        let key_str = Some(format!("{:?}{}", tune.header.key.root,
            tune.header.key.accidental.map(|a| format!("{:?}", a)).unwrap_or_default()));

        // Extract tempo from Tempo struct if present
        let tempo_val = tune.header.tempo.as_ref().map(|t| t.bpm);

        // Count notes across all voices
        let notes_count: usize = tune.voices.iter()
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
    pub async fn abc_validate_typed(&self, abc_str: &str) -> Result<AbcValidatedResponse, ToolError> {
        let parse_result = abc::parse(abc_str);

        let errors: Vec<AbcValidationError> = parse_result
            .errors()
            .map(|e| AbcValidationError {
                line: e.line,
                column: e.column,
                message: e.message.clone(),
            })
            .collect();

        let warnings: Vec<String> = parse_result
            .warnings()
            .map(|w| w.message.clone())
            .collect();

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
        let original_key = Some(format!("{:?}{}",
            parse_result.value.header.key.root,
            parse_result.value.header.key.accidental.map(|a| format!("{:?}", a)).unwrap_or_default()));

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
        let new_key = Some(format!("{:?}{}",
            transposed.header.key.root,
            transposed.header.key.accidental.map(|a| format!("{:?}", a)).unwrap_or_default()));
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
        use chaosgarden::ipc::ShellReply;

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;

        let reply = manager.get_transport_state().await.map_err(|e| {
            ToolError::service("chaosgarden", "status_failed", e.to_string())
        })?;

        match reply {
            ShellReply::TransportState { playing, position, tempo } => {
                Ok(GardenStatusResponse {
                    state: if playing { TransportState::Playing } else { TransportState::Stopped },
                    position_beats: position.0,
                    tempo_bpm: tempo,
                    region_count: 0, // Would need separate query
                })
            }
            ShellReply::Error { error, .. } => {
                Err(ToolError::service("chaosgarden", "status_failed", error))
            }
            _ => Err(ToolError::internal("Unexpected reply type for get_transport_state")),
        }
    }

    /// Get garden regions - typed response
    pub async fn garden_get_regions_typed(
        &self,
        start: Option<f64>,
        end: Option<f64>,
    ) -> Result<GardenRegionsResponse, ToolError> {
        use chaosgarden::ipc::{Beat, ShellReply, ShellRequest};
        use hooteproto::responses::GardenRegionInfo;

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;

        // Convert f64 beat range to (Beat, Beat) tuple
        let range = match (start, end) {
            (Some(s), Some(e)) => Some((Beat(s), Beat(e))),
            _ => None,
        };

        let reply = manager
            .request(ShellRequest::GetRegions { range })
            .await
            .map_err(|e| ToolError::service("chaosgarden", "get_regions_failed", e.to_string()))?;

        match reply {
            ShellReply::Regions { regions } => {
                let converted: Vec<GardenRegionInfo> = regions
                    .into_iter()
                    .map(|r| GardenRegionInfo {
                        region_id: r.region_id.to_string(),
                        position: r.position.0,
                        duration: r.duration.0,
                        behavior_type: if r.is_latent { "latent" } else { "play_content" }.to_string(),
                        content_id: r.artifact_id.unwrap_or_default(),
                    })
                    .collect();
                let count = converted.len();
                Ok(GardenRegionsResponse { regions: converted, count })
            }
            ShellReply::Error { error, .. } => {
                Err(ToolError::service("chaosgarden", "get_regions_failed", error))
            }
            _ => Err(ToolError::internal("Unexpected reply type for GetRegions")),
        }
    }

    // =========================================================================
    // Garden - Fire and Forget helpers
    //
    // These methods accept an optional job_id for correlation. The job_id is
    // passed to chaosgarden in message metadata, allowing hootenanny to track
    // async results back to jobs.
    // =========================================================================

    pub async fn garden_play_fire(&self, job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::ShellRequest;

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;
        manager
            .request_with_job_id(ShellRequest::Play, job_id)
            .await
            .map_err(|e| ToolError::service("chaosgarden", "play_failed", e.to_string()))?;
        Ok(())
    }

    pub async fn garden_pause_fire(&self, job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::ShellRequest;

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;
        manager
            .request_with_job_id(ShellRequest::Pause, job_id)
            .await
            .map_err(|e| ToolError::service("chaosgarden", "pause_failed", e.to_string()))?;
        Ok(())
    }

    pub async fn garden_stop_fire(&self, job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::ShellRequest;

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;
        manager
            .request_with_job_id(ShellRequest::Stop, job_id)
            .await
            .map_err(|e| ToolError::service("chaosgarden", "stop_failed", e.to_string()))?;
        Ok(())
    }

    pub async fn garden_seek_fire(&self, beat: f64, job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::{Beat, ShellRequest};

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;
        manager
            .request_with_job_id(ShellRequest::Seek { beat: Beat(beat) }, job_id)
            .await
            .map_err(|e| ToolError::service("chaosgarden", "seek_failed", e.to_string()))?;
        Ok(())
    }

    pub async fn garden_set_tempo_fire(&self, bpm: f64, job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::ShellRequest;

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;
        manager
            .request_with_job_id(ShellRequest::SetTempo { bpm }, job_id)
            .await
            .map_err(|e| ToolError::service("chaosgarden", "set_tempo_failed", e.to_string()))?;
        Ok(())
    }

    pub async fn garden_emergency_pause_fire(&self, job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::ControlRequest;

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;
        // Emergency pause goes on control channel (no job_id support there yet)
        manager
            .control(ControlRequest::EmergencyPause)
            .await
            .map_err(|e| ToolError::service("chaosgarden", "emergency_pause_failed", e.to_string()))?;
        Ok(())
    }

    pub async fn garden_create_region_fire(
        &self,
        position: f64,
        duration: f64,
        behavior_type: &str,
        content_id: &str,
        job_id: Option<&str>,
    ) -> Result<String, ToolError> {
        use chaosgarden::ipc::{Beat, Behavior, ShellReply, ShellRequest};

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;

        let behavior = match behavior_type {
            "play_content" => Behavior::PlayContent {
                artifact_id: content_id.to_string(),
            },
            "latent" => Behavior::Latent {
                job_id: content_id.to_string(),
            },
            _ => return Err(ToolError::validation("invalid_behavior", format!("Unknown behavior type: {}", behavior_type))),
        };

        let reply = manager
            .request_with_job_id(
                ShellRequest::CreateRegion {
                    position: Beat(position),
                    duration: Beat(duration),
                    behavior,
                },
                job_id,
            )
            .await
            .map_err(|e| ToolError::service("chaosgarden", "create_region_failed", e.to_string()))?;

        match reply {
            ShellReply::RegionCreated { region_id } => Ok(region_id.to_string()),
            ShellReply::Error { error, .. } => Err(ToolError::service("chaosgarden", "create_region_error", error)),
            _ => Err(ToolError::internal("unexpected reply from chaosgarden")),
        }
    }

    pub async fn garden_delete_region_fire(&self, region_id: &str, job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::{ShellReply, ShellRequest};

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;

        let region_uuid = uuid::Uuid::parse_str(region_id)
            .map_err(|_| ToolError::validation("invalid_uuid", "Invalid region_id UUID format"))?;

        let reply = manager
            .request_with_job_id(ShellRequest::DeleteRegion { region_id: region_uuid }, job_id)
            .await
            .map_err(|e| ToolError::service("chaosgarden", "delete_region_failed", e.to_string()))?;

        match reply {
            ShellReply::Ok { .. } => Ok(()),
            ShellReply::Error { error, .. } => Err(ToolError::service("chaosgarden", "delete_region_error", error)),
            _ => Err(ToolError::internal("unexpected reply from chaosgarden")),
        }
    }

    pub async fn garden_move_region_fire(&self, region_id: &str, new_position: f64, job_id: Option<&str>) -> Result<(), ToolError> {
        use chaosgarden::ipc::{Beat, ShellReply, ShellRequest};

        let manager = self
            .garden_manager
            .as_ref()
            .ok_or_else(|| ToolError::validation("not_connected", "Not connected to chaosgarden"))?;

        let region_uuid = uuid::Uuid::parse_str(region_id)
            .map_err(|_| ToolError::validation("invalid_uuid", "Invalid region_id UUID format"))?;

        let reply = manager
            .request_with_job_id(
                ShellRequest::MoveRegion {
                    region_id: region_uuid,
                    new_position: Beat(new_position),
                },
                job_id,
            )
            .await
            .map_err(|e| ToolError::service("chaosgarden", "move_region_failed", e.to_string()))?;

        match reply {
            ShellReply::Ok { .. } => Ok(()),
            ShellReply::Error { error, .. } => Err(ToolError::service("chaosgarden", "move_region_error", error)),
            _ => Err(ToolError::internal("unexpected reply from chaosgarden")),
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
            result: None, // TODO: Convert result to typed response
            error: info.error,
            created_at: info.created_at,
            started_at: info.started_at,
            completed_at: info.completed_at,
        })
    }

    /// List jobs - typed response
    pub async fn job_list_typed(&self, status_filter: Option<&str>) -> Result<JobListResponse, ToolError> {
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
                    result: None,
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

        let value = match (section, key) {
            (None, None) => {
                // Return full config as nested object
                ConfigValue::Object(std::collections::HashMap::from([
                    ("paths".to_string(), ConfigValue::Object(std::collections::HashMap::from([
                        ("state_dir".to_string(), ConfigValue::String(config.infra.paths.state_dir.display().to_string())),
                        ("cas_dir".to_string(), ConfigValue::String(config.infra.paths.cas_dir.display().to_string())),
                    ]))),
                    ("bind".to_string(), ConfigValue::Object(std::collections::HashMap::from([
                        ("http_port".to_string(), ConfigValue::Integer(config.infra.bind.http_port as i64)),
                    ]))),
                ]))
            }
            (Some("paths"), None) => {
                ConfigValue::Object(std::collections::HashMap::from([
                    ("state_dir".to_string(), ConfigValue::String(config.infra.paths.state_dir.display().to_string())),
                    ("cas_dir".to_string(), ConfigValue::String(config.infra.paths.cas_dir.display().to_string())),
                    ("socket_dir".to_string(), ConfigValue::String(config.infra.paths.socket_dir.display().to_string())),
                ]))
            }
            (Some("paths"), Some("cas_dir")) => {
                ConfigValue::String(config.infra.paths.cas_dir.display().to_string())
            }
            (Some("bind"), Some("http_port")) => {
                ConfigValue::Integer(config.infra.bind.http_port as i64)
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
}
