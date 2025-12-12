use crate::api::responses::{JobSpawnResponse, JobStatus};
use crate::api::schema::MusicgenGenerateRequest;
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use hooteproto::{ToolOutput, ToolResult};
use std::sync::Arc;
use tracing;

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.musicgen_generate",
        skip(self, request),
        fields(
            prompt = %request.prompt.as_deref().unwrap_or(""),
            duration = ?request.duration,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn musicgen_generate(
        &self,
        request: MusicgenGenerateRequest,
    ) -> ToolResult {
        let job_id = self.job_store.create_job("musicgen_generate".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let prompt = request.prompt.clone();
        let duration = request.duration;
        let temperature = request.temperature;
        let top_k = request.top_k;
        let top_p = request.top_p;
        let guidance_scale = request.guidance_scale;
        let do_sample = request.do_sample;
        let creator = request.creator.clone();
        let variation_set_id = request.variation_set_id.clone();
        let parent_id = request.parent_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let result: anyhow::Result<JobSpawnResponse> = (async {
                let prompt_value = prompt.clone();

                let response = local_models.run_musicgen_generate(
                    prompt_value.unwrap_or_default(),
                    duration.unwrap_or(10.0),
                    temperature.unwrap_or(1.0),
                    top_k.unwrap_or(250),
                    top_p.unwrap_or(0.9),
                    guidance_scale.unwrap_or(3.0),
                    do_sample.unwrap_or(true),
                    Some(job_id_clone.as_str().to_string()),
                ).await?;

                let audio_b64 = response.get("audio_base64")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("No audio_base64 in MusicGen response"))?;

                use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
                let audio_bytes = BASE64.decode(audio_b64)?;

                let audio_hash = local_models.store_cas_content(&audio_bytes, "audio/wav").await?;

                let content_hash = ContentHash::new(&audio_hash);
                let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

                let duration_secs = response.get("duration")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                let sample_rate = response.get("sample_rate")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(32000);

                let mut artifact = Artifact::new(
                    artifact_id.clone(),
                    content_hash,
                    creator.unwrap_or_else(|| "unknown".to_string()),
                    serde_json::json!({
                        "type": "musicgen_generation",
                        "prompt": prompt,
                        "params": {
                            "duration": duration,
                            "temperature": temperature,
                            "top_k": top_k,
                            "top_p": top_p,
                            "guidance_scale": guidance_scale,
                            "do_sample": do_sample,
                        },
                        "output": {
                            "duration_seconds": duration_secs,
                            "sample_rate": sample_rate,
                            "format": "wav",
                        }
                    })
                ).with_tags(vec!["type:audio", "format:wav", "source:musicgen", "tool:musicgen_generate"]);

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
                    message: Some(format!("Generated {:.1}s of music", duration_secs)),
                })
            }).await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, serde_json::to_value(response).unwrap());
                }
                Err(e) => {
                    tracing::error!(error = %e, "MusicGen generation failed");
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
            message: Some("MusicGen generation started...".to_string()),
        };

        Ok(ToolOutput::new(format!("Started MusicGen job: {}", job_id.as_str()), &response))
    }
}
