use crate::api::service::EventDualityServer;
use crate::api::schema::{CasStoreRequest, CasInspectRequest, UploadFileRequest, MidiToWavRequest, SoundfontInspectRequest, SoundfontPresetInspectRequest};
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::rustysynth::{render_midi_to_wav, inspect_soundfont, inspect_preset};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use baton::{ErrorData as McpError, CallToolResult, Content};
use baton::protocol::ProgressSender;
use base64::{Engine as _, engine::general_purpose};
use tracing;

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
    ) -> Result<CallToolResult, McpError> {
        let decoded_content = general_purpose::STANDARD.decode(&request.content_base64)
            .map_err(|e| McpError::parse_error(format!("Failed to base64 decode content: {}", e)))?;

        let hash = self.local_models.store_cas_content(&decoded_content, &request.mime_type)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to store content in CAS: {}", e)))?;

        tracing::Span::current().record("cas.hash", &hash);

        Ok(CallToolResult::success(vec![Content::text(hash)]))
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
    ) -> Result<CallToolResult, McpError> {
        let cas_ref = self.local_models.inspect_cas_content(&request.hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to inspect CAS: {}", e)))?;

        let span = tracing::Span::current();
        span.record("cas.mime_type", &*cas_ref.mime_type);
        span.record("cas.size_bytes", cas_ref.size_bytes);

        let result = serde_json::json!({
            "hash": cas_ref.hash,
            "mime_type": cas_ref.mime_type,
            "size": cas_ref.size_bytes,
            "local_path": cas_ref.local_path,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize CAS reference: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
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
    ) -> Result<CallToolResult, McpError> {
        // Read file from disk
        let file_bytes = tokio::fs::read(&request.file_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read file: {}", e)))?;

        let span = tracing::Span::current();
        span.record("file.size", file_bytes.len());

        // Store in CAS
        let hash = self.local_models.store_cas_content(&file_bytes, &request.mime_type)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to store file in CAS: {}", e)))?;

        span.record("cas.hash", &*hash);

        let result = serde_json::json!({
            "hash": hash,
            "size_bytes": file_bytes.len(),
            "mime_type": request.mime_type,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize response: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tracing::instrument(
        name = "mcp.tool.midi_to_wav",
        skip(self, request),
        fields(
            midi.hash = %request.input_hash,
            soundfont.hash = %request.soundfont_hash,
            audio.sample_rate = request.sample_rate.unwrap_or(44100),
            cas.output_hash = tracing::field::Empty,
        )
    )]
    pub async fn midi_to_wav(
        &self,
        request: MidiToWavRequest,
    ) -> Result<CallToolResult, McpError> {
        let sample_rate = request.sample_rate.unwrap_or(44100);

        // Fetch MIDI bytes from CAS
        let midi_ref = self.local_models.inspect_cas_content(&request.input_hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get MIDI from CAS: {}", e)))?;
        let midi_path = midi_ref.local_path
            .ok_or_else(|| McpError::internal_error("MIDI not found in local CAS"))?;
        let midi_bytes = tokio::fs::read(&midi_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read MIDI file: {}", e)))?;

        // Fetch SoundFont bytes from CAS
        let sf_ref = self.local_models.inspect_cas_content(&request.soundfont_hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get SoundFont from CAS: {}", e)))?;
        let sf_path = sf_ref.local_path
            .ok_or_else(|| McpError::internal_error("SoundFont not found in local CAS"))?;
        let sf_bytes = tokio::fs::read(&sf_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read SoundFont file: {}", e)))?;

        // Get SoundFont name for metadata (best effort)
        let soundfont_name = inspect_soundfont(&sf_bytes, false)
            .map(|info| info.info.name)
            .ok();

        // Render MIDI to WAV
        let wav_bytes = render_midi_to_wav(&midi_bytes, &sf_bytes, sample_rate)
            .map_err(|e| McpError::internal_error(format!("Failed to render MIDI to WAV: {}", e)))?;

        let wav_size = wav_bytes.len();
        let duration_secs = crate::mcp_tools::rustysynth::calculate_wav_duration(&wav_bytes, sample_rate);

        // Store WAV in CAS
        let wav_hash = self.local_models.store_cas_content(&wav_bytes, "audio/wav")
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to store WAV in CAS: {}", e)))?;

        tracing::Span::current().record("cas.output_hash", &*wav_hash);

        // Look up source artifact IDs
        let store = self.artifact_store.read()
            .map_err(|_| McpError::internal_error("Lock poisoned"))?;
        let midi_artifact_id = find_artifact_by_hash(&*store, &request.input_hash);
        let soundfont_artifact_id = find_artifact_by_hash(&*store, &request.soundfont_hash);
        drop(store);

        // Create artifact for tracking
        let mut tags = request.tags.clone();
        tags.push("type:audio".to_string());
        tags.push("format:wav".to_string());
        tags.push("tool:midi_to_wav".to_string());

        let content_hash = ContentHash::new(&wav_hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
        let metadata = serde_json::json!({
            "type": "wav_render",
            "source": {
                "midi_hash": request.input_hash,
                "midi_artifact_id": midi_artifact_id,
            },
            "soundfont": {
                "hash": request.soundfont_hash,
                "name": soundfont_name,
                "artifact_id": soundfont_artifact_id,
            },
            "params": {
                "sample_rate": sample_rate,
            },
            "output": {
                "duration_seconds": duration_secs,
                "channels": 2,
                "bit_depth": 16,
                "size_bytes": wav_size,
            },
        });

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            request.creator.unwrap_or_else(|| "unknown".to_string()),
            metadata,
        ).with_tags(tags);

        // Acquire lock for artifact store operations
        let store = self.artifact_store.write()
            .map_err(|_| McpError::internal_error("Lock poisoned"))?;

        if let Some(set_id) = request.variation_set_id {
            let next_idx = store.next_variation_index(&set_id)
                .unwrap_or(0);
            artifact = artifact.with_variation_set(VariationSetId::new(set_id), next_idx);
        }

        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent_id));
        }

        store.put(artifact)
            .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e)))?;
        store.flush()
            .map_err(|e| McpError::internal_error(format!("Failed to flush artifact store: {}", e)))?;

        let result = serde_json::json!({
            "artifact_id": artifact_id.as_str(),
            "hash": wav_hash,
            "size_bytes": wav_size,
            "duration_secs": duration_secs,
            "sample_rate": sample_rate,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize response: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
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
    ) -> Result<CallToolResult, McpError> {
        // Fetch SoundFont bytes from CAS
        let sf_ref = self.local_models.inspect_cas_content(&request.soundfont_hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get SoundFont from CAS: {}", e)))?;
        let sf_path = sf_ref.local_path
            .ok_or_else(|| McpError::internal_error("SoundFont not found in local CAS"))?;
        let sf_bytes = tokio::fs::read(&sf_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read SoundFont file: {}", e)))?;

        // Inspect the SoundFont
        let inspection = inspect_soundfont(&sf_bytes, request.include_drum_map)
            .map_err(|e| McpError::internal_error(format!("Failed to inspect SoundFont: {}", e)))?;

        let span = tracing::Span::current();
        span.record("soundfont.name", &*inspection.info.name);
        span.record("soundfont.preset_count", inspection.info.preset_count);

        let json = serde_json::to_string_pretty(&inspection)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize response: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
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
    ) -> Result<CallToolResult, McpError> {
        let sf_ref = self.local_models.inspect_cas_content(&request.soundfont_hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to get SoundFont from CAS: {}", e)))?;
        let sf_path = sf_ref.local_path
            .ok_or_else(|| McpError::internal_error("SoundFont not found in local CAS"))?;
        let sf_bytes = tokio::fs::read(&sf_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read SoundFont file: {}", e)))?;

        let inspection = inspect_preset(&sf_bytes, request.bank, request.program)
            .map_err(|e| McpError::internal_error(format!("Failed to inspect preset: {}", e)))?;

        let json = serde_json::to_string_pretty(&inspection)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize response: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// MIDI to WAV conversion with progress notifications
    pub async fn midi_to_wav_with_progress(
        &self,
        request: MidiToWavRequest,
        _progress: Option<ProgressSender>,
    ) -> Result<CallToolResult, McpError> {
        // TODO: Add progress notifications for conversion stages
        self.midi_to_wav(request).await
    }
}
