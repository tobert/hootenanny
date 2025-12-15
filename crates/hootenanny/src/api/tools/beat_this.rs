use crate::api::responses::BeatthisAnalyzeResponse;
use crate::api::schema::{AnalyzeBeatsRequest, BeatThisServiceRequest, BeatThisServiceResponse};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash};
use hooteproto::{ToolOutput, ToolResult, ToolError};
use base64::{engine::general_purpose, Engine as _};
use rubato::{Resampler, SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};
use std::io::Cursor;

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
    ) -> ToolResult {
        let span = tracing::Span::current();

        let source_audio_hash: Option<String>;

        let audio_bytes = match (&request.audio_path, &request.audio_hash) {
            (Some(path), None) => {
                span.record("audio.source", "file");
                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|e| ToolError::internal(format!("Failed to read audio file: {}", e)))?;
                source_audio_hash = Some(self.local_models.store_cas_content(&bytes, "audio/wav")
                    .await
                    .map_err(|e| ToolError::internal(format!("Failed to store audio in CAS: {}", e)))?);
                bytes
            }
            (None, Some(hash)) => {
                span.record("audio.source", "cas");
                source_audio_hash = Some(hash.clone());
                let cas_ref = self
                    .local_models
                    .inspect_cas_content(hash)
                    .await
                    .map_err(|e| ToolError::internal(format!("Failed to get audio from CAS: {}", e)))?;
                let local_path = cas_ref
                    .local_path
                    .ok_or_else(|| ToolError::internal("Audio not found in local CAS"))?;
                tokio::fs::read(&local_path)
                    .await
                    .map_err(|e| ToolError::internal(format!("Failed to read CAS audio: {}", e)))?
            }
            (Some(_), Some(_)) => {
                return Err(ToolError::invalid_params(
                    "Provide either audio_path or audio_hash, not both",
                ));
            }
            (None, None) => {
                return Err(ToolError::invalid_params(
                    "Either audio_path or audio_hash is required",
                ));
            }
        };

        span.record("audio.size_bytes", audio_bytes.len());

        let prepared_audio = prepare_audio_for_beatthis(&audio_bytes)?;

        let audio_base64 = general_purpose::STANDARD.encode(&prepared_audio);

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
                    ToolError::internal(
                        "beat-this service not running. Start with: just start beat-this",
                    )
                } else if e.is_timeout() {
                    ToolError::internal("beat-this request timed out (30s limit)")
                } else {
                    ToolError::internal(format!("beat-this request failed: {}", e))
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                422 => ToolError::invalid_params(format!(
                    "Invalid audio format: {}. Required: 22050 Hz mono WAV, ≤30s",
                    error_text
                )),
                429 => ToolError::internal(
                    "beat-this service busy (already processing another request)",
                ),
                _ => ToolError::internal(format!(
                    "beat-this error ({}): {}",
                    status, error_text
                )),
            });
        }

        let service_response: BeatThisServiceResponse = response
            .json()
            .await
            .map_err(|e| ToolError::internal(format!("Failed to parse response: {}", e)))?;

        span.record("beats.count", service_response.num_beats);
        span.record("beats.bpm", service_response.bpm);

        let beats_per_measure = if service_response.num_downbeats > 0 {
            (service_response.num_beats as f64 / service_response.num_downbeats as f64).round()
                as usize
        } else {
            4
        };

        let source_artifact_id = if let Some(ref hash) = source_audio_hash {
            let store = self.artifact_store.read()
                .map_err(|_| ToolError::internal("Lock poisoned"))?;
            find_artifact_by_hash(&*store, hash)
        } else {
            None
        };

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

        let analysis_json = serde_json::to_string(&analysis_data)
            .map_err(|e| ToolError::internal(format!("Failed to serialize analysis: {}", e)))?;
        let analysis_hash = self.local_models.store_cas_content(analysis_json.as_bytes(), "application/json")
            .await
            .map_err(|e| ToolError::internal(format!("Failed to store analysis in CAS: {}", e)))?;

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

        let store = self.artifact_store.write()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;
        store.put(artifact)
            .map_err(|e| ToolError::internal(format!("Failed to store artifact: {}", e)))?;
        store.flush()
            .map_err(|e| ToolError::internal(format!("Failed to flush artifact store: {}", e)))?;

        let response = BeatthisAnalyzeResponse {
            beats: service_response.beats.clone(),
            downbeats: service_response.downbeats.clone(),
            estimated_bpm: service_response.bpm,
            confidence: 0.95,
        };

        let human_text = format!(
            "Beat analysis complete: {} BPM, {} beats, {} downbeats\nArtifact: {}",
            service_response.bpm.round(),
            service_response.num_beats,
            service_response.num_downbeats,
            artifact_id.as_str()
        );

        Ok(ToolOutput::new(human_text, &response))
    }
}

/// Prepare audio for BeatThis: convert to mono 22050 Hz WAV
///
/// Handles:
/// - Stereo → mono conversion (averages channels)
/// - Sample rate conversion via rubato (high-quality sinc resampling)
/// - Returns WAV bytes ready for the service
fn prepare_audio_for_beatthis(data: &[u8]) -> Result<Vec<u8>, ToolError> {
    let cursor = Cursor::new(data);
    let reader = hound::WavReader::new(cursor)
        .map_err(|e| ToolError::invalid_params(format!("Invalid WAV file: {}", e)))?;

    let spec = reader.spec();
    let source_rate = spec.sample_rate;
    let channels = spec.channels as usize;

    tracing::debug!(
        source_rate,
        channels,
        bits = spec.bits_per_sample,
        "Reading WAV for beat analysis"
    );

    // Read samples as f32
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.into_samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ToolError::internal(format!("Failed to read samples: {}", e)))?
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1i32 << (bits - 1)) as f32;
            reader.into_samples::<i32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ToolError::internal(format!("Failed to read samples: {}", e)))?
                .into_iter()
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    // Convert to mono if stereo (average channels)
    let mono_samples: Vec<f32> = if channels > 1 {
        samples
            .chunks(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    // Resample if needed
    let final_samples = if source_rate != REQUIRED_SAMPLE_RATE {
        tracing::info!(
            from = source_rate,
            to = REQUIRED_SAMPLE_RATE,
            "Resampling audio for BeatThis"
        );
        resample_audio(&mono_samples, source_rate, REQUIRED_SAMPLE_RATE)?
    } else {
        mono_samples
    };

    // Write output WAV
    let mut output = Cursor::new(Vec::new());
    let out_spec = hound::WavSpec {
        channels: 1,
        sample_rate: REQUIRED_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::new(&mut output, out_spec)
        .map_err(|e| ToolError::internal(format!("Failed to create WAV writer: {}", e)))?;

    for sample in final_samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let int_sample = (clamped * 32767.0) as i16;
        writer.write_sample(int_sample)
            .map_err(|e| ToolError::internal(format!("Failed to write sample: {}", e)))?;
    }

    writer.finalize()
        .map_err(|e| ToolError::internal(format!("Failed to finalize WAV: {}", e)))?;

    Ok(output.into_inner())
}

/// Resample audio using rubato's high-quality sinc interpolation
fn resample_audio(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, ToolError> {
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let ratio = to_rate as f64 / from_rate as f64;

    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        2.0,  // max relative ratio (allows some flexibility)
        params,
        samples.len(),
        1,    // mono
    ).map_err(|e| ToolError::internal(format!("Failed to create resampler: {}", e)))?;

    let input = vec![samples.to_vec()];
    let output = resampler.process(&input, None)
        .map_err(|e| ToolError::internal(format!("Resampling failed: {}", e)))?;

    Ok(output.into_iter().next().unwrap_or_default())
}
