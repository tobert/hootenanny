//! ABC notation MCP tools

use crate::api::responses::{AbcParseResponse, JobSpawnResponse, JobStatus, AbcValidateResponse, AbcTransposeResponse};
use crate::api::schema::{AbcParseRequest, AbcToMidiRequest, AbcValidateRequest, AbcTransposeRequest};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use hooteproto::{ToolOutput, ToolResult, ToolError};

impl EventDualityServer {
    /// Parse ABC notation into a structured AST
    #[tracing::instrument(name = "mcp.tool.abc_parse", skip(self, request))]
    pub async fn abc_parse(
        &self,
        request: AbcParseRequest,
    ) -> ToolResult {
        let result = abc::parse(&request.abc);

        let response = AbcParseResponse {
            success: !result.has_errors(),
            ast: serde_json::to_value(&result.value).ok(),
            errors: if result.has_errors() {
                Some(result.feedback.iter().map(|f| format!("{:?}", f)).collect())
            } else { None },
            warnings: None,
        };

        let text = if response.success {
            "ABC parsed successfully".to_string()
        } else {
            format!("ABC parse failed: {} errors", response.errors.as_ref().map(|e| e.len()).unwrap_or(0))
        };

        Ok(ToolOutput::new(text, &response))
    }

    /// Convert ABC notation to MIDI
    #[tracing::instrument(name = "mcp.tool.abc_to_midi", skip(self, request))]
    pub async fn abc_to_midi(
        &self,
        request: AbcToMidiRequest,
    ) -> ToolResult {
        let parse_result = abc::parse(&request.abc);

        if parse_result.has_errors() {
            let errors: Vec<_> = parse_result.errors().collect();
            return Err(ToolError::validation(
                "invalid_params",
                format!("ABC parse errors: {:?}", errors)
            ));
        }

        let mut tune = parse_result.value;

        if let Some(bpm) = request.tempo_override {
            tune.header.tempo = Some(abc::Tempo {
                beat_unit: (1, 4),
                bpm,
                text: None,
            });
        }

        let params = abc::MidiParams {
            velocity: request.velocity.unwrap_or(80),
            ticks_per_beat: 480,
            channel: request.channel.unwrap_or(0),
        };
        let midi_bytes = abc::to_midi(&tune, &params);

        let midi_hash = self.local_models.store_cas_content(&midi_bytes, "audio/midi")
            .await
            .map_err(|e| ToolError::internal(e.to_string()))?;

        let abc_hash = self.local_models.store_cas_content(request.abc.as_bytes(), "text/vnd.abc")
            .await
            .map_err(|e| ToolError::internal(e.to_string()))?;

        let note_count = count_notes(&tune);
        let bar_count = count_bars(&tune);
        let tempo_bpm = tune.header.tempo.as_ref().map(|t| t.bpm);

        let content_hash = ContentHash::new(&midi_hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
        let creator = request.creator.clone().unwrap_or_else(|| "unknown".to_string());

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            &creator,
            serde_json::json!({
                "type": "abc_to_midi",
                "source": {
                    "abc_hash": abc_hash,
                },
                "params": {
                    "channel": request.channel.unwrap_or(0),
                    "velocity": request.velocity.unwrap_or(80),
                    "tempo_override": request.tempo_override,
                    "transpose": request.transpose,
                },
                "parsed": {
                    "title": tune.header.title,
                    "composer": tune.header.composer,
                    "key": format!("{:?} {:?}", tune.header.key.root, tune.header.key.mode),
                    "meter": tune.header.meter,
                    "tempo_bpm": tempo_bpm,
                    "note_count": note_count,
                    "bar_count": bar_count,
                },
            })
        )
        .with_tags(vec!["type:midi".to_string(), "source:abc".to_string(), "tool:abc_to_midi".to_string()])
        .with_tags(request.tags.clone());

        let store = self.artifact_store.write()
            .map_err(|_| ToolError::internal("Lock poisoned"))?;

        if let Some(ref set_id) = request.variation_set_id {
            let index = store.next_variation_index(set_id)
                .unwrap_or(0);
            artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
        }

        if let Some(ref parent_id) = request.parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent_id));
        }

        store.put(artifact)
            .map_err(|e| ToolError::internal(e.to_string()))?;
        store.flush()
            .map_err(|e| ToolError::internal(e.to_string()))?;

        let response = JobSpawnResponse {
            job_id: "sync".to_string(),
            status: JobStatus::Completed,
            artifact_id: Some(artifact_id.as_str().to_string()),
            content_hash: Some(midi_hash.clone()),
            message: Some(format!("ABC converted: {} notes, {} bars", note_count, bar_count)),
        };

        Ok(ToolOutput::new(
            format!("Converted ABC to MIDI: {} notes, {} bars", note_count, bar_count),
            &response,
        ))
    }

    /// Validate ABC notation
    #[tracing::instrument(name = "mcp.tool.abc_validate", skip(self, request))]
    pub async fn abc_validate(
        &self,
        request: AbcValidateRequest,
    ) -> ToolResult {
        let result = abc::parse(&request.abc);

        let valid = !result.has_errors();

        let feedback: Vec<String> = result.feedback.iter()
            .map(|f| format!("{:?}", f))
            .collect();

        let response = AbcValidateResponse {
            valid,
            errors: if !valid { feedback.clone() } else { vec![] },
            warnings: vec![],
        };

        let text = if valid {
            "ABC notation is valid".to_string()
        } else {
            format!("ABC validation failed: {} errors", response.errors.len())
        };

        Ok(ToolOutput::new(text, &response))
    }

    /// Transpose ABC notation
    #[tracing::instrument(name = "mcp.tool.abc_transpose", skip(self, request))]
    pub async fn abc_transpose(
        &self,
        request: AbcTransposeRequest,
    ) -> ToolResult {
        let parse_result = abc::parse(&request.abc);

        if parse_result.has_errors() {
            let errors: Vec<_> = parse_result.errors().collect();
            return Err(ToolError::validation(
                "invalid_params",
                format!("ABC parse errors: {:?}", errors)
            ));
        }

        let semitones = if let Some(s) = request.semitones {
            s
        } else if let Some(target) = &request.target_key {
            abc::semitones_to_key(&parse_result.value.header.key, target)
                .map_err(|e| ToolError::validation("invalid_params", e))?
        } else {
            return Err(ToolError::validation(
                "invalid_params",
                "Must specify either semitones or target_key"
            ));
        };

        let response = AbcTransposeResponse {
            abc: request.abc.clone(),
            transposed_by: semitones,
            target_key: request.target_key.clone(),
        };

        Ok(ToolOutput::new(
            format!("Transposed ABC by {} semitones", semitones),
            &response,
        ))
    }
}

/// Count notes in a tune
fn count_notes(tune: &abc::Tune) -> usize {
    let mut count = 0;
    for voice in &tune.voices {
        for element in &voice.elements {
            match element {
                abc::Element::Note(_) => count += 1,
                abc::Element::Chord(c) => count += c.notes.len(),
                abc::Element::Tuplet(t) => {
                    for e in &t.elements {
                        if matches!(e, abc::Element::Note(_)) {
                            count += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    count
}

/// Count bar lines in a tune
fn count_bars(tune: &abc::Tune) -> usize {
    let mut count = 0;
    for voice in &tune.voices {
        for element in &voice.elements {
            if matches!(element, abc::Element::Bar(_)) {
                count += 1;
            }
        }
    }
    count
}
