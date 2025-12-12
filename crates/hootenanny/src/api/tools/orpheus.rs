use crate::api::responses::{JobSpawnResponse, JobStatus};
use crate::api::schema::{OrpheusGenerateRequest, OrpheusGenerateSeededRequest, OrpheusContinueRequest, OrpheusBridgeRequest, OrpheusClassifyRequest, OrpheusLoopsRequest};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::local_models::OrpheusGenerateParams;
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use baton::{ErrorData as McpError, CallToolResult, Content};
use baton::protocol::ProgressSender;
use baton::types::progress::ProgressNotification;
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
        let broadcaster = self.broadcaster.clone();

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

        // Return job ID immediately with structured content
        let response = JobSpawnResponse {
            job_id: job_id.as_str().to_string(),
            status: JobStatus::Pending,
            artifact_id: None,
            content_hash: None,
            message: Some("Generation started. Use job_poll() to retrieve results.".to_string()),
        };

        Ok(CallToolResult::success(vec![Content::text(
            format!("Started job: {}", job_id.as_str())
        )])
        .with_structured(serde_json::to_value(&response).unwrap()))
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

        let response = JobSpawnResponse {
            job_id: job_id.as_str().to_string(),
            status: JobStatus::Pending,
            artifact_id: None,
            content_hash: None,
            message: Some("Seeded generation started. Use job_poll() to retrieve results.".to_string()),
        };

        Ok(CallToolResult::success(vec![Content::text(
            format!("Started job: {}", job_id.as_str())
        )])
        .with_structured(serde_json::to_value(&response).unwrap()))
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

        let response = JobSpawnResponse {
            job_id: job_id.as_str().to_string(),
            status: JobStatus::Pending,
            artifact_id: None,
            content_hash: None,
            message: Some("Continuation started. Use job_poll() to retrieve results.".to_string()),
        };

        Ok(CallToolResult::success(vec![Content::text(
            format!("Started job: {}", job_id.as_str())
        )])
        .with_structured(serde_json::to_value(&response).unwrap()))
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

        let response = JobSpawnResponse {
            job_id: job_id.as_str().to_string(),
            status: JobStatus::Pending,
            artifact_id: None,
            content_hash: None,
            message: Some("Bridge generation started. Use job_poll() to retrieve results.".to_string()),
        };

        Ok(CallToolResult::success(vec![Content::text(
            format!("Started job: {}", job_id.as_str())
        )])
        .with_structured(serde_json::to_value(&response).unwrap()))
    }

    // ========================================================================
    // Progress-aware versions of tools
    // ========================================================================

    /// Orpheus generate with progress notifications
    pub async fn orpheus_generate_with_progress(
        &self,
        request: OrpheusGenerateRequest,
        progress: Option<ProgressSender>,
    ) -> Result<CallToolResult, McpError> {
        // If no progress sender, fall back to regular version
        let Some(progress_tx) = progress else {
            return self.orpheus_generate(request).await;
        };

        // Get progress token from the context (we'll need to extract it)
        // For now, use a placeholder - the dispatch layer provides the actual token
        let progress_token = baton::types::progress::ProgressToken::String("progress".to_string());

        // Validate and create job (same as regular version)
        Self::validate_sampling_params(request.temperature, request.top_p)?;
        let job_id = self.job_store.create_job("orpheus_generate".to_string());

        // Clone for background task
        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let progress_token_clone = progress_token.clone();

        // Spawn background task with progress reporting
        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            // Send initial progress
            let _ = progress_tx.send(ProgressNotification::normalized(
                progress_token_clone.clone(),
                0.0,
                "Starting generation...",
            )).await;

            let params = OrpheusGenerateParams {
                temperature: request.temperature,
                top_p: request.top_p,
                max_tokens: request.max_tokens,
                num_variations: request.num_variations,
            };

            let model = request.model.unwrap_or_else(|| "base".to_string());

            // Progress: tokenizing
            let _ = progress_tx.send(ProgressNotification::normalized(
                progress_token_clone.clone(),
                0.25,
                "Tokenizing...",
            )).await;

            match local_models.run_orpheus_generate(
                model.clone(),
                "generate".to_string(),
                None,
                params,
                Some(job_id_clone.as_str().to_string())
            ).await {
                Ok(result) => {
                    // Progress: creating artifacts
                    let _ = progress_tx.send(ProgressNotification::normalized(
                        progress_token_clone.clone(),
                        0.75,
                        "Creating artifacts...",
                    )).await;

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
                                    "model": { "name": model },
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
                            // Progress: complete
                            let _ = progress_tx.send(ProgressNotification::normalized(
                                progress_token_clone,
                                1.0,
                                "Complete",
                            )).await;

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

        // Return job ID immediately with structured content
        let response = JobSpawnResponse {
            job_id: job_id.as_str().to_string(),
            status: JobStatus::Pending,
            artifact_id: None,
            content_hash: None,
            message: Some("Generation started with progress tracking.".to_string()),
        };

        Ok(CallToolResult::success(vec![Content::text(
            format!("Started job: {}", job_id.as_str())
        )])
        .with_structured(serde_json::to_value(&response).unwrap()))
    }

    /// Stub implementations for other progress-aware methods
    /// TODO: Implement full progress reporting for these
    pub async fn orpheus_generate_seeded_with_progress(
        &self,
        request: OrpheusGenerateSeededRequest,
        _progress: Option<ProgressSender>,
    ) -> Result<CallToolResult, McpError> {
        // TODO: Add progress notifications
        self.orpheus_generate_seeded(request).await
    }

    pub async fn orpheus_continue_with_progress(
        &self,
        request: OrpheusContinueRequest,
        _progress: Option<ProgressSender>,
    ) -> Result<CallToolResult, McpError> {
        // TODO: Add progress notifications
        self.orpheus_continue(request).await
    }

    pub async fn orpheus_bridge_with_progress(
        &self,
        request: OrpheusBridgeRequest,
        _progress: Option<ProgressSender>,
    ) -> Result<CallToolResult, McpError> {
        // TODO: Add progress notifications
        self.orpheus_bridge(request).await
    }

    #[tracing::instrument(
        name = "mcp.tool.orpheus_classify",
        skip(self, request),
        fields(
            midi.hash = %request.midi_hash,
        )
    )]
    pub async fn orpheus_classify(
        &self,
        request: OrpheusClassifyRequest,
    ) -> Result<CallToolResult, McpError> {
        let result = self.local_models.run_orpheus_classifier(request.midi_hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Orpheus classifier failed: {}", e)))?;

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)])
            .with_structured(result))
    }

    #[tracing::instrument(
        name = "mcp.tool.orpheus_loops",
        skip(self, request),
        fields(
            job.id = tracing::field::Empty,
            variations = request.num_variations.unwrap_or(1),
        )
    )]
    pub async fn orpheus_loops(
        &self,
        request: OrpheusLoopsRequest,
    ) -> Result<CallToolResult, McpError> {
        let job_id = self.job_store.create_job("orpheus_loops".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let result: anyhow::Result<JobSpawnResponse> = (async {
                let orpheus_result = local_models.run_orpheus_loops(
                    request.seed_hash.clone(),
                    request.temperature,
                    request.top_p,
                    request.max_tokens,
                    request.num_variations,
                    Some(job_id_clone.as_str().to_string()),
                ).await?;

                // Create artifacts
                let mut artifacts_result = Vec::new();
                let store = artifact_store.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                for (i, hash) in orpheus_result.output_hashes.iter().enumerate() {
                    let tokens = orpheus_result.num_tokens.get(i).copied();
                    let content_hash = ContentHash::new(hash);
                    let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                    let creator = request.creator.clone().unwrap_or_else(|| "agent_orpheus".to_string());

                    let mut tags = request.tags.clone();
                    tags.extend_from_slice(&["type:midi".to_string(), "source:orpheus".to_string(), "tool:orpheus_loops".to_string()]);

                    let mut artifact = Artifact::new(
                        artifact_id.clone(),
                        content_hash,
                        &creator,
                        serde_json::json!({
                            "type": "orpheus_loops",
                            "params": {
                                "temperature": request.temperature,
                                "top_p": request.top_p,
                                "max_tokens": request.max_tokens,
                            },
                            "generation": {
                                "tokens": tokens,
                                "job_id": job_id_clone.as_str(),
                            },
                        })
                    ).with_tags(tags);

                    if let Some(ref set_id) = request.variation_set_id {
                        let index = store.next_variation_index(set_id)?;
                        artifact = artifact.with_variation_set(VariationSetId::new(set_id.clone()), index);
                    }

                    if let Some(ref parent_id) = request.parent_id {
                        artifact = artifact.with_parent(ArtifactId::new(parent_id.clone()));
                    }

                    store.put(artifact)?;
                    artifacts_result.push(artifact_id.as_str().to_string());
                }

                store.flush()?;
                drop(store);

                Ok(JobSpawnResponse {
                    job_id: job_id_clone.as_str().to_string(),
                    status: JobStatus::Completed,
                    artifact_id: artifacts_result.first().map(|s| s.to_string()),
                    content_hash: orpheus_result.output_hashes.first().map(|s| s.to_string()),
                    message: Some(orpheus_result.summary),
                })
            }).await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, serde_json::to_value(response).unwrap());
                }
                Err(e) => {
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
            message: Some("Generation started. Use job_poll() to retrieve results.".to_string()),
        };

        Ok(CallToolResult::success(vec![Content::text(format!("Started job: {}", job_id.as_str()))])
            .with_structured(serde_json::to_value(response).unwrap()))
    }
}
