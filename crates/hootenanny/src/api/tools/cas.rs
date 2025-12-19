use crate::api::responses::{
    ArtifactUploadResponse, CasInspectResponse, CasStatsResponse, CasStoreResponse,
    CasUploadResponse, InstrumentInfo, MimeTypeStats, PresetInfo, SoundfontInspectResponse,
    SoundfontPresetResponse,
};
use crate::api::schema::{
    ArtifactUploadRequest, CasInspectRequest, CasStoreRequest, SoundfontInspectRequest,
    SoundfontPresetInspectRequest, UploadFileRequest,
};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::rustysynth::{inspect_preset, inspect_soundfont};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use base64::{engine::general_purpose, Engine as _};
use hooteproto::{ToolError, ToolOutput, ToolResult};

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
    pub async fn cas_store(&self, request: CasStoreRequest) -> ToolResult {
        let decoded_content = general_purpose::STANDARD
            .decode(&request.content_base64)
            .map_err(|e| {
                ToolError::validation(
                    "invalid_params",
                    format!("Failed to base64 decode content: {}", e),
                )
            })?;

        let hash = self
            .local_models
            .store_cas_content(&decoded_content, &request.mime_type)
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
    pub async fn cas_inspect(&self, request: CasInspectRequest) -> ToolResult {
        let cas_ref = self
            .local_models
            .inspect_cas_content(&request.hash)
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
            format!(
                "CAS content: {} ({} bytes, {})",
                response.hash, response.size_bytes, response.mime_type
            ),
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
                .filter(|e| {
                    e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "json")
                })
            {
                if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&contents) {
                        let mime_type = meta
                            .get("mime_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("application/octet-stream")
                            .to_string();
                        let size = meta.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

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
    pub async fn upload_file(&self, request: UploadFileRequest) -> ToolResult {
        let file_bytes = tokio::fs::read(&request.file_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read file: {}", e)))?;

        let span = tracing::Span::current();
        span.record("file.size", file_bytes.len());

        let hash = self
            .local_models
            .store_cas_content(&file_bytes, &request.mime_type)
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
            format!(
                "Uploaded {} ({} bytes) as {}",
                request.file_path, response.size_bytes, hash
            ),
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
    pub async fn artifact_upload(&self, request: ArtifactUploadRequest) -> ToolResult {
        let file_bytes = tokio::fs::read(&request.file_path)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to read file: {}", e)))?;

        let span = tracing::Span::current();
        span.record("file.size", file_bytes.len());

        let hash = self
            .local_models
            .store_cas_content(&file_bytes, &request.mime_type)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to store file in CAS: {}", e)))?;

        span.record("cas.hash", &*hash);

        let mut tags = request.tags.clone();
        if !tags.iter().any(|t| t.starts_with("type:")) && request.mime_type.starts_with("audio/") {
            tags.push(format!(
                "type:{}",
                request.mime_type.strip_prefix("audio/").unwrap_or("audio")
            ));
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
        )
        .with_tags(tags);

        let store = self
            .artifact_store
            .write()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;
        if let Some(set_id) = request.variation_set_id {
            let next_idx = store.next_variation_index(&set_id).unwrap_or(0);
            artifact = artifact.with_variation_set(VariationSetId::new(set_id), next_idx);
        }
        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent_id));
        }
        store
            .put(artifact)
            .map_err(|e| ToolError::internal(format!("Failed to store artifact: {}", e)))?;
        store
            .flush()
            .map_err(|e| ToolError::internal(format!("Failed to flush artifact store: {}", e)))?;

        let response = ArtifactUploadResponse {
            artifact_id: artifact_id.as_str().to_string(),
            content_hash: hash.clone(),
            size_bytes: file_bytes.len() as u64,
            mime_type: request.mime_type.clone(),
            source_path: request.file_path.clone(),
        };

        Ok(ToolOutput::new(
            format!(
                "Created artifact {} from {}",
                response.artifact_id, request.file_path
            ),
            &response,
        ))
    }

    /// List artifacts with optional filtering by tag and creator
    #[tracing::instrument(
        name = "mcp.tool.artifact_list",
        skip(self, request),
        fields(
            filter.tag = ?request.tag,
            filter.creator = ?request.creator,
            result.count = tracing::field::Empty,
        )
    )]
    pub async fn artifact_list(
        &self,
        request: crate::api::schema::ArtifactListRequest,
    ) -> ToolResult {
        let store = self
            .artifact_store
            .read()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;

        let all_artifacts = store
            .all()
            .map_err(|e| ToolError::internal(format!("Failed to list artifacts: {}", e)))?;

        let artifacts: Vec<_> = all_artifacts
            .into_iter()
            .filter(|a| {
                let tag_match = request
                    .tag
                    .as_ref()
                    .is_none_or(|t| a.tags.iter().any(|at| at == t));
                let creator_match = request
                    .creator
                    .as_ref()
                    .is_none_or(|c| a.creator.as_str() == c);
                tag_match && creator_match
            })
            .collect();

        tracing::Span::current().record("result.count", artifacts.len());

        Ok(ToolOutput::new(
            format!("Found {} artifacts", artifacts.len()),
            &artifacts,
        ))
    }

    /// Get a single artifact by ID
    #[tracing::instrument(
        name = "mcp.tool.artifact_get",
        skip(self, request),
        fields(
            artifact.id = %request.id,
        )
    )]
    pub async fn artifact_get(&self, request: crate::api::schema::ArtifactGetRequest) -> ToolResult {
        let store = self
            .artifact_store
            .read()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;

        match store.get(&request.id) {
            Ok(Some(artifact)) => Ok(ToolOutput::new(
                format!("Found artifact {}", request.id),
                &artifact,
            )),
            Ok(None) => Err(ToolError::not_found("artifact", &request.id)),
            Err(e) => Err(ToolError::internal(format!(
                "Failed to get artifact: {}",
                e
            ))),
        }
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
    pub async fn soundfont_inspect(&self, request: SoundfontInspectRequest) -> ToolResult {
        let sf_ref = self
            .local_models
            .inspect_cas_content(&request.soundfont_hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to get SoundFont from CAS: {}", e)))?;
        let sf_path = sf_ref
            .local_path
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
            presets: inspection
                .presets
                .iter()
                .map(|p| PresetInfo {
                    bank: p.bank,
                    program: p.program,
                    name: p.name.clone(),
                })
                .collect(),
            has_drum_presets: inspection.presets.iter().any(|p| p.bank == 128),
        };

        let human_text = format!(
            "SoundFont: {}\n{} presets ({})",
            inspection.info.name,
            inspection.info.preset_count,
            if response.has_drum_presets {
                "includes drums"
            } else {
                "melodic only"
            }
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
        let sf_ref = self
            .local_models
            .inspect_cas_content(&request.soundfont_hash)
            .await
            .map_err(|e| ToolError::internal(format!("Failed to get SoundFont from CAS: {}", e)))?;
        let sf_path = sf_ref
            .local_path
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
            instruments: inspection
                .regions
                .iter()
                .map(|region| InstrumentInfo {
                    name: region.keys.clone(),
                    key_range: None,
                    velocity_range: None,
                })
                .collect(),
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
