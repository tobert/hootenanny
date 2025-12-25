//! Bridge tool - creates smooth transitions between MIDI sections.
//!
//! This implements the model-native `bridge()` API for creating transitions
//! between musical sections using the Orpheus bridge model.

use crate::api::native::types::{Encoding, InferenceContext};
use crate::api::responses::{JobSpawnResponse, JobStatus};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use hooteproto::responses::ToolResponse;
use hooteproto::{ToolError, ToolOutput, ToolResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing;

fn default_creator() -> Option<String> {
    Some("unknown".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BridgeRequest {
    #[schemars(description = "Starting content (section A)")]
    pub from: Encoding,

    #[schemars(description = "Target content (section B) - optional, for future A->B bridging")]
    pub to: Option<Encoding>,

    #[schemars(description = "Inference parameters")]
    #[serde(default)]
    pub inference: InferenceContext,

    #[schemars(description = "Variation set ID for grouping")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Tags for organizing")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

/// Look up an artifact by its ID and return the content hash
fn artifact_to_hash<S: ArtifactStore>(store: &S, artifact_id: &str) -> anyhow::Result<String> {
    store
        .get(artifact_id)?
        .map(|a| a.content_hash.as_str().to_string())
        .ok_or_else(|| anyhow::anyhow!("Artifact not found: {}", artifact_id))
}

/// Resolve an Encoding to a content hash
fn resolve_encoding<S: ArtifactStore>(
    store: &S,
    encoding: &Encoding,
) -> anyhow::Result<String> {
    match encoding {
        Encoding::Hash { content_hash, format } => {
            if !format.contains("midi") {
                return Err(anyhow::anyhow!(
                    "Bridge requires MIDI content, got format: {}",
                    format
                ));
            }
            Ok(content_hash.clone())
        }
        Encoding::Midi { artifact_id } => artifact_to_hash(store, artifact_id),
        Encoding::Audio { .. } => {
            Err(anyhow::anyhow!("Bridge does not support audio encoding"))
        }
        Encoding::Abc { .. } => {
            Err(anyhow::anyhow!("Bridge does not support ABC encoding"))
        }
    }
}

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.bridge",
        skip(self, request),
        fields(
            from.artifact_id = request.from.artifact_id(),
            to.artifact_id = request.to.as_ref().and_then(|e| e.artifact_id()),
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn bridge(&self, request: BridgeRequest) -> ToolResult {
        // Validate inference context
        request
            .inference
            .validate()
            .map_err(|e| ToolError::validation("invalid_inference", e.to_string()))?;

        let job_id = self.job_store.create_job("bridge".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let broadcaster = self.broadcaster.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let result: anyhow::Result<serde_json::Value> = (async {
                // Resolve section_a hash
                let section_a_hash = {
                    let store = artifact_store
                        .read()
                        .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                    resolve_encoding(&*store, &request.from)?
                };

                // Resolve section_b hash if provided
                let section_b_hash = if let Some(ref to) = request.to {
                    let store = artifact_store
                        .read()
                        .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                    Some(resolve_encoding(&*store, to)?)
                } else {
                    None
                };

                // Extract parent artifact ID from the `from` encoding
                let inferred_parent_id = request.from.artifact_id();

                // Get inference parameters
                let (_, temperature, top_p, max_tokens) = request.inference.to_orpheus_params();

                // Call orpheus bridge
                let orpheus_result = local_models
                    .run_orpheus_bridge(
                        section_a_hash.clone(),
                        section_b_hash.clone(),
                        temperature,
                        top_p,
                        max_tokens,
                        Some(job_id_clone.as_str().to_string()),
                    )
                    .await?;

                let mut artifacts = Vec::new();
                let store = artifact_store
                    .write()
                    .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                for (i, hash) in orpheus_result.output_hashes.iter().enumerate() {
                    let tokens = orpheus_result.num_tokens.get(i).copied().map(|t| t as u32);
                    let content_hash = ContentHash::new(hash);
                    let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                    let creator = request
                        .creator
                        .clone()
                        .unwrap_or_else(|| "agent_orpheus".to_string());

                    let mut tags = vec![
                        "type:midi".to_string(),
                        "source:orpheus".to_string(),
                        "tool:bridge".to_string(),
                    ];
                    tags.extend_from_slice(&request.tags);

                    let mut artifact = Artifact::new(
                        artifact_id,
                        content_hash,
                        &creator,
                        serde_json::json!({
                            "type": "orpheus_generation",
                            "task": "bridge",
                            "section_a": section_a_hash,
                            "section_b": section_b_hash,
                            "params": {
                                "temperature": temperature,
                                "top_p": top_p,
                                "max_tokens": max_tokens,
                            },
                            "generation": {
                                "tokens": tokens,
                                "job_id": job_id_clone.as_str(),
                            },
                        }),
                    )
                    .with_tags(tags);

                    // Set parent_id - use explicit parent_id if provided, otherwise use inferred
                    if let Some(ref parent_id) = request.parent_id {
                        artifact = artifact.with_parent(ArtifactId::new(parent_id));
                    } else if let Some(inferred_id) = inferred_parent_id {
                        artifact = artifact.with_parent(ArtifactId::new(inferred_id));
                    }

                    if let Some(ref set_id) = request.variation_set_id {
                        let index = store.next_variation_index(set_id)?;
                        artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
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

                Ok(serde_json::json!({
                    "status": orpheus_result.status,
                    "output_hashes": orpheus_result.output_hashes,
                    "artifact_ids": artifacts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
                    "summary": orpheus_result.summary,
                    "variation_set_id": artifacts.first().and_then(|a| a.variation_set_id.as_ref().map(|s| s.as_str())),
                    "variation_indices": artifacts.iter().map(|a| a.variation_index).collect::<Vec<_>>(),
                }))
            })
            .await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, ToolResponse::LegacyJson(response));
                }
                Err(e) => {
                    tracing::error!(error = %e, "Bridge generation failed");
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
            message: Some(
                "Bridge generation started. Use job_poll() to retrieve results.".to_string(),
            ),
        };

        Ok(ToolOutput::new(
            format!("Started bridge job: {}", job_id.as_str()),
            &response,
        ))
    }
}
