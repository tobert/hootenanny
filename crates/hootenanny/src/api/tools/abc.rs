//! ABC notation MCP tools

use crate::api::schema::{AbcParseRequest, AbcToMidiRequest, AbcValidateRequest, AbcTransposeRequest};
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use baton::{ErrorData as McpError, CallToolResult, Content};

impl EventDualityServer {
    /// Parse ABC notation into a structured AST
    #[tracing::instrument(name = "mcp.tool.abc_parse", skip(self, request))]
    pub async fn abc_parse(
        &self,
        request: AbcParseRequest,
    ) -> Result<CallToolResult, McpError> {
        let result = abc::parse(&request.abc);

        let response = serde_json::json!({
            "tune": result.value,
            "feedback": result.feedback,
            "has_errors": result.has_errors(),
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    /// Convert ABC notation to MIDI
    #[tracing::instrument(name = "mcp.tool.abc_to_midi", skip(self, request))]
    pub async fn abc_to_midi(
        &self,
        request: AbcToMidiRequest,
    ) -> Result<CallToolResult, McpError> {
        let parse_result = abc::parse(&request.abc);

        if parse_result.has_errors() {
            let errors: Vec<_> = parse_result.errors().collect();
            return Err(McpError::invalid_params(
                format!("ABC parse errors: {:?}", errors)
            ));
        }

        let mut tune = parse_result.value;

        // Apply tempo override
        if let Some(bpm) = request.tempo_override {
            tune.header.tempo = Some(abc::Tempo {
                beat_unit: (1, 4),
                bpm,
                text: None,
            });
        }

        // Apply transposition (simple pitch shift for now)
        // TODO: Implement full transposition with key change

        // Generate MIDI
        let params = abc::MidiParams {
            velocity: request.velocity.unwrap_or(80),
            ticks_per_beat: 480,
            channel: request.channel.unwrap_or(0),
        };
        let midi_bytes = abc::to_midi(&tune, &params);

        // Store MIDI in CAS
        let midi_hash = self.local_models.store_cas_content(&midi_bytes, "audio/midi")
            .await
            .map_err(|e| McpError::internal_error(e.to_string()))?;

        // Store ABC source in CAS for provenance tracking
        let abc_hash = self.local_models.store_cas_content(request.abc.as_bytes(), "text/vnd.abc")
            .await
            .map_err(|e| McpError::internal_error(e.to_string()))?;

        // Count notes and bars for metadata
        let note_count = count_notes(&tune);
        let bar_count = count_bars(&tune);

        // Extract tempo if present
        let tempo_bpm = tune.header.tempo.as_ref().map(|t| t.bpm);

        // Create artifact
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

        // Acquire lock for artifact store operations
        let store = self.artifact_store.write()
            .map_err(|_| McpError::internal_error("Lock poisoned"))?;

        if let Some(ref set_id) = request.variation_set_id {
            let index = store.next_variation_index(set_id)
                .unwrap_or(0);
            artifact = artifact.with_variation_set(VariationSetId::new(set_id), index);
        }

        if let Some(ref parent_id) = request.parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent_id));
        }

        store.put(artifact)
            .map_err(|e| McpError::internal_error(e.to_string()))?;
        store.flush()
            .map_err(|e| McpError::internal_error(e.to_string()))?;

        let response = serde_json::json!({
            "midi_hash": midi_hash,
            "abc_hash": abc_hash,
            "artifact_id": artifact_id.as_str(),
            "note_count": note_count,
            "bar_count": bar_count,
            "feedback": parse_result.feedback,
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    /// Validate ABC notation
    #[tracing::instrument(name = "mcp.tool.abc_validate", skip(self, request))]
    pub async fn abc_validate(
        &self,
        request: AbcValidateRequest,
    ) -> Result<CallToolResult, McpError> {
        let result = abc::parse(&request.abc);

        let valid = !result.has_errors();
        let tune = &result.value;

        let response = serde_json::json!({
            "valid": valid,
            "feedback": result.feedback,
            "summary": {
                "title": tune.header.title,
                "key": format!("{:?} {:?}", tune.header.key.root, tune.header.key.mode),
                "meter": tune.header.meter,
                "bars": count_bars(tune),
                "notes": count_notes(tune),
            }
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    /// Transpose ABC notation
    #[tracing::instrument(name = "mcp.tool.abc_transpose", skip(self, request))]
    pub async fn abc_transpose(
        &self,
        request: AbcTransposeRequest,
    ) -> Result<CallToolResult, McpError> {
        let parse_result = abc::parse(&request.abc);

        if parse_result.has_errors() {
            let errors: Vec<_> = parse_result.errors().collect();
            return Err(McpError::invalid_params(
                format!("ABC parse errors: {:?}", errors)
            ));
        }

        let semitones = if let Some(s) = request.semitones {
            s
        } else if let Some(target) = &request.target_key {
            abc::semitones_to_key(&parse_result.value.header.key, target)
                .map_err(|e| McpError::invalid_params(e))?
        } else {
            return Err(McpError::invalid_params(
                "Must specify either semitones or target_key"
            ));
        };

        let original_key = format!("{:?} {:?}",
            parse_result.value.header.key.root,
            parse_result.value.header.key.mode
        );

        // TODO: Implement actual transposition
        // For now, return an error explaining this is not yet implemented
        let response = serde_json::json!({
            "error": "Transposition not yet implemented",
            "original_key": original_key,
            "requested_semitones": semitones,
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
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
