use crate::api::responses::BeatthisAnalyzeResponse;
use crate::api::schema::{AnalyzeBeatsRequest, BeatThisServiceRequest, BeatThisServiceResponse};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash};
use baton::{CallToolResult, Content, ErrorData as McpError};
use base64::{engine::general_purpose, Engine as _};

/// Look up an artifact ID by its content hash
fn find_artifact_by_hash<S: ArtifactStore>(store: &S, content_hash: &str) -> Option<String> {
    store.all().ok()?.into_iter()
        .find(|a| a.content_hash.as_str() == content_hash)
        .map(|a| a.id.as_str().to_string())
}

const BEAT_THIS_URL: &str = "http://localhost:2012/predict";
const BEAT_THIS_TIMEOUT_SECS: u64 = 30;
const REQUIRED_SAMPLE_RATE: u32 = 22050;

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.analyze_beats",
        skip(self, request),
        fields(
            audio.source = tracing::field::Empty,
            audio.size_bytes = tracing::field::Empty,
            beats.count = tracing::field::Empty,
            beats.bpm = tracing::field::Empty,
        )
    )]
    pub async fn analyze_beats(
        &self,
        request: AnalyzeBeatsRequest,
    ) -> Result<CallToolResult, McpError> {
        let span = tracing::Span::current();

        // Track source audio hash for artifact metadata
        let source_audio_hash: Option<String>;

        let audio_bytes = match (&request.audio_path, &request.audio_hash) {
            (Some(path), None) => {
                span.record("audio.source", "file");
                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Failed to read audio file: {}", e)))?;
                // Store in CAS to get hash for provenance
                source_audio_hash = Some(self.local_models.store_cas_content(&bytes, "audio/wav")
                    .await
                    .map_err(|e| McpError::internal_error(format!("Failed to store audio in CAS: {}", e)))?);
                bytes
            }
            (None, Some(hash)) => {
                span.record("audio.source", "cas");
                source_audio_hash = Some(hash.clone());
                let cas_ref = self
                    .local_models
                    .inspect_cas_content(hash)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Failed to get audio from CAS: {}", e)))?;
                let local_path = cas_ref
                    .local_path
                    .ok_or_else(|| McpError::internal_error("Audio not found in local CAS"))?;
                tokio::fs::read(&local_path)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Failed to read CAS audio: {}", e)))?
            }
            (Some(_), Some(_)) => {
                return Err(McpError::invalid_params(
                    "Provide either audio_path or audio_hash, not both",
                ));
            }
            (None, None) => {
                return Err(McpError::invalid_params(
                    "Either audio_path or audio_hash is required",
                ));
            }
        };

        span.record("audio.size_bytes", audio_bytes.len());

        validate_wav_format(&audio_bytes)?;

        let audio_base64 = general_purpose::STANDARD.encode(&audio_bytes);

        let service_request = BeatThisServiceRequest {
            audio: audio_base64,
            client_job_id: None,
        };

        let client = reqwest::Client::new();
        let response = client
            .post(BEAT_THIS_URL)
            .json(&service_request)
            .timeout(std::time::Duration::from_secs(BEAT_THIS_TIMEOUT_SECS))
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    McpError::internal_error(
                        "beat-this service not running. Start with: just start beat-this",
                    )
                } else if e.is_timeout() {
                    McpError::internal_error("beat-this request timed out (30s limit)")
                } else {
                    McpError::internal_error(format!("beat-this request failed: {}", e))
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                422 => McpError::invalid_params(format!(
                    "Invalid audio format: {}. Required: 22050 Hz mono WAV, ≤30s",
                    error_text
                )),
                429 => McpError::internal_error(
                    "beat-this service busy (already processing another request)",
                ),
                _ => McpError::internal_error(format!(
                    "beat-this error ({}): {}",
                    status, error_text
                )),
            });
        }

        let service_response: BeatThisServiceResponse = response
            .json()
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to parse response: {}", e)))?;

        span.record("beats.count", service_response.num_beats);
        span.record("beats.bpm", service_response.bpm);

        let beats_per_measure = if service_response.num_downbeats > 0 {
            (service_response.num_beats as f64 / service_response.num_downbeats as f64).round()
                as usize
        } else {
            4
        };

        // Look up source audio artifact ID
        let source_artifact_id = if let Some(ref hash) = source_audio_hash {
            let store = self.artifact_store.read()
                .map_err(|_| McpError::internal_error("Lock poisoned"))?;
            find_artifact_by_hash(&*store, hash)
        } else {
            None
        };

        // Build analysis results for storage
        let analysis_data = if request.include_frames {
            serde_json::json!({
                "bpm": service_response.bpm,
                "num_beats": service_response.num_beats,
                "num_downbeats": service_response.num_downbeats,
                "duration_seconds": service_response.duration,
                "beats_per_measure": beats_per_measure,
                "beat_times": service_response.beats,
                "downbeat_times": service_response.downbeats,
                "frames": service_response.frames,
            })
        } else {
            serde_json::json!({
                "bpm": service_response.bpm,
                "num_beats": service_response.num_beats,
                "num_downbeats": service_response.num_downbeats,
                "duration_seconds": service_response.duration,
                "beats_per_measure": beats_per_measure,
                "beat_times": service_response.beats,
                "downbeat_times": service_response.downbeats,
            })
        };

        // Store analysis results in CAS as JSON
        let analysis_json = serde_json::to_string(&analysis_data)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize analysis: {}", e)))?;
        let analysis_hash = self.local_models.store_cas_content(analysis_json.as_bytes(), "application/json")
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to store analysis in CAS: {}", e)))?;

        // Create artifact for the beat analysis
        let content_hash = ContentHash::new(&analysis_hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

        let artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            "agent_beatthis",
            serde_json::json!({
                "type": "beat_analysis",
                "source": {
                    "audio_hash": source_audio_hash,
                    "audio_artifact_id": source_artifact_id,
                },
                "results": {
                    "bpm": service_response.bpm,
                    "num_beats": service_response.num_beats,
                    "num_downbeats": service_response.num_downbeats,
                    "duration_seconds": service_response.duration,
                    "beats_per_measure": beats_per_measure,
                },
            })
        )
        .with_tags(vec![
            "type:analysis".to_string(),
            "tool:beatthis_analyze".to_string(),
            format!("bpm:{}", service_response.bpm.round() as u32),
        ]);

        // Store artifact
        let store = self.artifact_store.write()
            .map_err(|_| McpError::internal_error("Lock poisoned"))?;
        store.put(artifact)
            .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e)))?;
        store.flush()
            .map_err(|e| McpError::internal_error(format!("Failed to flush artifact store: {}", e)))?;

        // Build response with structured content
        let response = BeatthisAnalyzeResponse {
            beats: service_response.beats.clone(),
            downbeats: service_response.downbeats.clone(),
            estimated_bpm: service_response.bpm,
            confidence: 0.95, // beat-this doesn't provide confidence, using default
        };

        let human_text = format!(
            "Beat analysis complete: {} BPM, {} beats, {} downbeats\nArtifact: {}",
            service_response.bpm.round(),
            service_response.num_beats,
            service_response.num_downbeats,
            artifact_id.as_str()
        );

        Ok(CallToolResult::success(vec![Content::text(human_text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }
}

fn validate_wav_format(data: &[u8]) -> Result<(), McpError> {
    if data.len() < 44 {
        return Err(McpError::invalid_params("File too small to be a valid WAV"));
    }

    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(McpError::invalid_params(
            "Not a valid WAV file (missing RIFF/WAVE header)",
        ));
    }

    let mut offset = 12;
    while offset + 8 < data.len() {
        let chunk_id = &data[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;

        if chunk_id == b"fmt " {
            if chunk_size < 16 || offset + 8 + chunk_size > data.len() {
                return Err(McpError::invalid_params("Invalid fmt chunk"));
            }

            let fmt_data = &data[offset + 8..];

            let audio_format = u16::from_le_bytes([fmt_data[0], fmt_data[1]]);
            if audio_format != 1 {
                return Err(McpError::invalid_params(format!(
                    "Unsupported audio format: {}. Only PCM (1) supported",
                    audio_format
                )));
            }

            let num_channels = u16::from_le_bytes([fmt_data[2], fmt_data[3]]);
            if num_channels != 1 {
                return Err(McpError::invalid_params(format!(
                    "Audio must be mono. Found {} channels",
                    num_channels
                )));
            }

            let sample_rate = u32::from_le_bytes([fmt_data[4], fmt_data[5], fmt_data[6], fmt_data[7]]);
            if sample_rate != REQUIRED_SAMPLE_RATE {
                return Err(McpError::invalid_params(format!(
                    "Sample rate must be {} Hz. Found {} Hz",
                    REQUIRED_SAMPLE_RATE, sample_rate
                )));
            }

            return Ok(());
        }

        offset += 8 + chunk_size;
        if chunk_size % 2 == 1 {
            offset += 1;
        }
    }

    Err(McpError::invalid_params("WAV file missing fmt chunk"))
}
