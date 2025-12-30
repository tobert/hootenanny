//! Sample tool - unified generation across model spaces.
//!
//! This implements the model-native `sample()` API that abstracts different generative
//! models (Orpheus, MusicGen, YuE) behind a unified interface based on the `Space` parameter.

use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::local_models::OrpheusGenerateParams;
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use hooteproto::responses::{AudioFormat, AudioGeneratedResponse, JobSpawnResponse, JobState, OrpheusGeneratedResponse, ToolResponse};
use hooteproto::{Encoding, Space, ToolError, ToolOutput, ToolResult};
use std::sync::Arc;
use tracing;

// Re-export from hooteproto for backwards compatibility
pub use hooteproto::request::SampleRequest;

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
        name = "mcp.tool.sample",
        skip(self, request),
        fields(
            space = ?request.space,
            num_variations = request.num_variations,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn sample(&self, request: SampleRequest) -> ToolResult {
        // Validate inference context
        request
            .inference
            .validate()
            .map_err(|e| ToolError::validation("invalid_inference", e.to_string()))?;

        // Validate space-specific requirements
        match request.space {
            Space::Yue => {
                if request.prompt.is_none() {
                    return Err(ToolError::validation(
                        "missing_prompt",
                        "YuE space requires a lyrics prompt".to_string(),
                    ));
                }
            }
            Space::Abc => {
                return Err(ToolError::validation(
                    "unsupported_space",
                    "ABC space does not support sampling (use project() instead)".to_string(),
                ));
            }
            _ => {}
        }

        let job_id = self.job_store.create_job("sample".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();
        let broadcaster = self.broadcaster.clone();
        let space = request.space;

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let result: anyhow::Result<ToolResponse> = (async {
                match request.space {
                    // Orpheus family - MIDI generation
                    Space::Orpheus | Space::OrpheusChildren | Space::OrpheusMonoMelodies => {
                        sample_orpheus(
                            &local_models,
                            &artifact_store,
                            &request,
                            job_id_clone.as_str(),
                            &broadcaster,
                        )
                        .await
                    }

                    // Orpheus loops - special case
                    Space::OrpheusLoops => {
                        sample_orpheus_loops(
                            &local_models,
                            &artifact_store,
                            &request,
                            job_id_clone.as_str(),
                            &broadcaster,
                        )
                        .await
                    }

                    // Orpheus bridge - special case
                    Space::OrpheusBridge => {
                        Err(anyhow::anyhow!(
                            "OrpheusBridge space requires two sections (use orpheus_bridge tool directly)"
                        ))
                    }

                    // MusicGen - audio generation
                    Space::MusicGen => {
                        sample_musicgen(
                            &local_models,
                            &artifact_store,
                            &request,
                            job_id_clone.as_str(),
                        )
                        .await
                    }

                    // YuE - lyrics-to-song
                    Space::Yue => {
                        sample_yue(
                            &local_models,
                            &artifact_store,
                            &request,
                            job_id_clone.as_str(),
                        )
                        .await
                    }

                    // ABC - not supported
                    Space::Abc => {
                        Err(anyhow::anyhow!("ABC space does not support sampling"))
                    }
                }
            })
            .await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, response);
                }
                Err(e) => {
                    tracing::error!(error = %e, space = ?space, "Sample generation failed");
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        self.job_store.store_handle(&job_id, handle);

        let response = JobSpawnResponse {
            job_id: job_id.as_str().to_string(),
            status: JobState::Pending,
            artifact_id: None,
            content_hash: None,
            message: Some(format!(
                "Sample generation started in {:?} space. Use job_poll() to retrieve results.",
                space
            )),
        };

        Ok(ToolOutput::new(
            format!("Started sample job: {}", job_id.as_str()),
            &response,
        ))
    }
}

/// Sample from Orpheus (base, children, mono_melodies)
async fn sample_orpheus(
    local_models: &Arc<crate::mcp_tools::local_models::LocalModels>,
    artifact_store: &Arc<std::sync::RwLock<crate::artifact_store::FileStore>>,
    request: &SampleRequest,
    job_id: &str,
    broadcaster: &Option<crate::zmq::BroadcastPublisher>,
) -> anyhow::Result<ToolResponse> {
    let (variant, temperature, top_p, max_tokens) = request.inference.to_orpheus_params();
    let model = variant.or_else(|| request.space.model_variant().map(String::from));

    let params = OrpheusGenerateParams {
        temperature,
        top_p,
        max_tokens,
        num_variations: request.num_variations,
    };

    // Resolve seed if provided
    let seed_hash = if let Some(ref seed) = request.seed {
        match seed {
            Encoding::Hash { content_hash, .. } => Some(content_hash.clone()),
            Encoding::Midi { artifact_id } => {
                let store = artifact_store
                    .read()
                    .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                artifact_to_hash(&*store, artifact_id)
            }
            Encoding::Audio { .. } => {
                return Err(anyhow::anyhow!("Audio encoding not supported for Orpheus"));
            }
            Encoding::Abc { .. } => {
                return Err(anyhow::anyhow!("ABC encoding not supported for Orpheus"));
            }
        }
    } else {
        None
    };

    let task = if seed_hash.is_some() {
        "generate_seeded".to_string()
    } else {
        "generate".to_string()
    };

    let model_name = model.clone().unwrap_or_else(|| "base".to_string());

    let result = local_models
        .run_orpheus_generate(
            model_name.clone(),
            task.clone(),
            seed_hash.clone(),
            params,
            Some(job_id.to_string()),
        )
        .await?;

    let mut artifacts = Vec::new();
    let store = artifact_store
        .write()
        .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

    for (i, hash) in result.output_hashes.iter().enumerate() {
        let tokens = result.num_tokens.get(i).copied().map(|t| t as u32);
        let content_hash = ContentHash::new(hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
        let creator = request
            .creator
            .clone()
            .unwrap_or_else(|| "agent_orpheus".to_string());

        let mut metadata = serde_json::json!({
            "type": "orpheus_generation",
            "task": task,
            "space": request.space,
            "model": { "name": model_name },
            "params": {
                "temperature": temperature,
                "top_p": top_p,
                "max_tokens": max_tokens,
                "num_variations": request.num_variations,
            },
            "generation": {
                "tokens": tokens,
                "job_id": job_id,
            },
        });

        if let Some(ref seed_hash_value) = seed_hash {
            metadata["seed"] = serde_json::json!({
                "hash": seed_hash_value,
            });
        }

        let mut tags = vec![
            "type:midi".to_string(),
            "source:orpheus".to_string(),
            "tool:sample".to_string(),
        ];
        tags.extend_from_slice(&request.tags);

        let mut artifact =
            Artifact::new(artifact_id, content_hash, &creator, metadata).with_tags(tags.clone());

        if let Some(ref set_id) = request.variation_set_id {
            let index = store.next_variation_index(set_id)?;
            artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
        }

        if let Some(ref parent_id) = request.parent_id {
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

    let tokens_per_variation: Vec<u64> = result.num_tokens.iter().map(|&t| t as u64).collect();
    let total_tokens: u64 = tokens_per_variation.iter().sum();

    Ok(ToolResponse::OrpheusGenerated(OrpheusGeneratedResponse {
        output_hashes: result.output_hashes,
        artifact_ids: artifacts.iter().map(|a| a.id.as_str().to_string()).collect(),
        tokens_per_variation,
        total_tokens,
        variation_set_id: artifacts.first().and_then(|a| a.variation_set_id.as_ref().map(|s| s.as_str().to_string())),
        summary: result.summary,
    }))
}

/// Sample from Orpheus loops
async fn sample_orpheus_loops(
    local_models: &Arc<crate::mcp_tools::local_models::LocalModels>,
    artifact_store: &Arc<std::sync::RwLock<crate::artifact_store::FileStore>>,
    request: &SampleRequest,
    job_id: &str,
    broadcaster: &Option<crate::zmq::BroadcastPublisher>,
) -> anyhow::Result<ToolResponse> {
    let (_, temperature, top_p, max_tokens) = request.inference.to_orpheus_params();

    // Resolve seed if provided
    let seed_hash = if let Some(ref seed) = request.seed {
        match seed {
            Encoding::Hash { content_hash, .. } => Some(content_hash.clone()),
            Encoding::Midi { artifact_id } => {
                let store = artifact_store
                    .read()
                    .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                artifact_to_hash(&*store, artifact_id)
            }
            _ => return Err(anyhow::anyhow!("Invalid encoding for Orpheus loops")),
        }
    } else {
        None
    };

    let result = local_models
        .run_orpheus_loops(
            seed_hash.clone(),
            temperature,
            top_p,
            max_tokens,
            request.num_variations,
            Some(job_id.to_string()),
        )
        .await?;

    let mut artifacts = Vec::new();
    let store = artifact_store
        .write()
        .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

    for (i, hash) in result.output_hashes.iter().enumerate() {
        let tokens = result.num_tokens.get(i).copied();
        let content_hash = ContentHash::new(hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
        let creator = request
            .creator
            .clone()
            .unwrap_or_else(|| "agent_orpheus".to_string());

        let mut tags = vec![
            "type:midi".to_string(),
            "source:orpheus".to_string(),
            "tool:sample".to_string(),
            "loopable".to_string(),
        ];
        tags.extend_from_slice(&request.tags);

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            &creator,
            serde_json::json!({
                "type": "orpheus_loops",
                "space": Space::OrpheusLoops,
                "params": {
                    "temperature": temperature,
                    "top_p": top_p,
                    "max_tokens": max_tokens,
                },
                "generation": {
                    "tokens": tokens,
                    "job_id": job_id,
                },
            }),
        )
        .with_tags(tags);

        if let Some(ref set_id) = request.variation_set_id {
            let index = store.next_variation_index(set_id)?;
            artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
        }

        if let Some(ref parent_id) = request.parent_id {
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

    let tokens_per_variation: Vec<u64> = result.num_tokens.iter().map(|&t| t as u64).collect();
    let total_tokens: u64 = tokens_per_variation.iter().sum();

    Ok(ToolResponse::OrpheusGenerated(OrpheusGeneratedResponse {
        output_hashes: result.output_hashes,
        artifact_ids: artifacts.iter().map(|a| a.id.as_str().to_string()).collect(),
        tokens_per_variation,
        total_tokens,
        variation_set_id: artifacts.first().and_then(|a| a.variation_set_id.as_ref().map(|s| s.as_str().to_string())),
        summary: result.summary,
    }))
}

/// Sample from MusicGen
async fn sample_musicgen(
    local_models: &Arc<crate::mcp_tools::local_models::LocalModels>,
    artifact_store: &Arc<std::sync::RwLock<crate::artifact_store::FileStore>>,
    request: &SampleRequest,
    job_id: &str,
) -> anyhow::Result<ToolResponse> {
    let (temperature, top_p, top_k, guidance_scale, duration) =
        request.inference.to_musicgen_params();

    let response = local_models
        .run_musicgen_generate(
            request.prompt.clone().unwrap_or_default(),
            duration.unwrap_or(10.0),
            temperature.unwrap_or(1.0),
            top_k.unwrap_or(250),
            top_p.unwrap_or(0.9),
            guidance_scale.unwrap_or(3.0),
            true, // do_sample
            Some(job_id.to_string()),
        )
        .await?;

    let audio_b64 = response
        .get("audio_base64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No audio_base64 in MusicGen response"))?;

    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    let audio_bytes = BASE64.decode(audio_b64)?;

    let audio_hash = local_models
        .store_cas_content(&audio_bytes, "audio/wav")
        .await?;

    let content_hash = ContentHash::new(&audio_hash);
    let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

    let duration_secs = response
        .get("duration")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let sample_rate = response
        .get("sample_rate")
        .and_then(|v| v.as_u64())
        .unwrap_or(32000);

    let mut tags = vec![
        "type:audio".to_string(),
        "format:wav".to_string(),
        "source:musicgen".to_string(),
        "tool:sample".to_string(),
    ];
    tags.extend_from_slice(&request.tags);

    let mut artifact = Artifact::new(
        artifact_id.clone(),
        content_hash,
        request.creator.as_deref().unwrap_or("unknown"),
        serde_json::json!({
            "type": "musicgen_generation",
            "space": Space::MusicGen,
            "prompt": request.prompt,
            "params": {
                "duration": duration,
                "temperature": temperature,
                "top_k": top_k,
                "top_p": top_p,
                "guidance_scale": guidance_scale,
            },
            "output": {
                "duration_seconds": duration_secs,
                "sample_rate": sample_rate,
                "format": "wav",
            }
        }),
    )
    .with_tags(tags);

    if let Some(ref set_id) = request.variation_set_id {
        let store = artifact_store
            .write()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        let index = store.next_variation_index(set_id)?;
        artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
    }

    if let Some(ref parent_id) = request.parent_id {
        artifact = artifact.with_parent(ArtifactId::new(parent_id));
    }

    {
        let store = artifact_store
            .write()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        store.put(artifact)?;
        store.flush()?;
    }

    Ok(ToolResponse::AudioGenerated(AudioGeneratedResponse {
        artifact_id: artifact_id.as_str().to_string(),
        content_hash: audio_hash,
        duration_seconds: duration_secs,
        sample_rate: sample_rate as u32,
        format: AudioFormat::Wav,
        genre: None,
    }))
}

/// Sample from YuE
async fn sample_yue(
    local_models: &Arc<crate::mcp_tools::local_models::LocalModels>,
    artifact_store: &Arc<std::sync::RwLock<crate::artifact_store::FileStore>>,
    request: &SampleRequest,
    job_id: &str,
) -> anyhow::Result<ToolResponse> {
    let lyrics = request
        .prompt
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("YuE requires lyrics in prompt field"))?;

    let response = local_models
        .run_yue_generate(
            lyrics.clone(),
            request
                .inference
                .variant
                .clone()
                .unwrap_or_else(|| "Pop".to_string()),
            request.inference.max_tokens.unwrap_or(3000),
            2, // run_n_segments
            request.inference.seed.unwrap_or(42),
            Some(job_id.to_string()),
        )
        .await?;

    if let Some(error) = response.get("error") {
        let error_msg = error.as_str().unwrap_or("Unknown error");
        return Err(anyhow::anyhow!("YuE generation failed: {}", error_msg));
    }

    let audio_b64 = response
        .get("audio_base64")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No audio_base64 in YuE response"))?;

    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    let audio_bytes = BASE64.decode(audio_b64)?;

    let format = response
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("wav");
    let mime_type = match format {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "audio/wav",
    };

    let audio_hash = local_models
        .store_cas_content(&audio_bytes, mime_type)
        .await?;

    let content_hash = ContentHash::new(&audio_hash);
    let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

    let genre_value = response
        .get("genre")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut tags = vec![
        "type:audio".to_string(),
        format!("format:{}", format),
        "source:yue".to_string(),
        "tool:sample".to_string(),
        "has:vocals".to_string(),
    ];
    tags.extend_from_slice(&request.tags);

    let mut artifact = Artifact::new(
        artifact_id.clone(),
        content_hash,
        request.creator.as_deref().unwrap_or("unknown"),
        serde_json::json!({
            "type": "yue_generation",
            "space": Space::Yue,
            "lyrics": lyrics,
            "params": {
                "genre": genre_value,
                "max_tokens": request.inference.max_tokens,
                "seed": request.inference.seed,
            },
            "output": {
                "format": format,
            }
        }),
    )
    .with_tags(tags);

    if let Some(ref set_id) = request.variation_set_id {
        let store = artifact_store
            .write()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        let index = store.next_variation_index(set_id)?;
        artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
    }

    if let Some(ref parent_id) = request.parent_id {
        artifact = artifact.with_parent(ArtifactId::new(parent_id));
    }

    {
        let store = artifact_store
            .write()
            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        store.put(artifact)?;
        store.flush()?;
    }

    let audio_format = match format {
        "mp3" => AudioFormat::Mp3,
        "flac" => AudioFormat::Flac,
        _ => AudioFormat::Wav,
    };

    Ok(ToolResponse::AudioGenerated(AudioGeneratedResponse {
        artifact_id: artifact_id.as_str().to_string(),
        content_hash: audio_hash,
        duration_seconds: 0.0, // YuE doesn't report duration
        sample_rate: 0,        // YuE doesn't report sample rate
        format: audio_format,
        genre: genre_value,
    }))
}
