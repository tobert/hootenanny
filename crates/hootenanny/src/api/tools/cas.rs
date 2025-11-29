use crate::api::service::EventDualityServer;
use crate::api::schema::{CasStoreRequest, CasInspectRequest, UploadFileRequest, MidiToWavRequest};
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::rustysynth::render_midi_to_wav;
use baton::{ErrorData as McpError, CallToolResult, Content};
use base64::{Engine as _, engine::general_purpose};
use tracing;

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

        // Create artifact for tracking
        let mut tags = request.tags.clone();
        tags.push("type:audio".to_string());
        tags.push("tool:midi_to_wav".to_string());

        let artifact_id = format!("artifact_{}", &wav_hash[..12]);
        let data = serde_json::json!({
            "hash": wav_hash,
            "input_hash": request.input_hash,
            "soundfont_hash": request.soundfont_hash,
            "sample_rate": sample_rate,
            "duration_secs": duration_secs,
            "size_bytes": wav_size,
        });

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            request.creator.unwrap_or_else(|| "unknown".to_string()),
            data,
        ).with_tags(tags);

        if let Some(set_id) = request.variation_set_id {
            let next_idx = self.artifact_store.next_variation_index(&set_id)
                .unwrap_or(0);
            artifact = artifact.with_variation_set(set_id, next_idx);
        }

        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(parent_id);
        }
        self.artifact_store.put(artifact)
            .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e)))?;

        let result = serde_json::json!({
            "artifact_id": artifact_id,
            "hash": wav_hash,
            "size_bytes": wav_size,
            "duration_secs": duration_secs,
            "sample_rate": sample_rate,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize response: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}
