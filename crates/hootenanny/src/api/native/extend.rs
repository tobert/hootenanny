//! Extend tool - continue existing content across supported spaces.
//!
//! This implements the model-native `extend()` API that continues/extends existing
//! content using the appropriate continuation method for each space.

use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::local_models::OrpheusGenerateParams;
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use hooteproto::responses::{OrpheusGeneratedResponse, ToolResponse};
use hooteproto::{Encoding, Space, ToolError};
use std::sync::Arc;
use tracing;

// Re-export from hooteproto for backwards compatibility
pub use hooteproto::request::ExtendRequest;

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
        name = "mcp.tool.extend",
        skip(self, request),
        fields(
            space = ?request.space,
            num_variations = request.num_variations,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn extend(&self, request: ExtendRequest) -> Result<ToolResponse, ToolError> {
        // Resolve encoding to content hash
        let (content_hash, source_artifact_id) = {
            let store = self
                .artifact_store
                .read()
                .map_err(|_| ToolError::internal("Lock poisoned".to_string()))?;

            match &request.encoding {
                Encoding::Hash { content_hash, format } => {
                    if !format.contains("midi") {
                        return Err(ToolError::validation(
                            "unsupported_format",
                            format!("extend() only supports MIDI content, got: {}", format),
                        ));
                    }
                    (content_hash.clone(), None)
                }
                Encoding::Midi { artifact_id } => {
                    let hash = artifact_to_hash(&*store, artifact_id).ok_or_else(|| {
                        ToolError::validation(
                            "artifact_not_found",
                            format!("Artifact {} not found", artifact_id),
                        )
                    })?;
                    (hash, Some(artifact_id.clone()))
                }
                Encoding::Audio { .. } => {
                    return Err(ToolError::validation(
                        "unsupported_encoding",
                        "extend() does not support audio encoding".to_string(),
                    ));
                }
                Encoding::Abc { .. } => {
                    return Err(ToolError::validation(
                        "unsupported_encoding",
                        "extend() does not support ABC encoding".to_string(),
                    ));
                }
            }
        };

        // Determine space: use explicit space or default to Orpheus
        let space = request.space.unwrap_or(Space::Orpheus);

        // Validate that the space supports continuation
        if !space.supports_continuation() {
            return Err(ToolError::validation(
                "unsupported_space",
                format!("{:?} does not support continuation", space),
            ));
        }

        // Validate inference context
        request
            .inference
            .validate()
            .map_err(|e| ToolError::validation("invalid_inference", e.to_string()))?;

        let job_id = self.job_store.create_job("extend".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let broadcaster = self.broadcaster.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let result: anyhow::Result<ToolResponse> = (async {
                let (variant, temperature, top_p, max_tokens) = request.inference.to_orpheus_params();
                let model = variant.or_else(|| space.model_variant().map(String::from));
                let model_name = model.clone().unwrap_or_else(|| "base".to_string());

                let params = OrpheusGenerateParams {
                    temperature,
                    top_p,
                    max_tokens,
                    num_variations: request.num_variations,
                };

                // Run orpheus continuation
                let orpheus_result = local_models
                    .run_orpheus_generate(
                        model_name.clone(),
                        "continue".to_string(),
                        Some(content_hash.clone()),
                        params,
                        Some(job_id_clone.as_str().to_string()),
                    )
                    .await?;

                // Pre-calculate durations for all outputs (before acquiring write lock)
                let mut durations: Vec<Option<f64>> = Vec::new();
                for hash in &orpheus_result.output_hashes {
                    let dur = match local_models.inspect_cas_content(hash).await {
                        Ok(ref info) if info.local_path.is_some() => {
                            match tokio::fs::read(info.local_path.as_ref().unwrap()).await {
                                Ok(midi_bytes) => {
                                    crate::mcp_tools::rustysynth::calculate_midi_duration(&midi_bytes)
                                }
                                Err(_) => None,
                            }
                        }
                        _ => None,
                    };
                    durations.push(dur);
                }

                // Create artifacts
                let mut artifacts = Vec::new();
                let store = artifact_store
                    .write()
                    .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                for (i, hash) in orpheus_result.output_hashes.iter().enumerate() {
                    let tokens = orpheus_result.num_tokens.get(i).copied().map(|t| t as u32);
                    let output_hash = ContentHash::new(hash);
                    let artifact_id = ArtifactId::from_hash_prefix(&output_hash);
                    let creator = request
                        .creator
                        .clone()
                        .unwrap_or_else(|| "agent_orpheus".to_string());

                    let duration_seconds = durations.get(i).copied().flatten();

                    let metadata = serde_json::json!({
                        "type": "orpheus_generation",
                        "task": "continue",
                        "space": space,
                        "model": { "name": model_name },
                        "params": {
                            "temperature": temperature,
                            "top_p": top_p,
                            "max_tokens": max_tokens,
                            "num_variations": request.num_variations,
                        },
                        "continuation": {
                            "input_hash": content_hash,
                            "input_artifact_id": source_artifact_id,
                        },
                        "generation": {
                            "tokens": tokens,
                            "job_id": job_id_clone.as_str(),
                        },
                        "duration_seconds": duration_seconds,
                    });

                    let mut tags = vec![
                        "type:midi".to_string(),
                        "source:orpheus".to_string(),
                        "tool:extend".to_string(),
                    ];
                    tags.extend_from_slice(&request.tags);

                    let mut artifact = Artifact::new(artifact_id, output_hash, &creator, metadata)
                        .with_tags(tags.clone());

                    if let Some(ref set_id) = request.variation_set_id {
                        let index = store.next_variation_index(set_id)?;
                        artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
                    }

                    // Set parent_id to source artifact if available, otherwise use request parent_id
                    if let Some(ref src_artifact_id) = source_artifact_id {
                        artifact = artifact.with_parent(ArtifactId::new(src_artifact_id));
                    } else if let Some(ref parent_id) = request.parent_id {
                        artifact = artifact.with_parent(ArtifactId::new(parent_id));
                    }

                    store.put(artifact.clone())?;
                    artifacts.push(artifact);
                }

                store.flush()?;
                drop(store);

                // Broadcast artifact creation events
                if let Some(ref bc) = broadcaster {
                    for artifact in &artifacts {
                        let bc = bc.clone();
                        let id = artifact.id.as_str().to_string();
                        let hash = artifact.content_hash.as_str().to_string();
                        let tags = artifact.tags.clone();
                        let creator = Some(artifact.creator.clone());
                        tokio::spawn(async move {
                            let _ = bc.artifact_created(&id, &hash, tags, creator).await;
                        });
                    }
                }

                let tokens_per_variation: Vec<u64> = orpheus_result.num_tokens.iter().map(|&t| t as u64).collect();
                let total_tokens: u64 = tokens_per_variation.iter().sum();

                Ok(ToolResponse::OrpheusGenerated(OrpheusGeneratedResponse {
                    output_hashes: orpheus_result.output_hashes,
                    artifact_ids: artifacts.iter().map(|a| a.id.as_str().to_string()).collect(),
                    tokens_per_variation,
                    total_tokens,
                    variation_set_id: artifacts.first().and_then(|a| a.variation_set_id.as_ref().map(|s| s.as_str().to_string())),
                    summary: orpheus_result.summary,
                }))
            })
            .await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, response);
                }
                Err(e) => {
                    tracing::error!(error = %e, space = ?space, "Extension failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        self.job_store.store_handle(&job_id, handle);

        Ok(ToolResponse::job_started(job_id.as_str().to_string(), "extend"))
    }
}
