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
    /// Look up a CAS object by hash string, returning the reference.
    fn cas_lookup(&self, hash: &str) -> Result<cas::CasReference, ToolError> {
        use cas::ContentStore;
        let content_hash: cas::ContentHash = hash
            .parse()
            .map_err(|e| ToolError::internal(format!("Invalid hash: {}", e)))?;
        self.cas
            .inspect(&content_hash)?
            .ok_or_else(|| ToolError::not_found("cas_content", hash))
    }

    /// Store bytes in CAS, returning the hash string.
    fn cas_store(&self, data: &[u8], mime_type: &str) -> Result<String, ToolError> {
        use cas::ContentStore;
        self.cas
            .store(data, mime_type)
            .map(|h| h.into_inner())
            .map_err(|e| ToolError::internal(format!("Failed to store in CAS: {}", e)))
    }

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

        let cas_ref = self.cas_lookup(soundfont_hash)?;

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

        let cas_ref = self.cas_lookup(soundfont_hash)?;

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

    /// Get unified system status - combines transport, audio, and MIDI status.
    ///
    /// Makes 4 parallel ZMQ requests to chaosgarden and combines the results
    /// into a single unified response suitable for GUI polling.
    pub async fn unified_status_typed(
        &self,
    ) -> Result<hooteproto::responses::UnifiedStatusResponse, ToolError> {
        use hooteproto::responses::{
            AudioInputStatus, AudioOutputStatus, MidiConnectionInfo, MidiStatusResponse,
            MonitorStatus, TransportStatus, UnifiedStatusResponse,
        };

        // Fire all 4 requests in parallel
        let (transport, audio_out, audio_in, midi) = tokio::try_join!(
            self.garden_status_typed(),
            self.garden_audio_status_typed(),
            self.garden_input_status_typed(),
            self.midi_status_typed(),
        )?;

        Ok(UnifiedStatusResponse {
            transport: TransportStatus {
                state: transport.state,
                position_beats: transport.position_beats,
                tempo_bpm: transport.tempo_bpm,
                region_count: transport.region_count,
            },
            audio_output: AudioOutputStatus {
                attached: audio_out.attached,
                device_name: audio_out.device_name,
                sample_rate: audio_out.sample_rate,
                latency_frames: audio_out.latency_frames,
                callbacks: audio_out.callbacks,
                samples_written: audio_out.samples_written,
                underruns: audio_out.underruns,
            },
            audio_input: AudioInputStatus {
                attached: audio_in.attached,
                device_name: audio_in.device_name,
                sample_rate: audio_in.sample_rate,
                channels: audio_in.channels,
                callbacks: audio_in.callbacks,
                samples_captured: audio_in.samples_captured,
                overruns: audio_in.overruns,
            },
            monitor: MonitorStatus {
                enabled: audio_in.monitor_enabled,
                gain: audio_in.monitor_gain,
            },
            midi: MidiStatusResponse {
                inputs: midi
                    .inputs
                    .into_iter()
                    .map(|i| MidiConnectionInfo {
                        port_name: i.port_name,
                        messages: i.messages,
                    })
                    .collect(),
                outputs: midi
                    .outputs
                    .into_iter()
                    .map(|o| MidiConnectionInfo {
                        port_name: o.port_name,
                        messages: o.messages,
                    })
                    .collect(),
            },
        })
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
        let hash = self.cas_store(data, mime_type)?;

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
        let cas_ref = self.cas_lookup(hash)?;

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
        let cas_ref = self.cas_lookup(hash)?;

        Ok(hooteproto::responses::CasInspectedResponse {
            hash: cas_ref.hash.to_string(),
            exists: true,
            size: Some(cas_ref.size_bytes as usize),
            preview: None, // Could add preview logic if needed
        })
    }

    /// Get CAS storage statistics - typed response
    pub async fn cas_stats_typed(&self) -> Result<hooteproto::responses::CasStatsResponse, ToolError> {
        let cas_dir = self.cas.config().base_path.clone();
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

    /// Classify MIDI content via Orpheus ZMQ service - typed response
    pub async fn orpheus_classify_typed(
        &self,
        midi_hash: &str,
    ) -> Result<hooteproto::responses::OrpheusClassifiedResponse, ToolError> {
        use hooteproto::{Payload, ToolRequest, request::OrpheusClassifyRequest};

        let orpheus = self.orpheus.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Orpheus service not configured. Check orpheus_endpoint in config.",
            )
        })?;

        let request = OrpheusClassifyRequest {
            midi_hash: midi_hash.to_string(),
        };

        let payload = Payload::ToolRequest(ToolRequest::OrpheusClassify(request));

        let response = orpheus
            .request(payload)
            .await
            .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

        match response {
            Payload::TypedResponse(envelope) => match envelope {
                hooteproto::ResponseEnvelope::Success { response } => match response {
                    hooteproto::responses::ToolResponse::OrpheusClassified(resp) => Ok(resp),
                    _ => Err(ToolError::service(
                        "orpheus",
                        "unexpected_response",
                        "Expected OrpheusClassified response type",
                    )),
                },
                hooteproto::ResponseEnvelope::Error(err) => {
                    Err(ToolError::service("orpheus", "classify_failed", err.message()))
                }
                _ => Err(ToolError::service(
                    "orpheus",
                    "unexpected_envelope",
                    "Expected Success or Error envelope",
                )),
            },
            _ => Err(ToolError::service(
                "orpheus",
                "unexpected_payload",
                "Expected TypedResponse payload",
            )),
        }
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
        let midi_cas = self.cas_lookup(input_hash)?;

        let midi_path = midi_cas
            .local_path
            .ok_or_else(|| ToolError::not_found("midi", input_hash))?;

        let midi_bytes = std::fs::read(&midi_path)
            .map_err(|e| ToolError::internal(format!("Failed to read MIDI: {}", e)))?;

        // Get SoundFont content from CAS
        let sf_cas = self.cas_lookup(soundfont_hash)?;

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
        use hooteproto::{Payload, ToolRequest, request::OrpheusGenerateRequest, responses::ToolResponse};

        // Get the orpheus client
        let orpheus = self.orpheus.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Orpheus service not configured. Check orpheus_endpoint in config.",
            )
        })?;

        let variations_count = num_variations.unwrap_or(1) as usize;

        // Generate variation set ID if multiple variations
        let var_set_id = if variations_count > 1 {
            variation_set_id.clone().or_else(|| Some(VariationSetId::new(uuid::Uuid::new_v4().to_string()).as_str().to_string()))
        } else {
            variation_set_id.clone()
        };

        let mut output_hashes = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut tokens_per_variation = Vec::new();
        let mut total_tokens: u64 = 0;
        let creator_str = creator.clone().unwrap_or_else(|| "orpheus".to_string());

        // Generate each variation (Python service generates one at a time)
        for idx in 0..variations_count {
            // Create the request
            let request = OrpheusGenerateRequest {
                max_tokens,
                num_variations: Some(1), // Always 1 per call
                temperature,
                top_p,
                model: model.clone(),
                tags: tags.clone(),
                creator: creator.clone(),
                parent_id: parent_id.clone(),
                variation_set_id: var_set_id.clone(),
            };

            let payload = Payload::ToolRequest(ToolRequest::OrpheusGenerate(request));

            // Call the service
            let response = orpheus
                .request(payload)
                .await
                .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

            // Parse the response
            let (content_hash, num_tokens) = match response {
                Payload::TypedResponse(envelope) => {
                    match envelope {
                        hooteproto::ResponseEnvelope::Success { response } => {
                            match response {
                                ToolResponse::OrpheusGenerated(resp) => {
                                    let hash = resp.output_hashes.first()
                                        .ok_or_else(|| ToolError::service("orpheus", "invalid_response", "No output hash"))?
                                        .clone();
                                    let tokens = resp.tokens_per_variation.first().copied().unwrap_or(0);
                                    (hash, tokens)
                                }
                                _ => return Err(ToolError::service("orpheus", "invalid_response", "Unexpected response type")),
                            }
                        }
                        hooteproto::ResponseEnvelope::Error(err) => {
                            return Err(ToolError::service("orpheus", err.code(), err.message()));
                        }
                        hooteproto::ResponseEnvelope::JobStarted { .. } => {
                            return Err(ToolError::service("orpheus", "unexpected_async", "Got async response for sync operation"));
                        }
                        hooteproto::ResponseEnvelope::Ack { .. } => {
                            return Err(ToolError::service("orpheus", "unexpected_ack", "Got ack response for generate operation"));
                        }
                    }
                }
                _ => return Err(ToolError::service("orpheus", "invalid_response", "Unexpected payload type")),
            };

            tokens_per_variation.push(num_tokens);
            total_tokens += num_tokens;

            // Content is already in CAS (Python service stores it)
            let hash = ContentHash::new(&content_hash);
            let artifact_id = ArtifactId::from_hash_prefix(&hash);

            output_hashes.push(content_hash.clone());
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
                hash,
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
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use hooteproto::{Payload, ToolRequest, request::OrpheusGenerateSeededRequest, responses::ToolResponse};

        // Get the orpheus client
        let orpheus = self.orpheus.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Orpheus service not configured. Check orpheus_endpoint in config.",
            )
        })?;

        // Verify seed exists in CAS
        let _ = self.cas_lookup(seed_hash)?;

        let variations_count = num_variations.unwrap_or(1) as usize;
        let var_set_id = if variations_count > 1 {
            variation_set_id.clone().or_else(|| Some(VariationSetId::new(uuid::Uuid::new_v4().to_string()).as_str().to_string()))
        } else {
            variation_set_id.clone()
        };

        let mut output_hashes = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut tokens_per_variation = Vec::new();
        let mut total_tokens: u64 = 0;
        let creator_str = creator.clone().unwrap_or_else(|| "orpheus".to_string());
        let actual_parent_id = parent_id.clone().or_else(|| Some(seed_hash.to_string()));

        for idx in 0..variations_count {
            let request = OrpheusGenerateSeededRequest {
                seed_hash: seed_hash.to_string(),
                max_tokens,
                num_variations: Some(1),
                temperature,
                top_p,
                model: model.clone(),
                tags: tags.clone(),
                creator: creator.clone(),
                parent_id: actual_parent_id.clone(),
                variation_set_id: var_set_id.clone(),
            };

            let payload = Payload::ToolRequest(ToolRequest::OrpheusGenerateSeeded(request));

            let response = orpheus
                .request(payload)
                .await
                .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

            let (content_hash, num_tokens) = self.parse_orpheus_response(response)?;

            tokens_per_variation.push(num_tokens);
            total_tokens += num_tokens;

            let hash = ContentHash::new(&content_hash);
            let artifact_id = ArtifactId::from_hash_prefix(&hash);

            output_hashes.push(content_hash.clone());
            artifact_ids.push(artifact_id.as_str().to_string());

            self.create_orpheus_artifact(
                artifact_id, hash, &creator_str, &tags,
                "orpheus_generate_seeded", idx, num_tokens,
                actual_parent_id.as_deref(), var_set_id.as_deref(),
            )?;
        }

        let summary = format!(
            "Generated {} MIDI variation(s) from seed, {} total tokens",
            artifact_ids.len(), total_tokens
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
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use hooteproto::{Payload, ToolRequest, request::OrpheusContinueRequest, responses::ToolResponse};

        // Get the orpheus client
        let orpheus = self.orpheus.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Orpheus service not configured. Check orpheus_endpoint in config.",
            )
        })?;

        // Verify input exists in CAS
        let _ = self.cas_lookup(input_hash)?;

        let variations_count = num_variations.unwrap_or(1) as usize;
        let var_set_id = if variations_count > 1 {
            variation_set_id.clone().or_else(|| Some(VariationSetId::new(uuid::Uuid::new_v4().to_string()).as_str().to_string()))
        } else {
            variation_set_id.clone()
        };

        let mut output_hashes = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut tokens_per_variation = Vec::new();
        let mut total_tokens: u64 = 0;
        let creator_str = creator.clone().unwrap_or_else(|| "orpheus".to_string());
        let actual_parent_id = parent_id.clone().or_else(|| Some(input_hash.to_string()));

        for idx in 0..variations_count {
            let request = OrpheusContinueRequest {
                input_hash: input_hash.to_string(),
                max_tokens,
                num_variations: Some(1),
                temperature,
                top_p,
                model: model.clone(),
                tags: tags.clone(),
                creator: creator.clone(),
                parent_id: actual_parent_id.clone(),
                variation_set_id: var_set_id.clone(),
            };

            let payload = Payload::ToolRequest(ToolRequest::OrpheusContinue(request));

            let response = orpheus
                .request(payload)
                .await
                .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

            let (content_hash, num_tokens) = self.parse_orpheus_response(response)?;

            tokens_per_variation.push(num_tokens);
            total_tokens += num_tokens;

            let hash = ContentHash::new(&content_hash);
            let artifact_id = ArtifactId::from_hash_prefix(&hash);

            output_hashes.push(content_hash.clone());
            artifact_ids.push(artifact_id.as_str().to_string());

            self.create_orpheus_artifact(
                artifact_id, hash, &creator_str, &tags,
                "orpheus_continue", idx, num_tokens,
                actual_parent_id.as_deref(), var_set_id.as_deref(),
            )?;
        }

        let summary = format!(
            "Continued MIDI with {} variation(s), {} total tokens",
            artifact_ids.len(), total_tokens
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

    /// Generate bridge between sections using Orpheus - typed response
    pub async fn orpheus_bridge_typed(
        &self,
        section_a_hash: &str,
        section_b_hash: Option<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        model: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
        variation_set_id: Option<String>,
    ) -> Result<hooteproto::responses::OrpheusGeneratedResponse, ToolError> {
        use crate::types::{ArtifactId, ContentHash};
        use hooteproto::{Payload, ToolRequest, request::OrpheusBridgeRequest};

        // Get the orpheus client
        let orpheus = self.orpheus.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Orpheus service not configured. Check orpheus_endpoint in config.",
            )
        })?;

        // Verify section A exists in CAS
        let _ = self.cas_lookup(section_a_hash)?;

        // Verify section B if provided
        if let Some(ref hash) = section_b_hash {
            let _ = self.cas_lookup(hash)?;
        }

        let creator_str = creator.clone().unwrap_or_else(|| "orpheus_bridge".to_string());

        // Bridge generates single output
        let request = OrpheusBridgeRequest {
            section_a_hash: section_a_hash.to_string(),
            section_b_hash: section_b_hash.clone(),
            max_tokens,
            temperature,
            top_p,
            model,
            tags: tags.clone(),
            creator: creator.clone(),
            parent_id: parent_id.clone(),
            variation_set_id: variation_set_id.clone(),
        };

        let payload = Payload::ToolRequest(ToolRequest::OrpheusBridge(request));

        let response = orpheus
            .request(payload)
            .await
            .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

        let (content_hash, num_tokens) = self.parse_orpheus_response(response)?;

        let hash = ContentHash::new(&content_hash);
        let artifact_id = ArtifactId::from_hash_prefix(&hash);

        self.create_orpheus_artifact(
            artifact_id.clone(), hash, &creator_str, &tags,
            "orpheus_bridge", 0, num_tokens,
            parent_id.as_deref(), variation_set_id.as_deref(),
        )?;

        let summary = format!("Generated bridge, {} tokens", num_tokens);

        Ok(hooteproto::responses::OrpheusGeneratedResponse {
            output_hashes: vec![content_hash],
            artifact_ids: vec![artifact_id.as_str().to_string()],
            tokens_per_variation: vec![num_tokens],
            total_tokens: num_tokens,
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
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use hooteproto::{Payload, ToolRequest, request::OrpheusLoopsRequest};

        // Get the orpheus client
        let orpheus = self.orpheus.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Orpheus service not configured. Check orpheus_endpoint in config.",
            )
        })?;

        // Verify seed exists in CAS if provided
        if let Some(ref hash) = seed_hash {
            let _ = self.cas_lookup(hash)?;
        }

        let variations_count = num_variations.unwrap_or(1) as usize;
        let var_set_id = if variations_count > 1 {
            variation_set_id.clone().or_else(|| Some(VariationSetId::new(uuid::Uuid::new_v4().to_string()).as_str().to_string()))
        } else {
            variation_set_id.clone()
        };

        let mut output_hashes = Vec::new();
        let mut artifact_ids = Vec::new();
        let mut tokens_per_variation = Vec::new();
        let mut total_tokens: u64 = 0;
        let creator_str = creator.clone().unwrap_or_else(|| "orpheus_loops".to_string());
        let actual_parent_id = parent_id.clone().or_else(|| seed_hash.clone());

        for idx in 0..variations_count {
            let request = OrpheusLoopsRequest {
                seed_hash: seed_hash.clone(),
                max_tokens,
                num_variations: Some(1),
                temperature,
                top_p,
                tags: tags.clone(),
                creator: creator.clone(),
                parent_id: actual_parent_id.clone(),
                variation_set_id: var_set_id.clone(),
            };

            let payload = Payload::ToolRequest(ToolRequest::OrpheusLoops(request));

            let response = orpheus
                .request(payload)
                .await
                .map_err(|e| ToolError::service("orpheus", "request_failed", e.to_string()))?;

            let (content_hash, num_tokens) = self.parse_orpheus_response(response)?;

            tokens_per_variation.push(num_tokens);
            total_tokens += num_tokens;

            let hash = ContentHash::new(&content_hash);
            let artifact_id = ArtifactId::from_hash_prefix(&hash);

            output_hashes.push(content_hash.clone());
            artifact_ids.push(artifact_id.as_str().to_string());

            self.create_orpheus_artifact(
                artifact_id, hash, &creator_str, &tags,
                "orpheus_loops", idx, num_tokens,
                actual_parent_id.as_deref(), var_set_id.as_deref(),
            )?;
        }

        let summary = format!(
            "Generated {} loopable MIDI variation(s), {} total tokens",
            artifact_ids.len(), total_tokens
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
    // Orpheus ZMQ Helpers
    // ============================================================

    /// Parse an Orpheus ZMQ response to extract content_hash and num_tokens
    fn parse_orpheus_response(
        &self,
        response: hooteproto::Payload,
    ) -> Result<(String, u64), ToolError> {
        use hooteproto::responses::ToolResponse;

        match response {
            hooteproto::Payload::TypedResponse(envelope) => {
                match envelope {
                    hooteproto::ResponseEnvelope::Success { response } => {
                        match response {
                            ToolResponse::OrpheusGenerated(resp) => {
                                let hash = resp.output_hashes.first()
                                    .ok_or_else(|| ToolError::service("orpheus", "invalid_response", "No output hash"))?
                                    .clone();
                                let tokens = resp.tokens_per_variation.first().copied().unwrap_or(0);
                                Ok((hash, tokens))
                            }
                            _ => Err(ToolError::service("orpheus", "invalid_response", "Unexpected response type")),
                        }
                    }
                    hooteproto::ResponseEnvelope::Error(err) => {
                        Err(ToolError::service("orpheus", err.code(), err.message()))
                    }
                    hooteproto::ResponseEnvelope::JobStarted { .. } => {
                        Err(ToolError::service("orpheus", "unexpected_async", "Got async response for sync operation"))
                    }
                    hooteproto::ResponseEnvelope::Ack { .. } => {
                        Err(ToolError::service("orpheus", "unexpected_ack", "Got ack response for generate operation"))
                    }
                }
            }
            _ => Err(ToolError::service("orpheus", "invalid_response", "Unexpected payload type")),
        }
    }

    /// Create an artifact for generated Orpheus MIDI
    fn create_orpheus_artifact(
        &self,
        artifact_id: crate::types::ArtifactId,
        content_hash: crate::types::ContentHash,
        creator: &str,
        tags: &[String],
        source: &str,
        variation_index: usize,
        num_tokens: u64,
        parent_id: Option<&str>,
        variation_set_id: Option<&str>,
    ) -> Result<(), ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, VariationSetId};

        let mut artifact_tags = tags.to_vec();
        artifact_tags.push("type:midi".to_string());
        artifact_tags.push("source:orpheus".to_string());

        let metadata = serde_json::json!({
            "mime_type": "audio/midi",
            "source": source,
            "variation_index": variation_index,
            "tokens": num_tokens,
        });

        let mut artifact = Artifact::new(
            artifact_id,
            content_hash,
            creator,
            metadata,
        ).with_tags(artifact_tags);

        if let Some(parent) = parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent.to_string()));
        }
        if let Some(var_set) = variation_set_id {
            artifact.variation_set_id = Some(VariationSetId::new(var_set.to_string()));
            artifact.variation_index = Some(variation_index as u32);
        }

        let mut store = self.artifact_store.write().map_err(|e| {
            ToolError::internal(format!("Failed to lock artifact store: {}", e))
        })?;
        store.put(artifact).map_err(|e| {
            ToolError::internal(format!("Failed to store artifact: {}", e))
        })?;

        Ok(())
    }

    // ============================================================
    // AsyncLong Tools - Background job spawning
    // ============================================================

    /// Generate audio with MusicGen - spawns background job via ZMQ
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
        use hooteproto::{Payload, ToolRequest, request::MusicgenGenerateRequest, responses::ToolResponse};

        let musicgen = self.musicgen.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "MusicGen service not configured. Check musicgen endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("musicgen_generate".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let musicgen_client = Arc::clone(musicgen);

        let prompt_str = prompt.clone().unwrap_or_else(|| "ambient electronic music".to_string());

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<hooteproto::responses::ToolResponse> = (async {
                let request = MusicgenGenerateRequest {
                    prompt,
                    duration,
                    temperature,
                    top_k,
                    top_p,
                    guidance_scale,
                    do_sample,
                    tags: tags.clone(),
                    creator: creator.clone(),
                    parent_id: parent_id.clone(),
                    variation_set_id: variation_set_id.clone(),
                };

                let payload = Payload::ToolRequest(ToolRequest::MusicgenGenerate(request));

                let response = musicgen_client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("MusicGen request failed: {}", e))?;

                // Parse the ZMQ response
                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                let (content_hash_str, duration_seconds, sample_rate) = match &response {
                                    ToolResponse::AudioGenerated(r) => {
                                        (r.content_hash.clone(), r.duration_seconds, r.sample_rate)
                                    }
                                    _ => anyhow::bail!("Unexpected response type from MusicGen"),
                                };

                                let content_hash = ContentHash::new(&content_hash_str);
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

                                Ok(ToolResponse::AudioGenerated(
                                    hooteproto::responses::AudioGeneratedResponse {
                                        artifact_id: artifact_id.as_str().to_string(),
                                        content_hash: content_hash_str,
                                        duration_seconds,
                                        sample_rate,
                                        format: hooteproto::responses::AudioFormat::Wav,
                                        genre: None,
                                    },
                                ))
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("MusicGen error: {}", err.message())
                            }
                            hooteproto::ResponseEnvelope::JobStarted { .. } => {
                                anyhow::bail!("Unexpected async response")
                            }
                            hooteproto::ResponseEnvelope::Ack { .. } => {
                                anyhow::bail!("Unexpected ack response")
                            }
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
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
        self.job_store.store_handle(&job_id, handle);

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "musicgen_generate".to_string(),
        })
    }

    /// Generate song with YuE via ZMQ - spawns background job
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
        use hooteproto::{Payload, ToolRequest, request::YueGenerateRequest, responses::ToolResponse};

        let yue = self.yue.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "YuE service not configured. Check yue endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("yue_generate".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let yue_client = Arc::clone(yue);

        let genre_str = genre.clone().unwrap_or_else(|| "pop".to_string());

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<ToolResponse> = (async {
                let request = YueGenerateRequest {
                    lyrics: lyrics.clone(),
                    genre: genre.clone(),
                    max_new_tokens,
                    run_n_segments,
                    seed,
                    tags: tags.clone(),
                    creator: creator.clone(),
                    parent_id: parent_id.clone(),
                    variation_set_id: variation_set_id.clone(),
                };

                let payload = Payload::ToolRequest(ToolRequest::YueGenerate(request));

                let response = yue_client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("YuE request failed: {}", e))?;

                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                let (content_hash_str, duration_seconds, sample_rate) = match &response {
                                    ToolResponse::AudioGenerated(r) => {
                                        (r.content_hash.clone(), r.duration_seconds, r.sample_rate)
                                    }
                                    _ => anyhow::bail!("Unexpected response type from YuE"),
                                };

                                let content_hash = ContentHash::new(&content_hash_str);
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

                                Ok(ToolResponse::AudioGenerated(
                                    hooteproto::responses::AudioGeneratedResponse {
                                        artifact_id: artifact_id.as_str().to_string(),
                                        content_hash: content_hash_str,
                                        duration_seconds,
                                        sample_rate,
                                        format: hooteproto::responses::AudioFormat::Wav,
                                        genre: Some(genre_str),
                                    },
                                ))
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("YuE error: {}", err.message())
                            }
                            hooteproto::ResponseEnvelope::JobStarted { .. } => {
                                anyhow::bail!("Unexpected async response")
                            }
                            hooteproto::ResponseEnvelope::Ack { .. } => {
                                anyhow::bail!("Unexpected ack response")
                            }
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
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
        self.job_store.store_handle(&job_id, handle);

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
        use crate::api::tools::beat_this::prepare_audio_for_beatthis;
        use hooteproto::{Payload, ToolRequest, request::BeatthisAnalyzeRequest, responses::ToolResponse};

        // Get the beatthis client
        let beatthis = self.beatthis.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Beat-this service not configured. Check beatthis_endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("beatthis_analyze".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let beatthis_client = Arc::clone(beatthis);

        let audio_bytes = if let Some(ref hash) = audio_hash {
            let content = self.cas_lookup(hash)?;
            let path = content.local_path.ok_or_else(|| ToolError::not_found("audio", hash.clone()))?;
            std::fs::read(&path).map_err(|e| ToolError::internal(format!("Failed to read audio: {}", e)))?
        } else if let Some(ref path) = audio_path {
            std::fs::read(path).map_err(|e| ToolError::internal(format!("Failed to read audio: {}", e)))?
        } else {
            return Err(ToolError::validation("invalid_params", "Either audio_hash or audio_path required"));
        };

        // Check for silence before expensive GPU inference
        let decoded = chaosgarden::decode_audio(&audio_bytes)
            .map_err(|e| ToolError::internal(format!("Failed to decode audio: {}", e)))?;
        let (_, mean_db) = Self::calculate_audio_levels(&decoded.samples);
        if mean_db < -60.0 {
            return Err(ToolError::validation(
                "silent_audio",
                format!(
                    "Audio appears silent (mean: {:.1} dB). Beat detection requires audible content.",
                    mean_db
                ),
            ));
        }

        // Store prepared audio in CAS
        let prepared_audio = prepare_audio_for_beatthis(&audio_bytes)
            .map_err(|e| ToolError::internal(format!("Audio preparation failed: {}", e.message())))?;
        let cas_result = self.cas_store_typed(&prepared_audio, "audio/wav").await?;
        let audio_hash_for_service = cas_result.hash.clone();

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<hooteproto::responses::ToolResponse> = (async {
                let request = BeatthisAnalyzeRequest {
                    audio_hash: Some(audio_hash_for_service),
                    audio_path: None,
                    include_frames: false,
                };

                let payload = Payload::ToolRequest(ToolRequest::BeatthisAnalyze(request));

                let response = beatthis_client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("Beat-this request failed: {}", e))?;

                // Parse response
                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                match response {
                                    ToolResponse::BeatsAnalyzed(resp) => Ok(ToolResponse::BeatsAnalyzed(resp)),
                                    _ => anyhow::bail!("Unexpected response type"),
                                }
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("Beat-this error: {}", err.message())
                            }
                            hooteproto::ResponseEnvelope::JobStarted { .. } => {
                                anyhow::bail!("Unexpected async response")
                            }
                            hooteproto::ResponseEnvelope::Ack { .. } => {
                                anyhow::bail!("Unexpected ack response")
                            }
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
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
        self.job_store.store_handle(&job_id, handle);

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "beatthis_analyze".to_string(),
        })
    }

    /// Analyze audio with CLAP - spawns background job via ZMQ
    pub async fn clap_analyze_typed(
        &self,
        audio_hash: String,
        audio_b_hash: Option<String>,
        tasks: Vec<String>,
        text_candidates: Vec<String>,
        creator: Option<String>,
        parent_id: Option<String>,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use hooteproto::{Payload, ToolRequest, request::ClapAnalyzeRequest, responses::ToolResponse};

        let clap = self.clap.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "CLAP service not configured. Check clap endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("clap_analyze".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let clap_client = Arc::clone(clap);

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<hooteproto::responses::ToolResponse> = (async {
                let request = ClapAnalyzeRequest {
                    audio_hash,
                    audio_b_hash,
                    tasks,
                    text_candidates,
                    creator,
                    parent_id,
                };

                let payload = Payload::ToolRequest(ToolRequest::ClapAnalyze(request));

                let response = clap_client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("CLAP request failed: {}", e))?;

                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                match response {
                                    ToolResponse::ClapAnalyzed(resp) => Ok(ToolResponse::ClapAnalyzed(resp)),
                                    _ => anyhow::bail!("Unexpected response type from CLAP"),
                                }
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("CLAP error: {}", err.message())
                            }
                            hooteproto::ResponseEnvelope::JobStarted { .. } => {
                                anyhow::bail!("Unexpected async response")
                            }
                            hooteproto::ResponseEnvelope::Ack { .. } => {
                                anyhow::bail!("Unexpected ack response")
                            }
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
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
        self.job_store.store_handle(&job_id, handle);

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "clap_analyze".to_string(),
        })
    }

    /// Generate audio with AudioLDM2 - spawns background job via ZMQ
    pub async fn audioldm2_generate_typed(
        &self,
        req: hooteproto::request::Audioldm2GenerateRequest,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use hooteproto::{Payload, ToolRequest, responses::ToolResponse};

        let audioldm2 = self.audioldm2.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "AudioLDM2 service not configured. Check audioldm2 endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("audioldm2_generate".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let client = Arc::clone(audioldm2);

        let prompt_str = req.prompt.clone().unwrap_or_default();
        let tags = req.tags.clone();
        let creator = req.creator.clone();
        let parent_id = req.parent_id.clone();
        let variation_set_id = req.variation_set_id.clone();

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<ToolResponse> = (async {
                let payload = Payload::ToolRequest(ToolRequest::Audioldm2Generate(req));

                let response = client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("AudioLDM2 request failed: {}", e))?;

                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                let (content_hash_str, duration_seconds, sample_rate) = match &response {
                                    ToolResponse::Audioldm2Generated(r) => {
                                        (r.content_hash.clone(), r.duration_seconds, r.sample_rate)
                                    }
                                    ToolResponse::AudioGenerated(r) => {
                                        (r.content_hash.clone(), r.duration_seconds, r.sample_rate)
                                    }
                                    _ => anyhow::bail!("Unexpected response type from AudioLDM2"),
                                };

                                let content_hash = ContentHash::new(&content_hash_str);
                                let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

                                let creator_str = creator.unwrap_or_else(|| "audioldm2".to_string());
                                let mut artifact_tags = tags;
                                artifact_tags.push("type:audio".to_string());
                                artifact_tags.push("source:audioldm2".to_string());

                                let metadata = serde_json::json!({
                                    "mime_type": "audio/wav",
                                    "source": "audioldm2",
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

                                Ok(ToolResponse::AudioGenerated(
                                    hooteproto::responses::AudioGeneratedResponse {
                                        artifact_id: artifact_id.as_str().to_string(),
                                        content_hash: content_hash_str,
                                        duration_seconds,
                                        sample_rate,
                                        format: hooteproto::responses::AudioFormat::Wav,
                                        genre: None,
                                    },
                                ))
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("AudioLDM2 error: {}", err.message())
                            }
                            _ => anyhow::bail!("Unexpected response envelope"),
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
            })
            .await;

            match result {
                Ok(response) => { let _ = job_store.mark_complete(&job_id_clone, response); }
                Err(e) => {
                    tracing::error!(error = %e, "AudioLDM2 generation failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });
        self.job_store.store_handle(&job_id, handle);

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "audioldm2_generate".to_string(),
        })
    }

    /// Generate MIDI with Anticipatory Music Transformer - spawns background job via ZMQ
    pub async fn anticipatory_generate_typed(
        &self,
        req: hooteproto::request::AnticipatoryGenerateRequest,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use hooteproto::{Payload, ToolRequest, responses::ToolResponse};

        let anticipatory = self.anticipatory.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Anticipatory service not configured. Check anticipatory endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("anticipatory_generate".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let client = Arc::clone(anticipatory);

        let tags = req.tags.clone();
        let creator = req.creator.clone();
        let parent_id = req.parent_id.clone();
        let variation_set_id = req.variation_set_id.clone();

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<ToolResponse> = (async {
                let payload = Payload::ToolRequest(ToolRequest::AnticipatoryGenerate(req));

                let response = client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("Anticipatory request failed: {}", e))?;

                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                match response {
                                    ToolResponse::AnticipatoryGenerated(r) => {
                                        // Create artifact for the generated MIDI
                                        let content_hash = ContentHash::new(&r.content_hash);
                                        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

                                        let creator_str = creator.unwrap_or_else(|| "anticipatory".to_string());
                                        let mut artifact_tags = tags;
                                        artifact_tags.push("type:midi".to_string());
                                        artifact_tags.push("source:anticipatory".to_string());

                                        let metadata = serde_json::json!({
                                            "mime_type": "audio/midi",
                                            "source": "anticipatory",
                                            "model_size": r.model_size,
                                            "duration_seconds": r.duration_seconds,
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

                                        Ok(ToolResponse::AnticipatoryGenerated(
                                            hooteproto::responses::AnticipatoryGeneratedResponse {
                                                artifact_id: artifact_id.as_str().to_string(),
                                                ..r
                                            },
                                        ))
                                    }
                                    _ => anyhow::bail!("Unexpected response type from Anticipatory"),
                                }
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("Anticipatory error: {}", err.message())
                            }
                            _ => anyhow::bail!("Unexpected response envelope"),
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
            })
            .await;

            match result {
                Ok(response) => { let _ = job_store.mark_complete(&job_id_clone, response); }
                Err(e) => {
                    tracing::error!(error = %e, "Anticipatory generation failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });
        self.job_store.store_handle(&job_id, handle);

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "anticipatory_generate".to_string(),
        })
    }

    /// Continue MIDI with Anticipatory Music Transformer - spawns background job via ZMQ
    pub async fn anticipatory_continue_typed(
        &self,
        req: hooteproto::request::AnticipatoryContinueRequest,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash, VariationSetId};
        use hooteproto::{Payload, ToolRequest, responses::ToolResponse};

        let anticipatory = self.anticipatory.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Anticipatory service not configured. Check anticipatory endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("anticipatory_continue".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let client = Arc::clone(anticipatory);

        let tags = req.tags.clone();
        let creator = req.creator.clone();
        let parent_id = req.parent_id.clone();
        let variation_set_id = req.variation_set_id.clone();

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<ToolResponse> = (async {
                let payload = Payload::ToolRequest(ToolRequest::AnticipatoryContinue(req));

                let response = client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("Anticipatory continue request failed: {}", e))?;

                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                match response {
                                    ToolResponse::AnticipatoryGenerated(r) => {
                                        let content_hash = ContentHash::new(&r.content_hash);
                                        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

                                        let creator_str = creator.unwrap_or_else(|| "anticipatory".to_string());
                                        let mut artifact_tags = tags;
                                        artifact_tags.push("type:midi".to_string());
                                        artifact_tags.push("source:anticipatory".to_string());

                                        let metadata = serde_json::json!({
                                            "mime_type": "audio/midi",
                                            "source": "anticipatory",
                                            "model_size": r.model_size,
                                            "duration_seconds": r.duration_seconds,
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

                                        Ok(ToolResponse::AnticipatoryGenerated(
                                            hooteproto::responses::AnticipatoryGeneratedResponse {
                                                artifact_id: artifact_id.as_str().to_string(),
                                                ..r
                                            },
                                        ))
                                    }
                                    _ => anyhow::bail!("Unexpected response type from Anticipatory"),
                                }
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("Anticipatory continue error: {}", err.message())
                            }
                            _ => anyhow::bail!("Unexpected response envelope"),
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
            })
            .await;

            match result {
                Ok(response) => { let _ = job_store.mark_complete(&job_id_clone, response); }
                Err(e) => {
                    tracing::error!(error = %e, "Anticipatory continue failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });
        self.job_store.store_handle(&job_id, handle);

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "anticipatory_continue".to_string(),
        })
    }

    /// Extract MIDI embeddings with Anticipatory - spawns background job via ZMQ
    pub async fn anticipatory_embed_typed(
        &self,
        req: hooteproto::request::AnticipatoryEmbedRequest,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use hooteproto::{Payload, ToolRequest, responses::ToolResponse};

        let anticipatory = self.anticipatory.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Anticipatory service not configured. Check anticipatory endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("anticipatory_embed".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let client = Arc::clone(anticipatory);

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<ToolResponse> = (async {
                let payload = Payload::ToolRequest(ToolRequest::AnticipatoryEmbed(req));

                let response = client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("Anticipatory embed request failed: {}", e))?;

                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                match response {
                                    ToolResponse::AnticipatoryEmbedded(resp) => Ok(ToolResponse::AnticipatoryEmbedded(resp)),
                                    _ => anyhow::bail!("Unexpected response type from Anticipatory embed"),
                                }
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("Anticipatory embed error: {}", err.message())
                            }
                            _ => anyhow::bail!("Unexpected response envelope"),
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
            })
            .await;

            match result {
                Ok(response) => { let _ = job_store.mark_complete(&job_id_clone, response); }
                Err(e) => {
                    tracing::error!(error = %e, "Anticipatory embed failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });
        self.job_store.store_handle(&job_id, handle);

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "anticipatory_embed".to_string(),
        })
    }

    /// Separate audio into stems with Demucs - spawns background job via ZMQ
    pub async fn demucs_separate_typed(
        &self,
        req: hooteproto::request::DemucsSeparateRequest,
    ) -> Result<hooteproto::responses::JobStartedResponse, ToolError> {
        use hooteproto::{Payload, ToolRequest, responses::ToolResponse};

        let demucs = self.demucs.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Demucs service not configured. Check demucs endpoint in config.",
            )
        })?;

        let job_id = self.job_store.create_job("demucs_separate".to_string());
        let _ = self.job_store.mark_running(&job_id);

        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let client = Arc::clone(demucs);

        let handle = tokio::spawn(async move {
            let result: anyhow::Result<ToolResponse> = (async {
                let payload = Payload::ToolRequest(ToolRequest::DemucsSeparate(req));

                let response = client
                    .request(payload)
                    .await
                    .map_err(|e| anyhow::anyhow!("Demucs request failed: {}", e))?;

                match response {
                    Payload::TypedResponse(envelope) => {
                        match envelope {
                            hooteproto::ResponseEnvelope::Success { response } => {
                                match response {
                                    ToolResponse::DemucsSeparated(resp) => Ok(ToolResponse::DemucsSeparated(resp)),
                                    _ => anyhow::bail!("Unexpected response type from Demucs"),
                                }
                            }
                            hooteproto::ResponseEnvelope::Error(err) => {
                                anyhow::bail!("Demucs error: {}", err.message())
                            }
                            _ => anyhow::bail!("Unexpected response envelope"),
                        }
                    }
                    _ => anyhow::bail!("Unexpected payload type"),
                }
            })
            .await;

            match result {
                Ok(response) => { let _ = job_store.mark_complete(&job_id_clone, response); }
                Err(e) => {
                    tracing::error!(error = %e, "Demucs separation failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });
        self.job_store.store_handle(&job_id, handle);

        Ok(hooteproto::responses::JobStartedResponse {
            job_id: job_id.as_str().to_string(),
            tool: "demucs_separate".to_string(),
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
        let content = self.cas_lookup(&hash)?;

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

    /// Get audio file information (duration, sample rate, levels) without GPU.
    pub async fn audio_info_typed(
        &self,
        request: hooteproto::request::AudioInfoRequest,
    ) -> Result<hooteproto::responses::AudioInfoResponse, ToolError> {
        // Get audio content - either from artifact or direct hash
        let hash = if let Some(ref artifact_id) = request.artifact_id {
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

        // Get audio bytes from CAS
        let content = self.cas_lookup(&hash)?;

        let path = content.local_path
            .ok_or_else(|| ToolError::not_found("content", hash.clone()))?;

        let audio_bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read audio file: {}", e)))?;

        // Decode audio
        let decoded = chaosgarden::decode_audio(&audio_bytes)
            .map_err(|e| ToolError::internal(format!("Failed to decode audio: {}", e)))?;

        // Calculate audio levels
        let (peak_db, mean_db) = Self::calculate_audio_levels(&decoded.samples);
        let is_silent = mean_db < -60.0;

        Ok(hooteproto::responses::AudioInfoResponse {
            duration_seconds: decoded.duration_seconds(),
            sample_rate: decoded.sample_rate,
            channels: decoded.channels as u16,
            peak_db,
            mean_db,
            is_silent,
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

        // Check if monitor is enabled when capturing from monitor source
        let source = request.source.as_deref().unwrap_or("monitor");
        if source == "monitor" {
            let input_status = self.garden_input_status_typed().await?;
            if !input_status.monitor_enabled {
                return Err(ToolError::validation(
                    "monitor_disabled",
                    "Cannot capture from monitor - monitor is disabled. \
                     Use audio_monitor(enabled=true) first.",
                ));
            }
        }

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

    // =========================================================================
    // RAVE Streaming (coordinated between Python RAVE and chaosgarden)
    // =========================================================================

    /// Start RAVE streaming session.
    ///
    /// This coordinates between Python RAVE service (model loading) and
    /// chaosgarden (audio I/O). The flow is:
    /// 1. Python RAVE: load model, bind ZMQ PAIR socket
    /// 2. chaosgarden: connect to socket, start audio routing
    pub async fn rave_stream_start_typed(
        &self,
        request: hooteproto::request::RaveStreamStartRequest,
    ) -> Result<hooteproto::responses::RaveStreamStartedResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;
        use hooteproto::Payload;

        // Step 1: Tell Python RAVE to prepare (via ZMQ proxy)
        let rave = self.rave.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "RAVE service not configured. Check rave_endpoint in config.",
            )
        })?;

        let payload = Payload::ToolRequest(ToolRequest::RaveStreamStart(request.clone()));

        let rave_response = rave
            .request(payload)
            .await
            .map_err(|e| ToolError::internal(format!("RAVE service error: {}", e)))?;

        // Extract response from Payload
        let (stream_id, model, latency_ms) = match rave_response {
            Payload::TypedResponse(envelope) => {
                match envelope {
                    hooteproto::ResponseEnvelope::Success { response } => {
                        match response {
                            ToolResponse::RaveStreamStarted(resp) => {
                                (resp.stream_id, resp.model, resp.latency_ms)
                            }
                            _ => return Err(ToolError::internal("Unexpected response type from RAVE")),
                        }
                    }
                    hooteproto::ResponseEnvelope::Error(tool_error) => {
                        return Err(ToolError::internal(format!("RAVE error: {:?}", tool_error)));
                    }
                    _ => return Err(ToolError::internal("Unexpected envelope type from RAVE")),
                }
            }
            Payload::Error { message, .. } => {
                return Err(ToolError::internal(format!("RAVE error: {}", message)));
            }
            _ => return Err(ToolError::internal("Unexpected payload type from RAVE")),
        };

        // Step 2: Tell chaosgarden to connect and start audio routing
        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "Not connected to chaosgarden",
            )
        })?;

        // Send tool request to chaosgarden (uses Cap'n Proto path)
        let garden_result = manager
            .tool_request(ToolRequest::RaveStreamStart(request.clone()))
            .await;

        match garden_result {
            Ok(ToolResponse::RaveStreamStarted(_)) => {
                tracing::info!("RAVE streaming started: stream_id={}", stream_id);
            }
            Ok(_) => {
                tracing::warn!("chaosgarden RAVE start: unexpected response type");
            }
            Err(e) => {
                tracing::warn!("chaosgarden RAVE start failed: {}", e);
                // Continue anyway - Python RAVE is ready
            }
        }

        Ok(hooteproto::responses::RaveStreamStartedResponse {
            stream_id,
            model,
            input_identity: request.input_identity,
            output_identity: request.output_identity,
            latency_ms,
        })
    }

    /// Stop RAVE streaming session.
    pub async fn rave_stream_stop_typed(
        &self,
        request: hooteproto::request::RaveStreamStopRequest,
    ) -> Result<hooteproto::responses::RaveStreamStoppedResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;
        use hooteproto::Payload;

        // Step 1: Stop chaosgarden audio routing (uses Cap'n Proto path)
        if let Some(manager) = self.garden_manager.as_ref() {
            let _ = manager
                .tool_request(ToolRequest::RaveStreamStop(request.clone()))
                .await;
        }

        // Step 2: Stop Python RAVE
        let rave = self.rave.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "RAVE service not configured",
            )
        })?;

        let payload = Payload::ToolRequest(ToolRequest::RaveStreamStop(request.clone()));

        let rave_response = rave
            .request(payload)
            .await
            .map_err(|e| ToolError::internal(format!("RAVE service error: {}", e)))?;

        let duration_seconds = match rave_response {
            Payload::TypedResponse(hooteproto::ResponseEnvelope::Success {
                response: ToolResponse::RaveStreamStopped(resp),
            }) => resp.duration_seconds,
            _ => 0.0,
        };

        tracing::info!(
            "RAVE streaming stopped: stream_id={}, duration={:.1}s",
            request.stream_id,
            duration_seconds
        );

        Ok(hooteproto::responses::RaveStreamStoppedResponse {
            stream_id: request.stream_id,
            duration_seconds,
        })
    }

    /// Get RAVE streaming session status.
    pub async fn rave_stream_status_typed(
        &self,
        request: hooteproto::request::RaveStreamStatusRequest,
    ) -> Result<hooteproto::responses::RaveStreamStatusResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;
        use hooteproto::Payload;

        // Query Python RAVE for status (it's the source of truth)
        let rave = self.rave.as_ref().ok_or_else(|| {
            ToolError::validation(
                "not_connected",
                "RAVE service not configured",
            )
        })?;

        let payload = Payload::ToolRequest(ToolRequest::RaveStreamStatus(request.clone()));

        let rave_response = rave
            .request(payload)
            .await
            .map_err(|e| ToolError::internal(format!("RAVE service error: {}", e)))?;

        match rave_response {
            Payload::TypedResponse(envelope) => {
                match envelope {
                    hooteproto::ResponseEnvelope::Success { response } => {
                        match response {
                            ToolResponse::RaveStreamStatus(resp) => Ok(resp),
                            _ => Err(ToolError::internal("Unexpected response type")),
                        }
                    }
                    hooteproto::ResponseEnvelope::Error(tool_error) => {
                        Err(ToolError::internal(format!("RAVE error: {:?}", tool_error)))
                    }
                    _ => Err(ToolError::internal("Unexpected envelope type")),
                }
            }
            Payload::Error { message, .. } => {
                Err(ToolError::internal(format!("RAVE error: {}", message)))
            }
            _ => Err(ToolError::internal("Unexpected payload type")),
        }
    }

    // =========================================================================
    // MIDI I/O (direct ALSA via chaosgarden)
    // =========================================================================

    /// List available MIDI input and output ports.
    pub async fn midi_list_ports_typed(
        &self,
    ) -> Result<hooteproto::responses::MidiPortsResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::MidiListPorts).await {
            Ok(ToolResponse::MidiPorts(resp)) => Ok(resp),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("List MIDI ports failed: {}", e))),
        }
    }

    /// Attach a MIDI input by port name pattern.
    pub async fn midi_input_attach_typed(
        &self,
        request: hooteproto::request::MidiAttachRequest,
    ) -> Result<hooteproto::responses::MidiAttachedResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::MidiInputAttach(request)).await {
            Ok(ToolResponse::MidiAttached(resp)) => Ok(resp),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Attach MIDI input failed: {}", e))),
        }
    }

    /// Detach a MIDI input by port name pattern.
    pub async fn midi_input_detach_typed(
        &self,
        request: hooteproto::request::MidiDetachRequest,
    ) -> Result<hooteproto::responses::MidiStatusResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        // Detach returns Ack, then fetch status
        match manager.tool_request(ToolRequest::MidiInputDetach(request)).await {
            Ok(ToolResponse::Ack(_)) => self.midi_status_typed().await,
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Detach MIDI input failed: {}", e))),
        }
    }

    /// Attach a MIDI output by port name pattern.
    pub async fn midi_output_attach_typed(
        &self,
        request: hooteproto::request::MidiAttachRequest,
    ) -> Result<hooteproto::responses::MidiAttachedResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::MidiOutputAttach(request)).await {
            Ok(ToolResponse::MidiAttached(resp)) => Ok(resp),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Attach MIDI output failed: {}", e))),
        }
    }

    /// Detach a MIDI output by port name pattern.
    pub async fn midi_output_detach_typed(
        &self,
        request: hooteproto::request::MidiDetachRequest,
    ) -> Result<hooteproto::responses::MidiStatusResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        // Detach returns Ack, then fetch status
        match manager.tool_request(ToolRequest::MidiOutputDetach(request)).await {
            Ok(ToolResponse::Ack(_)) => self.midi_status_typed().await,
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Detach MIDI output failed: {}", e))),
        }
    }

    /// Send a MIDI message to an output port.
    pub async fn midi_send_typed(
        &self,
        request: hooteproto::request::MidiSendRequest,
    ) -> Result<hooteproto::responses::AckResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::MidiSend(request)).await {
            Ok(ToolResponse::Ack(resp)) => Ok(resp),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Send MIDI failed: {}", e))),
        }
    }

    /// Get MIDI I/O status (active connections and message counts).
    pub async fn midi_status_typed(
        &self,
    ) -> Result<hooteproto::responses::MidiStatusResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::MidiStatus).await {
            Ok(ToolResponse::MidiStatus(resp)) => Ok(resp),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("Get MIDI status failed: {}", e))),
        }
    }

    /// Play a MIDI artifact to external outputs.
    ///
    /// Looks up the artifact to get the content hash, then sends a PlayMidi
    /// request to chaosgarden which loads the MIDI file and schedules it
    /// for playback to attached MIDI outputs.
    pub async fn midi_play_typed(
        &self,
        request: hooteproto::request::MidiPlayRequest,
    ) -> Result<hooteproto::responses::MidiPlayStartedResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        // Look up artifact to get content hash - scope the lock to avoid Send issues
        let content_hash = {
            let store = self.artifact_store.read().map_err(|_| ToolError::internal("Lock poisoned"))?;
            let artifact = store.get(&request.artifact_id)
                .map_err(|e| ToolError::internal(e.to_string()))?
                .ok_or_else(|| ToolError::not_found("artifact", &request.artifact_id))?;
            artifact.content_hash.as_str().to_string()
        };

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::MidiPlay(hooteproto::request::MidiPlayRequest {
            artifact_id: content_hash, // Pass the content_hash to daemon
            port_pattern: request.port_pattern,
            start_beat: request.start_beat,
        })).await {
            Ok(ToolResponse::MidiPlayStarted(resp)) => Ok(resp),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("MIDI play failed: {}", e))),
        }
    }

    /// Stop MIDI file playback.
    ///
    /// Removes the MIDI region from the playback engine, stopping any
    /// further MIDI events from being sent to outputs.
    pub async fn midi_stop_typed(
        &self,
        request: hooteproto::request::MidiStopRequest,
    ) -> Result<hooteproto::responses::MidiPlayStoppedResponse, ToolError> {
        use hooteproto::request::ToolRequest;
        use hooteproto::responses::ToolResponse;

        let manager = self.garden_manager.as_ref().ok_or_else(|| {
            ToolError::validation("not_connected", "Not connected to chaosgarden")
        })?;

        match manager.tool_request(ToolRequest::MidiStop(request)).await {
            Ok(ToolResponse::MidiPlayStopped(resp)) => Ok(resp),
            Ok(other) => Err(ToolError::internal(format!("Unexpected response: {:?}", other))),
            Err(e) => Err(ToolError::internal(format!("MIDI stop failed: {}", e))),
        }
    }

    // =========================================================================
    // Audio Analysis Helpers
    // =========================================================================

    /// Calculate peak and RMS audio levels in dB.
    ///
    /// Returns (peak_db, mean_db) where 0 dB = full scale.
    fn calculate_audio_levels(samples: &[f32]) -> (f32, f32) {
        if samples.is_empty() {
            return (f32::NEG_INFINITY, f32::NEG_INFINITY);
        }

        let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();

        let peak_db = if peak > 0.0 { 20.0 * peak.log10() } else { f32::NEG_INFINITY };
        let mean_db = if rms > 0.0 { 20.0 * rms.log10() } else { f32::NEG_INFINITY };

        (peak_db, mean_db)
    }

    // =========================================================================
    // MIDI Analysis / Voice Separation
    // =========================================================================

    /// Analyze MIDI structure: extract notes, profile tracks, detect merged voices.
    pub async fn midi_analyze_typed(
        &self,
        request: hooteproto::request::MidiAnalyzeRequest,
    ) -> Result<hooteproto::responses::MidiAnalyzedResponse, ToolError> {
        let hash = self.resolve_midi_hash(&request.artifact_id, &request.hash)?;
        let midi_bytes = self.read_cas_bytes(&hash).await?;

        let analysis = midi_analysis::analyze(&midi_bytes, request.polyphony_threshold)
            .map_err(|e| ToolError::internal(format!("MIDI analysis failed: {}", e)))?;

        let analysis_json = serde_json::to_string(&analysis)
            .map_err(|e| ToolError::internal(format!("Failed to serialize analysis: {}", e)))?;

        Ok(hooteproto::responses::MidiAnalyzedResponse {
            analysis_json,
            track_count: analysis.context.track_count as u16,
            tracks_needing_separation: analysis.tracks_needing_separation.iter().map(|&i| i as u16).collect(),
            summary: analysis.summary,
        })
    }

    /// Separate merged voices in MIDI tracks into individual musical lines.
    pub async fn midi_voice_separate_typed(
        &self,
        request: hooteproto::request::MidiVoiceSeparateRequest,
    ) -> Result<hooteproto::responses::MidiVoiceSeparatedResponse, ToolError> {
        let hash = self.resolve_midi_hash(&request.artifact_id, &request.hash)?;
        let midi_bytes = self.read_cas_bytes(&hash).await?;

        // First analyze to get notes and context
        let analysis = midi_analysis::analyze(&midi_bytes, None)
            .map_err(|e| ToolError::internal(format!("MIDI analysis failed: {}", e)))?;

        let smf = midly::Smf::parse(&midi_bytes)
            .map_err(|e| ToolError::internal(format!("MIDI parse failed: {}", e)))?;

        let (all_notes, _context) = midi_analysis::analyze::extract_notes(&smf);

        // Determine which tracks to separate
        let track_indices: Vec<usize> = if request.track_indices.is_empty() {
            analysis.tracks_needing_separation.to_vec()
        } else {
            request.track_indices.iter().map(|&i| i as usize).collect()
        };

        // Parse method
        let method = request.method.as_deref().and_then(|m| match m {
            "auto" | "" => None,
            "channel_split" => Some(midi_analysis::SeparationMethod::ChannelSplit),
            "pitch_contiguity" => Some(midi_analysis::SeparationMethod::PitchContiguity),
            "skyline" => Some(midi_analysis::SeparationMethod::Skyline),
            "bassline" => Some(midi_analysis::SeparationMethod::Bassline),
            _ => None,
        });

        let params = midi_analysis::SeparationParams {
            max_pitch_jump: request.max_pitch_jump,
            max_gap_ticks: request.max_gap_beats.map(|b| (b * analysis.context.ppq as f64) as u64),
            method,
            max_voices: request.max_voices.map(|v| v as usize),
        };

        // Separate each flagged track
        let mut all_voices: Vec<midi_analysis::SeparatedVoice> = Vec::new();
        let mut primary_method = String::from("auto");

        for &track_idx in &track_indices {
            let track_notes: Vec<midi_analysis::TimedNote> = all_notes
                .iter()
                .filter(|n| n.track_index == track_idx)
                .cloned()
                .collect();

            if track_notes.is_empty() {
                continue;
            }

            let voices = midi_analysis::separate_voices(
                &track_notes,
                analysis.context.ppq,
                &params,
            );

            if let Some(first) = voices.first() {
                primary_method = format!("{:?}", first.method).to_lowercase();
            }

            all_voices.extend(voices);
        }

        // Re-index voices sequentially
        for (i, voice) in all_voices.iter_mut().enumerate() {
            voice.voice_index = i;
        }

        let voice_count = all_voices.len() as u16;
        let voices_json = serde_json::to_string(&all_voices)
            .map_err(|e| ToolError::internal(format!("Failed to serialize voices: {}", e)))?;

        let summary = format!(
            "Separated {} tracks into {} voices using {}",
            track_indices.len(),
            voice_count,
            primary_method,
        );

        Ok(hooteproto::responses::MidiVoiceSeparatedResponse {
            voice_count,
            voices_json,
            method: primary_method,
            summary,
        })
    }

    /// Export separated voices as individual MIDI files stored in CAS.
    pub async fn midi_stems_export_typed(
        &self,
        request: hooteproto::request::MidiStemsExportRequest,
    ) -> Result<hooteproto::responses::MidiStemsExportedResponse, ToolError> {
        use crate::artifact_store::{Artifact, ArtifactStore};
        use crate::types::{ArtifactId, ContentHash};

        // Deserialize voices from JSON
        let voices: Vec<midi_analysis::SeparatedVoice> =
            serde_json::from_str(&request.voice_data)
                .map_err(|e| ToolError::validation("invalid_voice_data", format!("Failed to parse voice_data JSON: {}", e)))?;

        // Get original MIDI context for tempo map
        let context = if request.artifact_id.is_some() || request.hash.is_some() {
            let hash = self.resolve_midi_hash(&request.artifact_id, &request.hash)?;
            let midi_bytes = self.read_cas_bytes(&hash).await?;
            let analysis = midi_analysis::analyze(&midi_bytes, None)
                .map_err(|e| ToolError::internal(format!("MIDI analysis failed: {}", e)))?;
            analysis.context
        } else {
            midi_analysis::MidiFileContext {
                ppq: 480,
                format: 1,
                track_count: 0,
                tempo_changes: vec![midi_analysis::analyze::TempoChange {
                    tick: 0,
                    microseconds_per_beat: 500_000,
                    bpm: 120.0,
                }],
                time_signatures: vec![midi_analysis::analyze::TimeSignature {
                    tick: 0,
                    numerator: 4,
                    denominator: 4,
                }],
                total_ticks: 0,
            }
        };

        let export_options = midi_analysis::ExportOptions::default();
        let mut stem_infos = Vec::new();

        for voice in &voices {
            let midi_bytes = midi_analysis::voices_to_midi(
                std::slice::from_ref(voice),
                &context,
                &export_options,
            );

            let cas_result = self.cas_store_typed(&midi_bytes, "audio/midi").await?;
            let content_hash = ContentHash::new(&cas_result.hash);
            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

            let mut artifact_tags = request.tags.clone();
            artifact_tags.push("type:midi".to_string());
            artifact_tags.push("stem".to_string());
            artifact_tags.push(format!("voice:{}", voice.voice_index));

            let metadata = serde_json::json!({
                "mime_type": "audio/midi",
                "source": "midi_stems_export",
                "voice_index": voice.voice_index,
                "method": format!("{:?}", voice.method).to_lowercase(),
                "note_count": voice.notes.len(),
            });

            let artifact = Artifact::new(
                artifact_id.clone(),
                content_hash.clone(),
                request.creator.clone().unwrap_or_else(|| "midi_stems_export".to_string()),
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

            stem_infos.push(hooteproto::responses::MidiStemInfo {
                voice_index: voice.voice_index as u16,
                artifact_id: artifact_id.as_str().to_string(),
                content_hash: content_hash.as_str().to_string(),
                note_count: voice.notes.len() as u32,
                method: format!("{:?}", voice.method).to_lowercase(),
            });
        }

        // Optionally export combined multi-track file
        let (combined_artifact_id, combined_hash) = if request.combined_file && !voices.is_empty() {
            let combined_bytes = midi_analysis::voices_to_midi(&voices, &context, &export_options);
            let cas_result = self.cas_store_typed(&combined_bytes, "audio/midi").await?;
            let content_hash = ContentHash::new(&cas_result.hash);
            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

            let mut artifact_tags = request.tags.clone();
            artifact_tags.push("type:midi".to_string());
            artifact_tags.push("combined_stems".to_string());

            let metadata = serde_json::json!({
                "mime_type": "audio/midi",
                "source": "midi_stems_export",
                "voice_count": voices.len(),
            });

            let artifact = Artifact::new(
                artifact_id.clone(),
                content_hash.clone(),
                request.creator.clone().unwrap_or_else(|| "midi_stems_export".to_string()),
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

            (Some(artifact_id.as_str().to_string()), Some(content_hash.as_str().to_string()))
        } else {
            (None, None)
        };

        let summary = format!(
            "Exported {} voice stems as individual MIDI files{}",
            stem_infos.len(),
            if combined_artifact_id.is_some() { " + combined file" } else { "" },
        );

        Ok(hooteproto::responses::MidiStemsExportedResponse {
            stems: stem_infos,
            combined_artifact_id,
            combined_hash,
            summary,
        })
    }

    /// Helper: resolve artifact_id or hash to a CAS hash
    fn resolve_midi_hash(
        &self,
        artifact_id: &Option<String>,
        hash: &Option<String>,
    ) -> Result<String, ToolError> {
        if let Some(ref artifact_id) = artifact_id {
            let store = self.artifact_store.read()
                .map_err(|_| ToolError::internal("Lock poisoned"))?;
            let artifact = store.get(artifact_id)
                .map_err(|e| ToolError::internal(e.to_string()))?
                .ok_or_else(|| ToolError::not_found("artifact", artifact_id.clone()))?;
            Ok(artifact.content_hash.as_str().to_string())
        } else if let Some(ref h) = hash {
            Ok(h.clone())
        } else {
            Err(ToolError::validation("missing_parameter", "Either artifact_id or hash must be provided"))
        }
    }

    /// Helper: read bytes from CAS by hash
    async fn read_cas_bytes(&self, hash: &str) -> Result<Vec<u8>, ToolError> {
        let content = self.cas_lookup(hash)?;
        let path = content.local_path
            .ok_or_else(|| ToolError::not_found("content", hash.to_string()))?;
        tokio::fs::read(&path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read file: {}", e)))
    }
}

