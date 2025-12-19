//! Analyze tool - unified analysis across content types.
//!
//! This implements the model-native `analyze()` API that dispatches to different
//! analysis models (Orpheus classifier, BeatThis, CLAP) based on the content type
//! and requested analysis tasks.

use crate::api::native::types::Encoding;
use crate::api::service::EventDualityServer;
use crate::artifact_store::ArtifactStore;
use hooteproto::{ToolError, ToolOutput, ToolResult};
use serde::{Deserialize, Serialize};
use tracing;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisTask {
    #[schemars(description = "Classify MIDI content (orpheus)")]
    Classify,

    #[schemars(description = "Detect beats in audio")]
    Beats,

    #[schemars(description = "Extract CLAP embeddings")]
    Embeddings,

    #[schemars(description = "Classify genre")]
    Genre,

    #[schemars(description = "Classify mood")]
    Mood,

    #[schemars(description = "Zero-shot classification with custom labels")]
    ZeroShot { labels: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeRequest {
    #[schemars(description = "Content to analyze")]
    pub encoding: Encoding,

    #[schemars(description = "Analysis tasks to run")]
    pub tasks: Vec<AnalysisTask>,
}

/// Look up an artifact by its ID and return the content hash
fn artifact_to_hash<S: ArtifactStore>(store: &S, artifact_id: &str) -> Option<String> {
    store
        .get(artifact_id)
        .ok()
        .flatten()
        .map(|a| a.content_hash.as_str().to_string())
}

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.analyze",
        skip(self, request),
        fields(
            encoding_type = ?request.encoding,
            num_tasks = request.tasks.len(),
        )
    )]
    pub async fn analyze(&self, request: AnalyzeRequest) -> ToolResult {
        if request.tasks.is_empty() {
            return Err(ToolError::validation(
                "invalid_params",
                "At least one analysis task is required",
            ));
        }

        // Resolve encoding to content hash
        let content_hash = match &request.encoding {
            Encoding::Hash { content_hash, .. } => content_hash.clone(),
            Encoding::Midi { artifact_id } | Encoding::Audio { artifact_id } => {
                let store = self
                    .artifact_store
                    .read()
                    .map_err(|_| ToolError::internal("Lock poisoned"))?;
                artifact_to_hash(&*store, artifact_id).ok_or_else(|| {
                    ToolError::validation(
                        "invalid_params",
                        format!("Artifact not found: {}", artifact_id),
                    )
                })?
            }
            Encoding::Abc { .. } => {
                return Err(ToolError::validation(
                    "invalid_params",
                    "ABC notation cannot be analyzed directly (convert to MIDI first)",
                ));
            }
        };

        let output_type = request.encoding.output_type();

        // Partition tasks by type
        let mut classify_tasks = Vec::new();
        let mut beat_tasks = Vec::new();
        let mut clap_tasks = Vec::new();

        for task in &request.tasks {
            match task {
                AnalysisTask::Classify => classify_tasks.push(task),
                AnalysisTask::Beats => beat_tasks.push(task),
                AnalysisTask::Embeddings | AnalysisTask::Genre | AnalysisTask::Mood => {
                    clap_tasks.push(task)
                }
                AnalysisTask::ZeroShot { .. } => clap_tasks.push(task),
            }
        }

        // Validate task compatibility with content type
        use crate::api::native::types::OutputType;
        match output_type {
            OutputType::Midi => {
                if !beat_tasks.is_empty() {
                    return Err(ToolError::validation(
                        "invalid_params",
                        "Beat analysis requires audio content, not MIDI",
                    ));
                }
                if !clap_tasks.is_empty() {
                    return Err(ToolError::validation(
                        "invalid_params",
                        "CLAP analysis requires audio content, not MIDI",
                    ));
                }
            }
            OutputType::Audio => {
                if !classify_tasks.is_empty() {
                    return Err(ToolError::validation(
                        "invalid_params",
                        "MIDI classification requires MIDI content, not audio",
                    ));
                }
            }
            OutputType::Symbolic => {
                return Err(ToolError::validation(
                    "invalid_params",
                    "Symbolic notation cannot be analyzed directly",
                ));
            }
        }

        // Run analysis tasks and aggregate results
        let mut results = serde_json::Map::new();

        // MIDI classification
        if !classify_tasks.is_empty() {
            let classification = self
                .local_models
                .run_orpheus_classifier(content_hash.clone())
                .await
                .map_err(|e| ToolError::internal(format!("Classification failed: {}", e)))?;

            results.insert("classification".to_string(), classification);
        }

        // Beat detection
        if !beat_tasks.is_empty() {
            use crate::api::schema::BeatThisServiceRequest;
            use base64::{engine::general_purpose, Engine as _};

            let cas_ref = self
                .local_models
                .inspect_cas_content(&content_hash)
                .await
                .map_err(|e| ToolError::internal(format!("Failed to inspect CAS: {}", e)))?;

            let audio_path = cas_ref
                .local_path
                .ok_or_else(|| ToolError::internal("Audio not found in local CAS"))?;

            let audio_bytes = tokio::fs::read(&audio_path)
                .await
                .map_err(|e| ToolError::internal(format!("Failed to read audio: {}", e)))?;

            let prepared_audio = crate::api::tools::beat_this::prepare_audio_for_beatthis(&audio_bytes)?;
            let audio_base64 = general_purpose::STANDARD.encode(&prepared_audio);

            let service_request = BeatThisServiceRequest {
                audio: audio_base64,
                client_job_id: None,
            };

            let client = reqwest::Client::new();
            let response = client
                .post("http://localhost:2012/predict")
                .json(&service_request)
                .timeout(std::time::Duration::from_secs(30))
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
                return Err(ToolError::internal(format!(
                    "beat-this error ({}): {}",
                    status, error_text
                )));
            }

            let beat_response: crate::api::schema::BeatThisServiceResponse = response
                .json()
                .await
                .map_err(|e| ToolError::internal(format!("Failed to parse response: {}", e)))?;

            let beats_per_measure = if beat_response.num_downbeats > 0 {
                (beat_response.num_beats as f64 / beat_response.num_downbeats as f64).round()
                    as usize
            } else {
                4
            };

            results.insert(
                "beats".to_string(),
                serde_json::json!({
                    "bpm": beat_response.bpm,
                    "num_beats": beat_response.num_beats,
                    "num_downbeats": beat_response.num_downbeats,
                    "duration_seconds": beat_response.duration,
                    "beats_per_measure": beats_per_measure,
                    "beat_times": beat_response.beats,
                    "downbeat_times": beat_response.downbeats,
                }),
            );
        }

        // CLAP analysis
        if !clap_tasks.is_empty() {
            let cas_ref = self
                .local_models
                .inspect_cas_content(&content_hash)
                .await
                .map_err(|e| ToolError::internal(format!("Failed to inspect CAS: {}", e)))?;

            let audio_path = cas_ref
                .local_path
                .ok_or_else(|| ToolError::internal("Audio not found in local CAS"))?;

            let audio_bytes = tokio::fs::read(&audio_path)
                .await
                .map_err(|e| ToolError::internal(format!("Failed to read audio: {}", e)))?;

            use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
            let audio_b64 = BASE64.encode(&audio_bytes);

            // Convert task enums to string task names for CLAP service
            let mut clap_task_names = Vec::new();
            let mut text_candidates = None;

            for task in &clap_tasks {
                match task {
                    AnalysisTask::Embeddings => clap_task_names.push("embeddings".to_string()),
                    AnalysisTask::Genre => clap_task_names.push("genre".to_string()),
                    AnalysisTask::Mood => clap_task_names.push("mood".to_string()),
                    AnalysisTask::ZeroShot { labels } => {
                        clap_task_names.push("zero_shot".to_string());
                        text_candidates = Some(labels.clone());
                    }
                    _ => {}
                }
            }

            let clap_result = self
                .local_models
                .run_clap_analyze(audio_b64, clap_task_names, None, text_candidates, None)
                .await
                .map_err(|e| ToolError::internal(format!("CLAP analysis failed: {}", e)))?;

            // Merge CLAP results into main results
            if let Some(obj) = clap_result.as_object() {
                for (key, value) in obj {
                    results.insert(key.clone(), value.clone());
                }
            }
        }

        // Build summary
        let mut summary_parts = Vec::new();

        if results.contains_key("classification") {
            summary_parts.push("classification".to_string());
        }
        if let Some(beats) = results.get("beats") {
            if let Some(bpm) = beats.get("bpm").and_then(|v| v.as_f64()) {
                summary_parts.push(format!("beats: {:.0} BPM", bpm));
            }
        }
        if results.contains_key("embeddings") {
            summary_parts.push("embeddings".to_string());
        }
        if let Some(genre) = results.get("genre") {
            if let Some(top_label) = genre.get("top_label").and_then(|v| v.as_str()) {
                summary_parts.push(format!("genre: {}", top_label));
            }
        }
        if let Some(mood) = results.get("mood") {
            if let Some(top_label) = mood.get("top_label").and_then(|v| v.as_str()) {
                summary_parts.push(format!("mood: {}", top_label));
            }
        }

        let summary = if summary_parts.is_empty() {
            "Analysis complete".to_string()
        } else {
            format!("Analysis: {}", summary_parts.join(", "))
        };

        let response_body = serde_json::json!({
            "content_hash": content_hash,
            "results": results,
            "summary": summary,
        });

        Ok(ToolOutput::new(summary, response_body))
    }
}
