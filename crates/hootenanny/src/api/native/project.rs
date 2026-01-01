//! Content projection - transform content between formats and encodings.
//!
//! The `project()` tool converts content from one encoding to another, such as:
//! - MIDI to audio via SoundFont rendering
//! - ABC notation to MIDI
//! - Audio format conversions (future)

use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::mcp_tools::rustysynth::{inspect_soundfont, render_midi_to_wav};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use hooteproto::responses::{AudioFormat, AudioGeneratedResponse, ToolResponse};
use hooteproto::{Encoding, OutputType, ProjectionTarget, ToolError};
use std::sync::Arc;

// Re-export from hooteproto for backwards compatibility
pub use hooteproto::request::ProjectRequest;

/// Artifact metadata for projection operations
#[derive(Debug, Clone)]
struct ArtifactMetadata {
    tags: Vec<String>,
    creator: Option<String>,
    variation_set_id: Option<String>,
    parent_id: Option<String>,
}

/// Helper to find an artifact by content hash
fn find_artifact_by_hash<S: ArtifactStore>(store: &S, content_hash: &str) -> Option<String> {
    store
        .all()
        .ok()?
        .into_iter()
        .find(|a| a.content_hash.as_str() == content_hash)
        .map(|a| a.id.as_str().to_string())
}

impl EventDualityServer {
    /// Project content from one encoding to another.
    ///
    /// This is a unified tool for content transformations:
    /// - MIDI to audio via SoundFont
    /// - ABC to MIDI
    /// - Format conversions
    #[tracing::instrument(
        name = "mcp.tool.project",
        skip(self, request),
        fields(
            encoding.type = ?request.encoding,
            target.type = ?request.target,
            artifact.id = tracing::field::Empty,
        )
    )]
    pub async fn project(&self, request: ProjectRequest) -> Result<ToolResponse, ToolError> {
        // Resolve source encoding to content hash and type
        let (source_hash, source_type) = self.resolve_encoding(&request.encoding).await?;

        // Determine projection type and validate compatibility
        let is_audio_projection = matches!(request.target, ProjectionTarget::Audio { .. });

        if is_audio_projection {
            // Audio projection: MIDI -> Audio
            if source_type != OutputType::Midi {
                return Err(ToolError::validation(
                    "invalid_params",
                    format!(
                        "Audio projection requires MIDI source, got {:?}",
                        source_type
                    ),
                ));
            }

            if let ProjectionTarget::Audio {
                soundfont_hash,
                sample_rate,
            } = request.target
            {
                let metadata = ArtifactMetadata {
                    tags: request.tags,
                    creator: request.creator,
                    variation_set_id: request.variation_set_id,
                    parent_id: request.parent_id,
                };
                return self
                    .project_to_audio(source_hash, soundfont_hash, sample_rate, metadata)
                    .await;
            }
        } else {
            // MIDI projection: ABC -> MIDI
            if source_type != OutputType::Symbolic {
                return Err(ToolError::validation(
                    "invalid_params",
                    "MIDI projection currently only supports ABC notation sources",
                ));
            }

            if let (Encoding::Abc { notation }, ProjectionTarget::Midi { channel, velocity }) =
                (request.encoding, request.target)
            {
                let metadata = ArtifactMetadata {
                    tags: request.tags,
                    creator: request.creator,
                    variation_set_id: request.variation_set_id,
                    parent_id: request.parent_id,
                };
                return self
                    .project_to_midi(notation, channel, velocity, metadata)
                    .await;
            } else {
                return Err(ToolError::validation(
                    "invalid_params",
                    "MIDI projection requires ABC notation encoding",
                ));
            }
        }

        Err(ToolError::internal("Invalid projection configuration"))
    }

    /// Resolve an encoding to a content hash and output type
    async fn resolve_encoding(
        &self,
        encoding: &Encoding,
    ) -> Result<(String, OutputType), ToolError> {
        match encoding {
            Encoding::Midi { artifact_id } => {
                let store = self
                    .artifact_store
                    .read()
                    .map_err(|_| ToolError::internal("Lock poisoned"))?;
                let artifact = store
                    .get(artifact_id)
                    .map_err(|e| ToolError::internal(format!("Failed to get artifact: {}", e)))?
                    .ok_or_else(|| ToolError::not_found("artifact", artifact_id))?;
                Ok((artifact.content_hash.as_str().to_string(), OutputType::Midi))
            }
            Encoding::Audio { artifact_id } => {
                let store = self
                    .artifact_store
                    .read()
                    .map_err(|_| ToolError::internal("Lock poisoned"))?;
                let artifact = store
                    .get(artifact_id)
                    .map_err(|e| ToolError::internal(format!("Failed to get artifact: {}", e)))?
                    .ok_or_else(|| ToolError::not_found("artifact", artifact_id))?;
                Ok((
                    artifact.content_hash.as_str().to_string(),
                    OutputType::Audio,
                ))
            }
            Encoding::Abc { notation: _ } => {
                // ABC is handled specially - we don't need a hash for it
                Ok((String::new(), OutputType::Symbolic))
            }
            Encoding::Hash {
                content_hash,
                format,
            } => {
                let output_type = if format.contains("midi") {
                    OutputType::Midi
                } else if format.contains("audio") || format.contains("wav") {
                    OutputType::Audio
                } else {
                    OutputType::Symbolic
                };
                Ok((content_hash.clone(), output_type))
            }
        }
    }

    /// Project MIDI to audio via SoundFont rendering
    async fn project_to_audio(
        &self,
        midi_hash: String,
        soundfont_hash: String,
        sample_rate: Option<u32>,
        metadata: ArtifactMetadata,
    ) -> Result<ToolResponse, ToolError> {
        let job_id = self.job_store.create_job("project_to_audio".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            let result: anyhow::Result<ToolResponse> = (async {
                let sample_rate = sample_rate.unwrap_or(44100);

                let midi_ref = local_models.inspect_cas_content(&midi_hash).await?;
                let midi_path = midi_ref
                    .local_path
                    .ok_or_else(|| anyhow::anyhow!("MIDI not found in local CAS"))?;
                let midi_bytes = tokio::fs::read(&midi_path).await?;

                let sf_ref = local_models.inspect_cas_content(&soundfont_hash).await?;
                let sf_path = sf_ref
                    .local_path
                    .ok_or_else(|| anyhow::anyhow!("SoundFont not found in local CAS"))?;
                let sf_bytes = tokio::fs::read(&sf_path).await?;

                let soundfont_name = inspect_soundfont(&sf_bytes, false)
                    .map(|info| info.info.name)
                    .ok();

                let wav_bytes = render_midi_to_wav(&midi_bytes, &sf_bytes, sample_rate)?;

                let wav_size = wav_bytes.len();
                let duration_secs =
                    crate::mcp_tools::rustysynth::calculate_wav_duration(&wav_bytes, sample_rate);

                let wav_hash = local_models
                    .store_cas_content(&wav_bytes, "audio/wav")
                    .await?;

                let store = artifact_store
                    .read()
                    .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                let midi_artifact_id = find_artifact_by_hash(&*store, &midi_hash);
                let soundfont_artifact_id = find_artifact_by_hash(&*store, &soundfont_hash);
                drop(store);

                let mut artifact_tags = metadata.tags.clone();
                artifact_tags.push("type:audio".to_string());
                artifact_tags.push("format:wav".to_string());
                artifact_tags.push("tool:project".to_string());
                artifact_tags.push("projection:audio".to_string());

                let content_hash = ContentHash::new(&wav_hash);
                let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                let artifact_metadata = serde_json::json!({
                    "type": "projection",
                    "projection_type": "midi_to_audio",
                    "source": {
                        "midi_hash": midi_hash,
                        "midi_artifact_id": midi_artifact_id
                    },
                    "soundfont": {
                        "hash": soundfont_hash,
                        "name": soundfont_name,
                        "artifact_id": soundfont_artifact_id
                    },
                    "params": {
                        "sample_rate": sample_rate
                    },
                    "output": {
                        "duration_seconds": duration_secs,
                        "channels": 2,
                        "bit_depth": 16,
                        "size_bytes": wav_size
                    },
                });

                let mut artifact = Artifact::new(
                    artifact_id.clone(),
                    content_hash,
                    metadata.creator.unwrap_or_else(|| "unknown".to_string()),
                    artifact_metadata,
                )
                .with_tags(artifact_tags);

                let store = artifact_store
                    .write()
                    .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
                if let Some(set_id) = metadata.variation_set_id {
                    let next_idx = store.next_variation_index(&set_id).unwrap_or(0);
                    artifact = artifact.with_variation_set(VariationSetId::new(set_id), next_idx);
                }
                if let Some(parent_id) = metadata.parent_id {
                    artifact = artifact.with_parent(ArtifactId::new(parent_id));
                }
                store.put(artifact)?;
                store.flush()?;

                Ok(ToolResponse::AudioGenerated(AudioGeneratedResponse {
                    artifact_id: artifact_id.as_str().to_string(),
                    content_hash: wav_hash,
                    duration_seconds: duration_secs,
                    sample_rate,
                    format: AudioFormat::Wav,
                    genre: None,
                }))
            })
            .await;

            match result {
                Ok(response) => {
                    let _ = job_store.mark_complete(&job_id_clone, response);
                }
                Err(e) => {
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        self.job_store.store_handle(&job_id, handle);

        Ok(ToolResponse::job_started(job_id.as_str().to_string(), "project"))
    }

    /// Project ABC notation to MIDI
    async fn project_to_midi(
        &self,
        abc_notation: String,
        channel: Option<u8>,
        velocity: Option<u8>,
        metadata: ArtifactMetadata,
    ) -> Result<ToolResponse, ToolError> {
        let parse_result = abc::parse(&abc_notation);

        if parse_result.has_errors() {
            let errors: Vec<_> = parse_result.errors().collect();
            return Err(ToolError::validation(
                "invalid_params",
                format!("ABC parse errors: {:?}", errors),
            ));
        }

        let tune = parse_result.value;

        let params = abc::MidiParams {
            velocity: velocity.unwrap_or(80),
            ticks_per_beat: 480,
            channel: channel.unwrap_or(0),
        };
        let midi_bytes = abc::to_midi(&tune, &params);

        // Calculate MIDI duration for scheduling
        // Get tempo from ABC header (default 120 BPM)
        let bpm = tune.header.tempo.as_ref().map(|t| t.bpm).unwrap_or(120) as f64;
        let duration_seconds =
            crate::mcp_tools::rustysynth::calculate_midi_duration(&midi_bytes);
        // Convert to beats: beats = seconds * (bpm / 60)
        let duration_beats = duration_seconds.map(|secs| secs * bpm / 60.0);

        let midi_hash = self
            .local_models
            .store_cas_content(&midi_bytes, "audio/midi")
            .await
            .map_err(|e| ToolError::internal(e.to_string()))?;

        let abc_hash = self
            .local_models
            .store_cas_content(abc_notation.as_bytes(), "text/vnd.abc")
            .await
            .map_err(|e| ToolError::internal(e.to_string()))?;

        let content_hash = ContentHash::new(&midi_hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
        let creator_str = metadata.creator.unwrap_or_else(|| "unknown".to_string());

        let mut artifact_tags = metadata.tags.clone();
        artifact_tags.push("type:midi".to_string());
        artifact_tags.push("source:abc".to_string());
        artifact_tags.push("tool:project".to_string());
        artifact_tags.push("projection:midi".to_string());

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            &creator_str,
            serde_json::json!({
                "type": "projection",
                "projection_type": "abc_to_midi",
                "duration_seconds": duration_beats,  // Actually beats, for schedule() compatibility
                "duration_seconds_real": duration_seconds,  // True seconds
                "tempo_bpm": bpm,
                "source": {
                    "abc_hash": abc_hash,
                },
                "params": {
                    "channel": channel.unwrap_or(0),
                    "velocity": velocity.unwrap_or(80),
                },
                "parsed": {
                    "title": tune.header.title,
                    "composer": tune.header.composer,
                    "key": format!("{:?} {:?}", tune.header.key.root, tune.header.key.mode),
                    "meter": tune.header.meter,
                },
            }),
        )
        .with_tags(artifact_tags);

        let store = self
            .artifact_store
            .write()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;

        if let Some(ref set_id) = metadata.variation_set_id {
            let index = store.next_variation_index(set_id).unwrap_or(0);
            artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
        }

        if let Some(ref parent_id) = metadata.parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent_id));
        }

        store
            .put(artifact)
            .map_err(|e| ToolError::internal(e.to_string()))?;
        store
            .flush()
            .map_err(|e| ToolError::internal(e.to_string()))?;

        tracing::Span::current().record("artifact.id", artifact_id.as_str());

        Ok(ToolResponse::ProjectResult(hooteproto::responses::ProjectResultResponse {
            artifact_id: artifact_id.as_str().to_string(),
            content_hash: midi_hash,
            projection_type: "abc_to_midi".to_string(),
            duration_seconds: None,
            sample_rate: None,
        }))
    }
}
