use crate::api::responses::{CasStoreResponse, CasInspectResponse, CasStatsResponse, MimeTypeStats, CasUploadResponse, ArtifactUploadResponse, MidiToWavResponse, SoundfontInspectResponse, PresetInfo, SoundfontPresetResponse, InstrumentInfo, JobSpawnResponse, JobStatus};
use crate::api::service::EventDualityServer;
use crate::api::schema::{CasStoreRequest, CasInspectRequest, UploadFileRequest, ArtifactUploadRequest, MidiToWavRequest, SoundfontInspectRequest, SoundfontPresetInspectRequest};
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::rustysynth::{render_midi_to_wav, inspect_soundfont, inspect_preset};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use hooteproto::{ToolOutput, ToolResult, ToolError};
use base64::{Engine as _, engine::general_purpose};
use tracing;
use std::sync::Arc;

/// Format bytes in human-readable form
fn humanize_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Look up an artifact ID by its content hash
fn find_artifact_by_hash<S: ArtifactStore>(store: &S, content_hash: &str) -> Option<String> {
    store.all().ok()?.into_iter()
        .find(|a| a.content_hash.as_str() == content_hash)
        .map(|a| a.id.as_str().to_string())
}

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.cas_store",
        skip(self, request),
        fields(
            cas.mime_type = %request.mime_type,
            cas.content_size = request.content_base64.len(),
            cas.hash = tracing::field::Empty,
        )
    )]
    pub async fn cas_store(
        &self,
        request: CasStoreRequest,
    ) -> ToolResult {
        let decoded_content = general_purpose::STANDARD.decode(&request.content_base64)
            .map_err(|e| ToolError::validation("invalid_params", format!("Failed to base64 decode content: {}", e)))?;

        let hash = self.local_models.store_cas_content(&decoded_content, &request.mime_type)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to store content in CAS: {}", e)))?;

        tracing::Span::current().record("cas.hash", &hash);

        let response = CasStoreResponse {
            hash: hash.clone(),
            size_bytes: decoded_content.len() as u64,
            mime_type: request.mime_type.clone(),
        };

        Ok(ToolOutput::new(
            format!("Stored {} bytes as {}", response.size_bytes, hash),
            &response,
        ))
    }

    #[tracing::instrument(
        name = "mcp.tool.cas_inspect",
        skip(self, request),
        fields(
            cas.hash = %request.hash,
            cas.mime_type = tracing::field::Empty,
            cas.size_bytes = tracing::field::Empty,
        )
    )]
    pub async fn cas_inspect(
        &self,
        request: CasInspectRequest,
    ) -> ToolResult {
        let cas_ref = self.local_models.inspect_cas_content(&request.hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to inspect CAS: {}", e)))?;

        let span = tracing::Span::current();
        span.record("cas.mime_type", &*cas_ref.mime_type);
        span.record("cas.size_bytes", cas_ref.size_bytes);

        let response = CasInspectResponse {
            hash: cas_ref.hash.to_string(),
            mime_type: cas_ref.mime_type.clone(),
            size_bytes: cas_ref.size_bytes,
            exists: true,
            local_path: cas_ref.local_path.clone(),
        };

        Ok(ToolOutput::new(
            format!("CAS content: {} ({} bytes, {})", response.hash, response.size_bytes, response.mime_type),
            &response,
        ))
    }

    #[tracing::instrument(
        name = "mcp.tool.cas_stats",
        skip(self),
        fields(
            cas.total_files = tracing::field::Empty,
            cas.total_bytes = tracing::field::Empty,
        )
    )]
    pub async fn cas_stats(&self) -> ToolResult {
        use std::collections::HashMap;

        let cas_dir = self.local_models.cas_base_path();
        let metadata_dir = cas_dir.join("metadata");

        let mut total_files = 0u64;
        let mut total_bytes = 0u64;
        let mut by_mime: HashMap<String, MimeTypeStats> = HashMap::new();

        // Walk the metadata directory to get stats
        if metadata_dir.exists() {
            for entry in walkdir::WalkDir::new(&metadata_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "json"))
            {
                if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&contents) {
                        let mime_type = meta.get("mime_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("application/octet-stream")
                            .to_string();
                        let size = meta.get("size")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        total_files += 1;
                        total_bytes += size;

                        let entry = by_mime.entry(mime_type).or_default();
                        entry.count += 1;
                        entry.bytes += size;
                    }
                }
            }
        }

        tracing::Span::current().record("cas.total_files", total_files);
        tracing::Span::current().record("cas.total_bytes", total_bytes);

        let response = CasStatsResponse {
            total_files,
            total_bytes,
            by_mime_type: by_mime,
        };

        let human_text = format!(
            "CAS: {} files, {} bytes ({} types)",
            total_files,
            humanize_bytes(total_bytes),
            response.by_mime_type.len()
        );

        Ok(ToolOutput::new(human_text, &response))
    }

    #[tracing::instrument(
        name = "mcp.tool.upload_file",
        skip(self, request),
        fields(
            file.path = %request.file_path,
            file.mime_type = %request.mime_type,
            file.size = tracing::field::Empty,
            cas.hash = tracing::field::Empty,
        )
    )]
    pub async fn upload_file(
        &self,
        request: UploadFileRequest,
    ) -> ToolResult {
        let file_bytes = tokio::fs::read(&request.file_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read file: {}", e)))?;

        let span = tracing::Span::current();
        span.record("file.size", file_bytes.len());

        let hash = self.local_models.store_cas_content(&file_bytes, &request.mime_type)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to store file in CAS: {}", e)))?;

        span.record("cas.hash", &*hash);

        let response = CasUploadResponse {
            hash: hash.clone(),
            size_bytes: file_bytes.len() as u64,
            mime_type: request.mime_type.clone(),
            source_path: request.file_path.clone(),
        };

        Ok(ToolOutput::new(
            format!("Uploaded {} ({} bytes) as {}", request.file_path, response.size_bytes, hash),
            &response,
        ))
    }

    #[tracing::instrument(
        name = "mcp.tool.artifact_upload",
        skip(self, request),
        fields(
            file.path = %request.file_path,
            file.mime_type = %request.mime_type,
            file.size = tracing::field::Empty,
            cas.hash = tracing::field::Empty,
            artifact.id = tracing::field::Empty,
        )
    )]
    pub async fn artifact_upload(
        &self,
        request: ArtifactUploadRequest,
    ) -> ToolResult {
        let file_bytes = tokio::fs::read(&request.file_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read file: {}", e)))?;

        let span = tracing::Span::current();
        span.record("file.size", file_bytes.len());

        let hash = self.local_models.store_cas_content(&file_bytes, &request.mime_type)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to store file in CAS: {}", e)))?;

        span.record("cas.hash", &*hash);

        let mut tags = request.tags.clone();
        if !tags.iter().any(|t| t.starts_with("type:")) && request.mime_type.starts_with("audio/") {
            tags.push(format!("type:{}", request.mime_type.strip_prefix("audio/").unwrap_or("audio")));
        }
        tags.push("tool:artifact_upload".to_string());

        let content_hash = ContentHash::new(&hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

        let metadata = serde_json::json!({
            "type": "uploaded_file",
            "source_path": request.file_path,
            "mime_type": request.mime_type,
            "size_bytes": file_bytes.len(),
        });

        span.record("artifact.id", artifact_id.as_str());

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            request.creator.unwrap_or_else(|| "unknown".to_string()),
            metadata,
        ).with_tags(tags);

        let store = self.artifact_store.write().map_err(|_| ToolError::internal("Lock poisoned"))?;
        if let Some(set_id) = request.variation_set_id {
            let next_idx = store.next_variation_index(&set_id).unwrap_or(0);
            artifact = artifact.with_variation_set(VariationSetId::new(set_id), next_idx);
        }
        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent_id));
        }
        store.put(artifact).map_err(|e| ToolError::internal(format!("Failed to store artifact: {}", e)))?;
        store.flush().map_err(|e| ToolError::internal(format!("Failed to flush artifact store: {}", e)))?;

        let response = ArtifactUploadResponse {
            artifact_id: artifact_id.as_str().to_string(),
            content_hash: hash.clone(),
            size_bytes: file_bytes.len() as u64,
            mime_type: request.mime_type.clone(),
            source_path: request.file_path.clone(),
        };

        Ok(ToolOutput::new(
            format!("Created artifact {} from {}", response.artifact_id, request.file_path),
            &response,
        ))
    }

    #[tracing::instrument(
        name = "mcp.tool.midi_to_wav",
        skip(self, request),
        fields(
            midi.hash = %request.input_hash,
            soundfont.hash = %request.soundfont_hash,
            audio.sample_rate = request.sample_rate.unwrap_or(44100),
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn midi_to_wav(
        &self,
        request: MidiToWavRequest,
    ) -> ToolResult {
        let job_id = self.job_store.create_job("convert_midi_to_wav".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let result: anyhow::Result<MidiToWavResponse> = (async {
                let sample_rate = request.sample_rate.unwrap_or(44100);

                let midi_ref = local_models.inspect_cas_content(&request.input_hash).await?;
                let midi_path = midi_ref.local_path.ok_or_else(|| anyhow::anyhow!("MIDI not found in local CAS"))?;
                let midi_bytes = tokio::fs::read(&midi_path).await?;

                let sf_ref = local_models.inspect_cas_content(&request.soundfont_hash).await?;
                let sf_path = sf_ref.local_path.ok_or_else(|| anyhow::anyhow!("SoundFont not found in local CAS"))?;
                let sf_bytes = tokio::fs::read(&sf_path).await?;

                let soundfont_name = inspect_soundfont(&sf_bytes, false).map(|info| info.info.name).ok();

                let wav_bytes = render_midi_to_wav(&midi_bytes, &sf_bytes, sample_rate)?;

                let wav_size = wav_bytes.len();
                let duration_secs = crate::mcp_tools::rustysynth::calculate_wav_duration(&wav_bytes, sample_rate);

                let wav_hash = local_models.store_cas_content(&wav_bytes, "audio/wav").await?;

                let store = artifact_store.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                let midi_artifact_id = find_artifact_by_hash(&*store, &request.input_hash);
                let soundfont_artifact_id = find_artifact_by_hash(&*store, &request.soundfont_hash);
                drop(store);

                let mut tags = request.tags.clone();
                tags.push("type:audio".to_string());
                tags.push("format:wav".to_string());
                tags.push("tool:midi_to_wav".to_string());

                let content_hash = ContentHash::new(&wav_hash);
                let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                let metadata = serde_json::json!({
                    "type": "wav_render",
                    "source": { "midi_hash": request.input_hash, "midi_artifact_id": midi_artifact_id },
                    "soundfont": { "hash": request.soundfont_hash, "name": soundfont_name, "artifact_id": soundfont_artifact_id },
                    "params": { "sample_rate": sample_rate },
                    "output": { "duration_seconds": duration_secs, "channels": 2, "bit_depth": 16, "size_bytes": wav_size },
                });

                let mut artifact = Artifact::new(
                    artifact_id.clone(),
                    content_hash,
                    request.creator.unwrap_or_else(|| "unknown".to_string()),
                    metadata,
                ).with_tags(tags);

                let store = artifact_store.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                if let Some(set_id) = request.variation_set_id {
                    let next_idx = store.next_variation_index(&set_id).unwrap_or(0);
                    artifact = artifact.with_variation_set(VariationSetId::new(set_id), next_idx);
                }
                if let Some(parent_id) = request.parent_id {
                    artifact = artifact.with_parent(ArtifactId::new(parent_id));
                }
                store.put(artifact)?;
                store.flush()?;

                Ok(MidiToWavResponse {
                    artifact_id: artifact_id.as_str().to_string(),
                    content_hash: wav_hash,
                    size_bytes: wav_size,
                    duration_secs,
                    sample_rate,
                })
            }).await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, serde_json::to_value(response).unwrap());
                }
                Err(e) => {
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        self.job_store.store_handle(&job_id, handle);

        let response = JobSpawnResponse {
            job_id: job_id.as_str().to_string(),
            status: JobStatus::Pending,
            artifact_id: None,
            content_hash: None,
            message: Some("MIDI to WAV conversion started.".to_string()),
        };

        Ok(ToolOutput::new(
            format!("Started job: {}", job_id.as_str()),
            &response,
        ))
    }

    #[tracing::instrument(
        name = "mcp.tool.soundfont_inspect",
        skip(self, request),
        fields(
            soundfont.hash = %request.soundfont_hash,
            soundfont.name = tracing::field::Empty,
            soundfont.preset_count = tracing::field::Empty,
        )
    )]
    pub async fn soundfont_inspect(
        &self,
        request: SoundfontInspectRequest,
    ) -> ToolResult {
        let sf_ref = self.local_models.inspect_cas_content(&request.soundfont_hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to get SoundFont from CAS: {}", e)))?;
        let sf_path = sf_ref.local_path
            .ok_or_else(|| ToolError::internal("SoundFont not found in local CAS"))?;
        let sf_bytes = tokio::fs::read(&sf_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read SoundFont file: {}", e)))?;

        let inspection = inspect_soundfont(&sf_bytes, request.include_drum_map)
            .map_err(|e| ToolError::internal(format!("Failed to inspect SoundFont: {}", e)))?;

        let span = tracing::Span::current();
        span.record("soundfont.name", &*inspection.info.name);
        span.record("soundfont.preset_count", inspection.info.preset_count);

        let response = SoundfontInspectResponse {
            soundfont_hash: request.soundfont_hash.clone(),
            presets: inspection.presets.iter().map(|p| PresetInfo {
                bank: p.bank,
                program: p.program,
                name: p.name.clone(),
            }).collect(),
            has_drum_presets: inspection.presets.iter().any(|p| p.bank == 128),
        };

        let human_text = format!(
            "SoundFont: {}\n{} presets ({})",
            inspection.info.name,
            inspection.info.preset_count,
            if response.has_drum_presets { "includes drums" } else { "melodic only" }
        );

        Ok(ToolOutput::new(human_text, &response))
    }

    #[tracing::instrument(
        name = "mcp.tool.soundfont_preset_inspect",
        skip(self, request),
        fields(
            soundfont.hash = %request.soundfont_hash,
            preset.bank = request.bank,
            preset.program = request.program,
        )
    )]
    pub async fn soundfont_preset_inspect(
        &self,
        request: SoundfontPresetInspectRequest,
    ) -> ToolResult {
        let sf_ref = self.local_models.inspect_cas_content(&request.soundfont_hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to get SoundFont from CAS: {}", e)))?;
        let sf_path = sf_ref.local_path
            .ok_or_else(|| ToolError::internal("SoundFont not found in local CAS"))?;
        let sf_bytes = tokio::fs::read(&sf_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read SoundFont file: {}", e)))?;

        let inspection = inspect_preset(&sf_bytes, request.bank, request.program)
            .map_err(|e| ToolError::internal(format!("Failed to inspect preset: {}", e)))?;

        let response = SoundfontPresetResponse {
            soundfont_hash: request.soundfont_hash.clone(),
            bank: inspection.bank,
            program: inspection.program,
            preset_name: inspection.name.clone(),
            instruments: inspection.regions.iter().map(|region| InstrumentInfo {
                name: region.keys.clone(),
                key_range: None,
                velocity_range: None,
            }).collect(),
        };

        let human_text = format!(
            "Preset: {} (Bank {}, Program {})\n{} regions",
            inspection.name,
            inspection.bank,
            inspection.program,
            inspection.regions.len()
        );

        Ok(ToolOutput::new(human_text, &response))
    }
}
