use crate::api::responses::{JobSpawnResponse, JobStatus};
use crate::api::schema::YueGenerateRequest;
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use hooteproto::{ToolOutput, ToolResult};
use std::sync::Arc;
use tracing;

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.yue_generate",
        skip(self, request),
        fields(
            genre = %request.genre.as_deref().unwrap_or("Pop"),
            lyrics_len = request.lyrics.len(),
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn yue_generate(
        &self,
        request: YueGenerateRequest,
    ) -> ToolResult {
        let job_id = self.job_store.create_job("yue_generate".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let lyrics = request.lyrics.clone();
        let genre = request.genre.clone();
        let max_new_tokens = request.max_new_tokens;
        let run_n_segments = request.run_n_segments;
        let seed = request.seed;
        let creator = request.creator.clone();
        let variation_set_id = request.variation_set_id.clone();
        let parent_id = request.parent_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let result: anyhow::Result<JobSpawnResponse> = (async {
                let response = local_models.run_yue_generate(
                    lyrics,
                    genre.unwrap_or_else(|| "Pop".to_string()),
                    max_new_tokens.unwrap_or(3000),
                    run_n_segments.unwrap_or(2),
                    seed.unwrap_or(42),
                    Some(job_id_clone.as_str().to_string()),
                ).await?;

                if let Some(error) = response.get("error") {
                    let error_msg = error.as_str().unwrap_or("Unknown error");
                    return Err(anyhow::anyhow!("YuE generation failed: {}", error_msg));
                }

                let audio_b64 = response.get("audio_base64")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("No audio_base64 in YuE response"))?;

                use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
                let audio_bytes = BASE64.decode(audio_b64)?;

                let format = response.get("format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("wav");
                let mime_type = match format {
                    "mp3" => "audio/mpeg",
                    "wav" => "audio/wav",
                    _ => "audio/wav",
                };

                let audio_hash = local_models.store_cas_content(&audio_bytes, mime_type).await?;

                let content_hash = ContentHash::new(&audio_hash);
                let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

                let lyrics_clone = response.get("lyrics")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let genre_value = response.get("genre")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let mut artifact = Artifact::new(
                    artifact_id.clone(),
                    content_hash,
                    creator.unwrap_or_else(|| "unknown".to_string()),
                    serde_json::json!({
                        "type": "yue_generation",
                        "lyrics": lyrics_clone,
                        "params": {
                            "genre": genre_value,
                            "max_new_tokens": max_new_tokens,
                            "run_n_segments": run_n_segments,
                            "seed": seed,
                        },
                        "output": {
                            "format": format,
                        }
                    })
                ).with_tags(vec![
                    "type:audio",
                    &format!("format:{}", format),
                    "source:yue",
                    "tool:yue_generate",
                    "has:vocals",
                ]);

                if let Some(set_id) = variation_set_id {
                    let next_idx = {
                        let store = artifact_store.write()
                            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
                        store.next_variation_index(&set_id)?
                    };
                    artifact = artifact.with_variation_set(VariationSetId::new(set_id), next_idx);
                }

                if let Some(parent_id) = parent_id {
                    artifact = artifact.with_parent(ArtifactId::new(parent_id));
                }

                {
                    let store = artifact_store.write()
                        .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
                    store.put(artifact)?;
                    store.flush()?;
                }

                Ok(JobSpawnResponse {
                    job_id: job_id_clone.as_str().to_string(),
                    status: JobStatus::Completed,
                    artifact_id: Some(artifact_id.as_str().to_string()),
                    content_hash: Some(audio_hash.clone()),
                    message: Some(format!("Generated song with vocals ({})", format)),
                })
            }).await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, serde_json::to_value(response).unwrap());
                }
                Err(e) => {
                    tracing::error!(error = %e, "YuE generation failed");
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
            message: Some("YuE song generation started (this may take several minutes)...".to_string()),
        };

        Ok(ToolOutput::new(format!("Started YuE job: {}", job_id.as_str()), &response))
    }
}
