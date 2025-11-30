use crate::api::schema::{OrpheusGenerateRequest, OrpheusGenerateSeededRequest, OrpheusContinueRequest, OrpheusBridgeRequest};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::local_models::OrpheusGenerateParams;
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use baton::{ErrorData as McpError, CallToolResult, Content};
use std::sync::Arc;
use tracing;

/// Look up an artifact ID by its content hash
fn find_artifact_by_hash<S: ArtifactStore>(store: &S, content_hash: &str) -> Option<String> {
    store.all().ok()?.into_iter()
        .find(|a| a.content_hash.as_str() == content_hash)
        .map(|a| a.id.as_str().to_string())
}

impl EventDualityServer {
    // Helper function to validate sampling parameters
    fn validate_sampling_params(temperature: Option<f32>, top_p: Option<f32>) -> Result<(), McpError> {
        if let Some(temp) = temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(McpError::invalid_params(
                    format!("temperature must be 0.0-2.0, got {}", temp)
                ));
            }
        }
        if let Some(p) = top_p {
            if !(0.0..=1.0).contains(&p) {
                return Err(McpError::invalid_params(
                    format!("top_p must be 0.0-1.0, got {}", p)
                ));
            }
        }
        Ok(())
    }

    #[tracing::instrument(
        name = "mcp.tool.orpheus_generate",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.temperature = request.temperature,
            model.num_variations = request.num_variations,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn orpheus_generate(
        &self,
        request: OrpheusGenerateRequest,
    ) -> Result<CallToolResult, McpError> {
        // Validate parameters upfront
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        // Create job
        let job_id = self.job_store.create_job("orpheus_generate".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        // Clone everything needed for the background task
        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        // Spawn background task
        let handle = tokio::spawn(async move {
            // Mark as running
            let _ = job_store.mark_running(&job_id_clone);

            let params = OrpheusGenerateParams {
                temperature: request.temperature,
                top_p: request.top_p,
                max_tokens: request.max_tokens,
                num_variations: request.num_variations,
            };

            let model = request.model.unwrap_or_else(|| "base".to_string());

            // Do the work
            match local_models.run_orpheus_generate(
                model.clone(),
                "generate".to_string(),
                None,  // No input for from-scratch generation
                params,
                Some(job_id_clone.as_str().to_string())
            ).await {
                Ok(result) => {
                    // Create artifacts (need to handle errors gracefully)
                    let artifacts_result = (|| -> anyhow::Result<Vec<Artifact>> {
                        let mut artifacts = Vec::new();
                        let store = artifact_store.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                        for (i, hash) in result.output_hashes.iter().enumerate() {
                            let tokens = result.num_tokens.get(i).copied().map(|t| t as u32);
                            let content_hash = ContentHash::new(hash);
                            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                            let creator = request.creator.clone().unwrap_or_else(|| "agent_orpheus".to_string());

                            let mut artifact = Artifact::new(
                                artifact_id,
                                content_hash,
                                &creator,
                                serde_json::json!({
                                    "type": "orpheus_generation",
                                    "task": "generate",
                                    "model": {
                                        "name": model,
                                    },
                                    "params": {
                                        "temperature": request.temperature,
                                        "top_p": request.top_p,
                                        "max_tokens": request.max_tokens,
                                        "num_variations": request.num_variations,
                                    },
                                    "generation": {
                                        "tokens": tokens,
                                        "job_id": job_id_clone.as_str(),
                                    },
                                })
                            )
                            .with_tags(vec![
                                "type:midi",
                                "source:orpheus",
                                "tool:orpheus_generate"
                            ]);

                            if let Some(ref set_id) = request.variation_set_id {
                                let index = store.next_variation_index(set_id)?;
                                artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
                            }

                            if let Some(ref parent_id) = request.parent_id {
                                artifact = artifact.with_parent(ArtifactId::new(parent_id));
                            }

                            artifact = artifact.with_tags(request.tags.clone());

                            store.put(artifact.clone())?;
                            artifacts.push(artifact);
                        }

                        store.flush()?;
                        Ok(artifacts)
                    })();

                    match artifacts_result {
                        Ok(artifacts) => {
                            let response = serde_json::json!({
                                "status": result.status,
                                "output_hashes": result.output_hashes,
                                "artifact_ids": artifacts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
                                "summary": result.summary,
                                "variation_set_id": artifacts.first().and_then(|a| a.variation_set_id.as_ref().map(|s| s.as_str())),
                                "variation_indices": artifacts.iter().map(|a| a.variation_index).collect::<Vec<_>>(),
                            });

                            let _ = job_store.mark_complete(&job_id_clone, response);
                        }
                        Err(e) => {
                            let _ = job_store.mark_failed(&job_id_clone, format!("Failed to create artifacts: {}", e));
                        }
                    }
                }
                Err(e) => {
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        // Store handle for potential cancellation
        self.job_store.store_handle(&job_id, handle);

        // Return job ID immediately
        let response = serde_json::json!({
            "job_id": job_id.as_str(),
            "status": "pending",
            "message": "Generation started. Use get_job_status() or wait_for_job() to retrieve results."
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    #[tracing::instrument(
        name = "mcp.tool.orpheus_generate_seeded",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.seed_hash = %request.seed_hash,
            model.temperature = request.temperature,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn orpheus_generate_seeded(
        &self,
        request: OrpheusGenerateSeededRequest,
    ) -> Result<CallToolResult, McpError> {
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        let job_id = self.job_store.create_job("orpheus_generate_seeded".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let params = OrpheusGenerateParams {
                temperature: request.temperature,
                top_p: request.top_p,
                max_tokens: request.max_tokens,
                num_variations: request.num_variations,
            };

            let model = request.model.unwrap_or_else(|| "base".to_string());

            // Clone seed_hash for use in artifact metadata
            let seed_hash_for_metadata = request.seed_hash.clone();

            match local_models.run_orpheus_generate(
                model.clone(),
                "generate_seeded".to_string(),
                Some(request.seed_hash),
                params,
                Some(job_id_clone.as_str().to_string())
            ).await {
                Ok(result) => {
                    let artifacts_result = (|| -> anyhow::Result<Vec<Artifact>> {
                        let mut artifacts = Vec::new();
                        let store = artifact_store.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                        // Look up seed artifact ID
                        let seed_artifact_id = find_artifact_by_hash(&*store, &seed_hash_for_metadata);

                        for (i, hash) in result.output_hashes.iter().enumerate() {
                            let tokens = result.num_tokens.get(i).copied().map(|t| t as u32);
                            let content_hash = ContentHash::new(hash);
                            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                            let creator = request.creator.clone().unwrap_or_else(|| "agent_orpheus".to_string());

                            let mut artifact = Artifact::new(
                                artifact_id,
                                content_hash,
                                &creator,
                                serde_json::json!({
                                    "type": "orpheus_generation",
                                    "task": "generate_seeded",
                                    "model": {
                                        "name": model,
                                    },
                                    "params": {
                                        "temperature": request.temperature,
                                        "top_p": request.top_p,
                                        "max_tokens": request.max_tokens,
                                        "num_variations": request.num_variations,
                                    },
                                    "seed": {
                                        "hash": seed_hash_for_metadata,
                                        "artifact_id": seed_artifact_id,
                                    },
                                    "generation": {
                                        "tokens": tokens,
                                        "job_id": job_id_clone.as_str(),
                                    },
                                })
                            )
                            .with_tags(vec!["type:midi", "source:orpheus", "tool:orpheus_generate_seeded"]);

                            if let Some(ref set_id) = request.variation_set_id {
                                let index = store.next_variation_index(set_id)?;
                                artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
                            }

                            if let Some(ref parent_id) = request.parent_id {
                                artifact = artifact.with_parent(ArtifactId::new(parent_id));
                            }

                            artifact = artifact.with_tags(request.tags.clone());
                            store.put(artifact.clone())?;
                            artifacts.push(artifact);
                        }

                        store.flush()?;
                        Ok(artifacts)
                    })();

                    match artifacts_result {
                        Ok(artifacts) => {
                            let response = serde_json::json!({
                                "status": result.status,
                                "output_hashes": result.output_hashes,
                                "artifact_ids": artifacts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
                                "summary": result.summary,
                                "variation_set_id": artifacts.first().and_then(|a| a.variation_set_id.as_ref().map(|s| s.as_str())),
                                "variation_indices": artifacts.iter().map(|a| a.variation_index).collect::<Vec<_>>(),
                            });
                            let _ = job_store.mark_complete(&job_id_clone, response);
                        }
                        Err(e) => {
                            let _ = job_store.mark_failed(&job_id_clone, format!("Failed to create artifacts: {}", e));
                        }
                    }
                }
                Err(e) => {
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        self.job_store.store_handle(&job_id, handle);

        let response = serde_json::json!({
            "job_id": job_id.as_str(),
            "status": "pending",
            "message": "Seeded generation started. Use get_job_status() or wait_for_job() to retrieve results."
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    #[tracing::instrument(
        name = "mcp.tool.orpheus_continue",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.input_hash = %request.input_hash,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn orpheus_continue(
        &self,
        request: OrpheusContinueRequest,
    ) -> Result<CallToolResult, McpError> {
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        let job_id = self.job_store.create_job("orpheus_continue".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let params = OrpheusGenerateParams {
                temperature: request.temperature,
                top_p: request.top_p,
                max_tokens: request.max_tokens,
                num_variations: request.num_variations,
            };

            let model = request.model.unwrap_or_else(|| "base".to_string());

            // Clone input_hash for use in artifact metadata
            let input_hash_for_metadata = request.input_hash.clone();

            match local_models.run_orpheus_generate(
                model.clone(),
                "continue".to_string(),
                Some(request.input_hash),
                params,
                Some(job_id_clone.as_str().to_string())
            ).await {
                Ok(result) => {
                    let artifacts_result = (|| -> anyhow::Result<Vec<Artifact>> {
                        let mut artifacts = Vec::new();
                        let store = artifact_store.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                        // Look up input artifact ID
                        let input_artifact_id = find_artifact_by_hash(&*store, &input_hash_for_metadata);

                        for (i, hash) in result.output_hashes.iter().enumerate() {
                            let tokens = result.num_tokens.get(i).copied().map(|t| t as u32);
                            let content_hash = ContentHash::new(hash);
                            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                            let creator = request.creator.clone().unwrap_or_else(|| "agent_orpheus".to_string());

                            let mut artifact = Artifact::new(
                                artifact_id,
                                content_hash,
                                &creator,
                                serde_json::json!({
                                    "type": "orpheus_generation",
                                    "task": "continue",
                                    "model": {
                                        "name": model,
                                    },
                                    "params": {
                                        "temperature": request.temperature,
                                        "top_p": request.top_p,
                                        "max_tokens": request.max_tokens,
                                        "num_variations": request.num_variations,
                                    },
                                    "continuation": {
                                        "input_hash": input_hash_for_metadata,
                                        "input_artifact_id": input_artifact_id,
                                    },
                                    "generation": {
                                        "tokens": tokens,
                                        "job_id": job_id_clone.as_str(),
                                    },
                                })
                            )
                            .with_tags(vec!["type:midi", "source:orpheus", "tool:orpheus_continue"]);

                            if let Some(ref set_id) = request.variation_set_id {
                                let index = store.next_variation_index(set_id)?;
                                artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
                            }

                            if let Some(ref parent_id) = request.parent_id {
                                artifact = artifact.with_parent(ArtifactId::new(parent_id));
                            }

                            artifact = artifact.with_tags(request.tags.clone());
                            store.put(artifact.clone())?;
                            artifacts.push(artifact);
                        }

                        store.flush()?;
                        Ok(artifacts)
                    })();

                    match artifacts_result {
                        Ok(artifacts) => {
                            let response = serde_json::json!({
                                "status": result.status,
                                "output_hashes": result.output_hashes,
                                "artifact_ids": artifacts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
                                "summary": result.summary,
                            });
                            let _ = job_store.mark_complete(&job_id_clone, response);
                        }
                        Err(e) => {
                            let _ = job_store.mark_failed(&job_id_clone, format!("Failed to create artifacts: {}", e));
                        }
                    }
                }
                Err(e) => {
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        self.job_store.store_handle(&job_id, handle);

        let response = serde_json::json!({
            "job_id": job_id.as_str(),
            "status": "pending",
            "message": "Continuation started. Use get_job_status() or wait_for_job() to retrieve results."
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    #[tracing::instrument(
        name = "mcp.tool.orpheus_bridge",
        skip(self, request),
        fields(
            model.name = ?request.model,
            model.section_a_hash = %request.section_a_hash,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn orpheus_bridge(
        &self,
        request: OrpheusBridgeRequest,
    ) -> Result<CallToolResult, McpError> {
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        let job_id = self.job_store.create_job("orpheus_bridge".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            match local_models.run_orpheus_bridge(
                request.section_a_hash.clone(),
                request.section_b_hash.clone(),
                request.temperature,
                request.top_p,
                request.max_tokens,
                Some(job_id_clone.as_str().to_string()),
            ).await {
                Ok(result) => {
                    let artifacts_result = (|| -> anyhow::Result<Vec<Artifact>> {
                        let mut artifacts = Vec::new();
                        let store = artifact_store.write()
                            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                        for (i, hash) in result.output_hashes.iter().enumerate() {
                            let tokens = result.num_tokens.get(i).copied().map(|t| t as u32);
                            let content_hash = ContentHash::new(hash);
                            let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                            let creator = request.creator.clone()
                                .unwrap_or_else(|| "agent_orpheus".to_string());

                            let mut artifact = Artifact::new(
                                artifact_id,
                                content_hash.clone(),
                                &creator,
                                serde_json::json!({
                                    "type": "orpheus_generation",
                                    "task": "bridge",
                                    "section_a": request.section_a_hash,
                                    "section_b": request.section_b_hash,
                                    "generation": {
                                        "tokens": tokens,
                                        "job_id": job_id_clone.as_str(),
                                    },
                                })
                            )
                            .with_tags(vec![
                                "type:midi",
                                "source:orpheus",
                                "tool:orpheus_bridge",
                            ]);

                            // Link to section_a as parent
                            artifact = artifact.with_parent(
                                ArtifactId::from_hash_prefix(&ContentHash::new(&request.section_a_hash))
                            );

                            if let Some(ref set_id) = request.variation_set_id {
                                let index = store.next_variation_index(set_id)?;
                                artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
                            }

                            if let Some(ref parent_id) = request.parent_id {
                                artifact = artifact.with_parent(ArtifactId::new(parent_id));
                            }

                            artifact = artifact.with_tags(request.tags.clone());
                            store.put(artifact.clone())?;
                            artifacts.push(artifact);
                        }

                        store.flush()?;
                        Ok(artifacts)
                    })();

                    match artifacts_result {
                        Ok(artifacts) => {
                            let response = serde_json::json!({
                                "status": result.status,
                                "output_hashes": result.output_hashes,
                                "artifact_ids": artifacts.iter()
                                    .map(|a| a.id.as_str()).collect::<Vec<_>>(),
                                "summary": result.summary,
                            });
                            let _ = job_store.mark_complete(&job_id_clone, response);
                        }
                        Err(e) => {
                            let _ = job_store.mark_failed(&job_id_clone,
                                format!("Failed to create artifacts: {}", e));
                        }
                    }
                }
                Err(e) => {
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        self.job_store.store_handle(&job_id, handle);

        let response = serde_json::json!({
            "job_id": job_id.as_str(),
            "status": "pending",
            "message": "Bridge generation started. Use job_poll() to retrieve results."
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }
}
