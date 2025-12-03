//! MCP Handler implementation for hootenanny
//!
//! Wraps EventDualityServer and implements baton::Handler trait.

use async_trait::async_trait;
use audio_graph_mcp::{graph_bind, graph_connect, graph_find, graph_tag, graph_connections, HintKind};
use baton::{
    CallToolResult, Content, ErrorData, Handler, Implementation,
    Prompt, PromptMessage, Resource, ResourceContents, ResourceTemplate, Tool, ToolSchema,
};
use baton::protocol::ToolContext;
use baton::types::prompt::GetPromptResult;
use baton::types::resource::ReadResourceResult;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use super::responses::*;
use super::schema::*;
use super::service::EventDualityServer;

/// Generate a ToolSchema from a type that implements schemars::JsonSchema.
///
/// Uses `inline_subschemas` to avoid `$defs`/`$ref` which some MCP clients
/// (like Gemini CLI) don't handle correctly.
fn schema_for<T: schemars::JsonSchema>() -> ToolSchema {
    let settings = schemars::generate::SchemaSettings::draft07().with(|s| {
        s.inline_subschemas = true;
    });
    let gen = settings.into_generator();
    let schema = gen.into_root_schema_for::<T>();
    let value = serde_json::to_value(&schema).unwrap_or_default();
    ToolSchema::from_value(value)
}

/// Handler wrapper that implements baton::Handler.
pub struct HootHandler {
    pub server: Arc<EventDualityServer>,
}

impl HootHandler {
    pub fn new(server: Arc<EventDualityServer>) -> Self {
        Self { server }
    }
}

#[async_trait]
impl Handler for HootHandler {
    fn tools(&self) -> Vec<Tool> {
        vec![
            // CAS tools
            Tool::new("cas_store", "Store content in the Content Addressable Storage")
                .with_input_schema(schema_for::<CasStoreRequest>())
                .with_output_schema(schema_for::<CasStoreResponse>()),
            Tool::new("cas_inspect", "Inspect content in the CAS by hash")
                .with_input_schema(schema_for::<CasInspectRequest>())
                .with_output_schema(schema_for::<CasInspectResponse>())
                .read_only(),
            Tool::new("cas_upload_file", "Upload a file to the CAS")
                .with_input_schema(schema_for::<UploadFileRequest>())
                .with_output_schema(schema_for::<CasUploadResponse>()),

            // Orpheus tools
            Tool::new("orpheus_generate", "Generate MIDI with the Orpheus model")
                .with_input_schema(schema_for::<OrpheusGenerateRequest>())
                .with_output_schema(schema_for::<JobSpawnResponse>()),
            Tool::new("orpheus_generate_seeded", "Generate MIDI from a seed with Orpheus")
                .with_input_schema(schema_for::<OrpheusGenerateSeededRequest>())
                .with_output_schema(schema_for::<JobSpawnResponse>()),
            Tool::new("orpheus_continue", "Generate continuation clip from MIDI (returns new material only, not concatenated)")
                .with_input_schema(schema_for::<OrpheusContinueRequest>())
                .with_output_schema(schema_for::<JobSpawnResponse>()),
            Tool::new("orpheus_bridge", "Generate a transition clip between MIDI sections (returns bridge only)")
                .with_input_schema(schema_for::<OrpheusBridgeRequest>())
                .with_output_schema(schema_for::<JobSpawnResponse>()),
            // TODO: Implement orpheus_loops
            // Tool::new("orpheus_loops", "Generate drum/percussion loops with Orpheus")
            //     .with_input_schema(schema_for::<OrpheusLoopsRequest>()),
            // TODO: Implement orpheus_classify
            // Tool::new("orpheus_classify", "Classify MIDI as human-composed or AI-generated")
            //     .with_input_schema(schema_for::<OrpheusClassifyRequest>())
            //     .read_only(),

            // Job tools
            Tool::new("job_status", "Get the status of an async job")
                .with_input_schema(schema_for::<GetJobStatusRequest>())
                .with_output_schema(schema_for::<JobStatusResponse>())
                .read_only(),
            Tool::new("job_list", "List all jobs")
                .with_output_schema(schema_for::<JobListResponse>())
                .read_only(),
            Tool::new("job_cancel", "Cancel a running job")
                .with_input_schema(schema_for::<CancelJobRequest>())
                .with_output_schema(schema_for::<JobCancelResponse>()),
            Tool::new("job_poll", "Poll for job completion")
                .with_input_schema(schema_for::<PollRequest>())
                .with_output_schema(schema_for::<JobPollResponse>())
                .read_only(),
            Tool::new("job_sleep", "Sleep for a specified duration")
                .with_input_schema(schema_for::<SleepRequest>())
                .read_only()
                .idempotent(),

            // Conversion tools
            Tool::new("convert_midi_to_wav", "Render MIDI to WAV using a SoundFont")
                .with_input_schema(schema_for::<MidiToWavRequest>())
                .with_output_schema(schema_for::<JobSpawnResponse>()),

            // SoundFont tools
            Tool::new("soundfont_inspect", "Inspect SoundFont presets and drum mappings")
                .with_input_schema(schema_for::<SoundfontInspectRequest>())
                .with_output_schema(schema_for::<SoundfontInspectResponse>())
                .read_only(),
            Tool::new("soundfont_preset_inspect", "Inspect a specific preset by bank/program")
                .with_input_schema(schema_for::<SoundfontPresetInspectRequest>())
                .with_output_schema(schema_for::<SoundfontPresetResponse>())
                .read_only(),

            Tool::new("graph_bind", "Bind an identity in the audio graph")
                .with_input_schema(schema_for::<GraphBindRequest>())
                .with_output_schema(schema_for::<GraphBindResponse>()),
            Tool::new("graph_tag", "Tag an identity in the audio graph")
                .with_input_schema(schema_for::<GraphTagRequest>())
                .with_output_schema(schema_for::<GraphTagResponse>()),
            Tool::new("graph_connect", "Connect nodes in the audio graph")
                .with_input_schema(schema_for::<GraphConnectRequest>())
                .with_output_schema(schema_for::<GraphConnectResponse>()),
            Tool::new("graph_find", "Find identities in the audio graph")
                .with_input_schema(schema_for::<GraphFindRequest>())
                .with_output_schema(schema_for::<GraphFindResponse>())
                .read_only(),

            // Graph context tools for sub-agent asset discovery
            Tool::new("graph_context", "Simple artifact discovery with filters (tag, creator, vibe). Returns bounded context for sub-agents. Use this for quick lookups; use graph_query for complex traversals.")
                .with_input_schema(schema_for::<GraphContextRequest>())
                .with_output_schema(schema_for::<GraphContextResponse>())
                .read_only(),
            Tool::new("add_annotation", "Add subjective annotations to artifacts with vibe keywords (e.g., 'warm, jazzy, vintage feel'). Makes artifacts searchable by mood/character.")
                .with_input_schema(schema_for::<AddAnnotationRequest>())
                .with_output_schema(schema_for::<AddAnnotationResponse>()),
            Tool::new("graph_query", "Execute GraphQL queries against the unified graph (artifacts, soundfonts, MIDI, audio). Query entry points: Artifact, SoundFont. Use @output for fields you want returned. Supports nested queries (annotations, tags, parent/children lineage).")
                .with_input_schema(schema_for::<GraphQueryRequest>())
                .with_output_schema(schema_for::<GraphQueryResponse>())
                .read_only(),

            // ABC notation tools
            Tool::new("abc_parse", "Parse ABC notation into a structured AST")
                .with_input_schema(schema_for::<AbcParseRequest>())
                .with_output_schema(schema_for::<AbcParseResponse>())
                .read_only(),
            Tool::new("abc_to_midi", "Convert ABC notation to MIDI")
                .with_input_schema(schema_for::<AbcToMidiRequest>())
                .with_output_schema(schema_for::<JobSpawnResponse>()),
            Tool::new("abc_validate", "Validate ABC notation and return feedback")
                .with_input_schema(schema_for::<AbcValidateRequest>())
                .with_output_schema(schema_for::<AbcValidateResponse>())
                .read_only(),
            Tool::new("abc_transpose", "Transpose ABC notation by semitones or to a target key")
                .with_input_schema(schema_for::<AbcTransposeRequest>())
                .with_output_schema(schema_for::<AbcTransposeResponse>()),

            // Beat detection tools (BeatThis model)
            Tool::new("beatthis_analyze", "Detect beats and downbeats in audio. Returns beat times in seconds, estimated BPM, and optionally frame-level probabilities.")
                .with_input_schema(schema_for::<AnalyzeBeatsRequest>())
                .with_output_schema(schema_for::<BeatthisAnalyzeResponse>())
                .read_only(),

            // Sampling tools (server-initiated LLM requests)
            Tool::new("sample_llm", "Request LLM inference from the connected client (requires client sampling support)")
                .with_input_schema(schema_for::<SampleLlmRequest>())
                .with_output_schema(schema_for::<SampleLlmResponse>())
                .read_only(),

            // TODO: Implement CLAP audio analysis
            // Tool::new("clap_analyze", "Analyze audio: extract embeddings, classify genre/mood, compare similarity")
            //     .with_input_schema(schema_for::<ClapAnalyzeRequest>())
            //     .read_only(),

            // TODO: Implement MusicGen text-to-music
            // Tool::new("musicgen_generate", "Generate music from text description using MusicGen")
            //     .with_input_schema(schema_for::<MusicgenGenerateRequest>()),

            // TODO: Implement Anticipatory Music Transformer
            // Tool::new("anticipatory_generate", "Generate polyphonic MIDI with Stanford's Anticipatory Music Transformer")
            //     .with_input_schema(schema_for::<AnticipatoryGenerateRequest>()),
            // Tool::new("anticipatory_continue", "Generate continuation clip from MIDI (returns new material only, not concatenated)")
            //     .with_input_schema(schema_for::<AnticipatoryContinueRequest>()),
            // Tool::new("anticipatory_embed", "Extract hidden state embeddings from MIDI")
            //     .with_input_schema(schema_for::<AnticipatoryEmbedRequest>())
            //     .read_only(),

            // TODO: Implement YuE text-to-song with vocals
            // Tool::new("yue_generate", "Generate a complete song with vocals from lyrics using YuE")
            //     .with_input_schema(schema_for::<YueGenerateRequest>()),
        ]
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<CallToolResult, ErrorData> {
        match name {
            // CAS tools
            "cas_store" => {
                let request: CasStoreRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.cas_store(request).await
            }
            "cas_inspect" => {
                let request: CasInspectRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.cas_inspect(request).await
            }
            "cas_upload_file" => {
                let request: UploadFileRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.upload_file(request).await
            }
            "convert_midi_to_wav" => {
                let request: MidiToWavRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.midi_to_wav(request).await
            }
            "soundfont_inspect" => {
                let request: SoundfontInspectRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.soundfont_inspect(request).await
            }
            "soundfont_preset_inspect" => {
                let request: SoundfontPresetInspectRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.soundfont_preset_inspect(request).await
            }

            // Orpheus tools
            "orpheus_generate" => {
                let request: OrpheusGenerateRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.orpheus_generate(request).await
            }
            "orpheus_generate_seeded" => {
                let request: OrpheusGenerateSeededRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.orpheus_generate_seeded(request).await
            }
            "orpheus_continue" => {
                let request: OrpheusContinueRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.orpheus_continue(request).await
            }
            "orpheus_bridge" => {
                let request: OrpheusBridgeRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.orpheus_bridge(request).await
            }
            // TODO: Implement orpheus_loops
            // "orpheus_loops" => {
            //     let request: OrpheusLoopsRequest = serde_json::from_value(args)
            //         .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            //     self.server.orpheus_loops(request).await
            // }
            // TODO: Implement orpheus_classify
            // "orpheus_classify" => {
            //     let request: OrpheusClassifyRequest = serde_json::from_value(args)
            //         .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            //     self.server.orpheus_classify(request).await
            // }

            // Job tools
            "job_status" => {
                let request: GetJobStatusRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.get_job_status(request).await
            }
            "job_list" => {
                self.server.list_jobs().await
            }
            "job_cancel" => {
                let request: CancelJobRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.cancel_job(request).await
            }
            "job_poll" => {
                let request: PollRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.poll(request).await
            }
            "job_sleep" => {
                let request: SleepRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.sleep(request).await
            }

            // Graph tools
            "graph_bind" => {
                let request: GraphBindRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                let hints: Vec<(HintKind, String, f64)> = request
                    .hints
                    .into_iter()
                    .filter_map(|h| {
                        h.kind.parse::<HintKind>().ok().map(|kind| (kind, h.value, h.confidence))
                    })
                    .collect();

                match graph_bind(&self.server.audio_graph_db, &request.id, &request.name, hints) {
                    Ok(identity) => {
                        let response = GraphBindResponse {
                            id: identity.id.0.clone(),
                            name: identity.name.clone(),
                            created_at: identity.created_at.clone(),
                        };
                        let text = format!("Identity bound: {} ({})", identity.name, identity.id.0);
                        Ok(CallToolResult::success(vec![Content::text(text)])
                            .with_structured(serde_json::to_value(&response).unwrap()))
                    }
                    Err(e) => Ok(CallToolResult::error(e)),
                }
            }
            "graph_tag" => {
                let request: GraphTagRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                let add = vec![(request.namespace.clone(), request.value.clone())];
                match graph_tag(&self.server.audio_graph_db, &request.identity_id, add, vec![]) {
                    Ok(tags) => {
                        let response = GraphTagResponse {
                            identity_id: request.identity_id.clone(),
                            tags: tags.iter().map(|t| TagInfo {
                                namespace: t.namespace.clone(),
                                value: t.value.clone(),
                            }).collect(),
                        };
                        let text = format!("Tagged {}: {}:{}", request.identity_id, request.namespace, request.value);
                        Ok(CallToolResult::success(vec![Content::text(text)])
                            .with_structured(serde_json::to_value(&response).unwrap()))
                    }
                    Err(e) => Ok(CallToolResult::error(e)),
                }
            }
            "graph_connect" => {
                let request: GraphConnectRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match graph_connect(
                    &self.server.audio_graph_db,
                    &request.from_identity,
                    &request.from_port,
                    &request.to_identity,
                    &request.to_port,
                    request.transport.as_deref(),
                ) {
                    Ok(conn) => {
                        let response = GraphConnectResponse {
                            connection_id: conn.id.clone(),
                            from: format!("{}:{}", conn.from_identity.0, conn.from_port),
                            to: format!("{}:{}", conn.to_identity.0, conn.to_port),
                            transport: conn.transport_kind.clone().unwrap_or_else(|| "unknown".to_string()),
                        };
                        let text = format!("Connected: {} -> {}", response.from, response.to);
                        Ok(CallToolResult::success(vec![Content::text(text)])
                            .with_structured(serde_json::to_value(&response).unwrap()))
                    }
                    Err(e) => Ok(CallToolResult::error(e)),
                }
            }
            "graph_find" => {
                let request: GraphFindRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match graph_find(
                    &self.server.audio_graph_db,
                    request.name.as_deref(),
                    request.tag_namespace.as_deref(),
                    request.tag_value.as_deref(),
                ) {
                    Ok(identities) => {
                        let response = GraphFindResponse {
                            identities: identities.iter().map(|i| IdentitySummary {
                                id: i.id.clone(),
                                name: i.name.clone(),
                                tags: i.tags.iter().map(|t| format!("{}:{}", t.namespace, t.value)).collect(),
                            }).collect(),
                            count: identities.len(),
                        };
                        let text = format!("Found {} identities", identities.len());
                        Ok(CallToolResult::success(vec![Content::text(text)])
                            .with_structured(serde_json::to_value(&response).unwrap()))
                    }
                    Err(e) => Ok(CallToolResult::error(e)),
                }
            }

            // Graph context tools for sub-agent asset discovery
            "graph_context" => {
                let request: GraphContextRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.graph_context(request).await
            }
            "add_annotation" => {
                let request: AddAnnotationRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.add_annotation(request).await
            }
            "graph_query" => {
                let request: GraphQueryRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.graph_query(request).await
            }

            // ABC notation tools
            "abc_parse" => {
                let request: AbcParseRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.abc_parse(request).await
            }
            "abc_to_midi" => {
                let request: AbcToMidiRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.abc_to_midi(request).await
            }
            "abc_validate" => {
                let request: AbcValidateRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.abc_validate(request).await
            }
            "abc_transpose" => {
                let request: AbcTransposeRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.abc_transpose(request).await
            }

            // Beat detection tools (BeatThis model)
            "beatthis_analyze" => {
                let request: AnalyzeBeatsRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.analyze_beats(request).await
            }

            // TODO: Implement CLAP audio analysis
            // "clap_analyze" => {
            //     let request: ClapAnalyzeRequest = serde_json::from_value(args)
            //         .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            //     self.server.clap_analyze(request).await
            // }

            // TODO: Implement MusicGen text-to-music
            // "musicgen_generate" => {
            //     let request: MusicgenGenerateRequest = serde_json::from_value(args)
            //         .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            //     self.server.musicgen_generate(request).await
            // }

            // TODO: Implement Anticipatory Music Transformer
            // "anticipatory_generate" => {
            //     let request: AnticipatoryGenerateRequest = serde_json::from_value(args)
            //         .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            //     self.server.anticipatory_generate(request).await
            // }
            // "anticipatory_continue" => {
            //     let request: AnticipatoryContinueRequest = serde_json::from_value(args)
            //         .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            //     self.server.anticipatory_continue(request).await
            // }
            // "anticipatory_embed" => {
            //     let request: AnticipatoryEmbedRequest = serde_json::from_value(args)
            //         .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            //     self.server.anticipatory_embed(request).await
            // }

            // TODO: Implement YuE text-to-song with vocals
            // "yue_generate" => {
            //     let request: YueGenerateRequest = serde_json::from_value(args)
            //         .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            //     self.server.yue_generate(request).await
            // }

            _ => Err(ErrorData::tool_not_found(name)),
        }
    }

    async fn call_tool_with_context(
        &self,
        name: &str,
        args: Value,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        // Log progress capability
        if context.has_progress() {
            tracing::debug!(
                tool = name,
                session_id = %context.session_id,
                "Tool called with progress token"
            );
        }

        // Route job-spawning tools to progress-aware implementations
        match name {
            "orpheus_generate" if context.has_progress() => {
                let request: OrpheusGenerateRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.orpheus_generate_with_progress(request, context.progress_sender).await
            }
            "orpheus_generate_seeded" if context.has_progress() => {
                let request: OrpheusGenerateSeededRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.orpheus_generate_seeded_with_progress(request, context.progress_sender).await
            }
            "orpheus_continue" if context.has_progress() => {
                let request: OrpheusContinueRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.orpheus_continue_with_progress(request, context.progress_sender).await
            }
            "orpheus_bridge" if context.has_progress() => {
                let request: OrpheusBridgeRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.orpheus_bridge_with_progress(request, context.progress_sender).await
            }
            "convert_midi_to_wav" if context.has_progress() => {
                let request: MidiToWavRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.midi_to_wav_with_progress(request, context.progress_sender).await
            }
            // Sampling tools (always need context for Sampler)
            "sample_llm" => {
                let request: SampleLlmRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
                self.server.sample_llm(request, context).await
            }

            // TODO: Add other tools when their modules are implemented:
            // - musicgen_generate, anticipatory_generate, anticipatory_continue
            // - yue_generate, beatthis_analyze, clap_analyze

            // Fall through to standard implementation for tools without progress or no token
            _ => self.call_tool(name, args).await
        }
    }

    fn server_info(&self) -> Implementation {
        Implementation::new("hootenanny", env!("CARGO_PKG_VERSION"))
            .with_title("HalfRemembered MCP Server")
    }

    fn instructions(&self) -> Option<String> {
        Some(
            "Hootenanny is an ensemble performance space for LLM agents and humans to create music together.".to_string()
        )
    }

    fn resources(&self) -> Vec<Resource> {
        vec![
            // Graph resources
            Resource::new("graph://identities", "identities")
                .with_description("All audio device identities with tags")
                .with_mime_type("application/json"),
            Resource::new("graph://connections", "connections")
                .with_description("All patch cable connections between devices")
                .with_mime_type("application/json"),

            // Artifact resources
            Resource::new("artifacts://summary", "artifact-summary")
                .with_description("Aggregate stats on all artifacts by type, phase, and tool")
                .with_mime_type("application/json"),
            Resource::new("artifacts://recent", "recent-artifacts")
                .with_description("10 most recently created artifacts")
                .with_mime_type("application/json"),
        ]
    }

    fn resource_templates(&self) -> Vec<ResourceTemplate> {
        vec![
            // Graph templates
            ResourceTemplate::new("graph://identity/{id}", "identity-by-id")
                .with_description("Single identity with hints and tags")
                .with_mime_type("application/json"),

            // CAS templates
            ResourceTemplate::new("cas://{hash}", "cas-content")
                .with_description("Content from CAS by hash"),

            // Artifact templates
            ResourceTemplate::new("artifacts://by-tag/{tag}", "artifacts-by-tag")
                .with_description("Filter artifacts by tag (e.g., type:midi, phase:generation)")
                .with_mime_type("application/json"),
            ResourceTemplate::new("artifacts://by-creator/{creator}", "artifacts-by-creator")
                .with_description("All artifacts created by a specific agent")
                .with_mime_type("application/json"),
            ResourceTemplate::new("artifacts://variation-set/{set_id}", "variation-set")
                .with_description("All artifacts in a variation set")
                .with_mime_type("application/json"),
            ResourceTemplate::new("artifacts://lineage/{artifact_id}", "artifact-lineage")
                .with_description("Parent chain showing refinement history")
                .with_mime_type("application/json"),
            ResourceTemplate::new("artifacts://detail/{artifact_id}", "artifact-detail")
                .with_description("Full artifact metadata with CAS info")
                .with_mime_type("application/json"),
        ]
    }

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, ErrorData> {
        // Parse the URI scheme and path
        let (scheme, path) = uri.split_once("://")
            .ok_or_else(|| ErrorData::invalid_params(format!("Invalid URI: {}", uri)))?;

        match scheme {
            "graph" => self.read_graph_resource(path).await,
            "cas" => self.read_cas_resource(path).await,
            "artifacts" => self.read_artifacts_resource(path).await,
            _ => Err(ErrorData::invalid_params(format!("Unknown URI scheme: {}", scheme))),
        }
    }

    fn prompts(&self) -> Vec<Prompt> {
        vec![
            Prompt::new("ensemble-jam")
                .with_title("Start Ensemble Jam")
                .with_description("Start a collaborative music session with available instruments")
                .argument("style", "Musical style (ambient, techno, jazz, experimental)", true)
                .argument("tempo", "BPM (beats per minute)", false)
                .argument("duration_bars", "Length in bars", false),
            Prompt::new("describe-setup")
                .with_title("Describe Studio Setup")
                .with_description("Generate documentation of the current audio graph and connections")
                .argument("format", "Output format (markdown, json)", false),
            Prompt::new("patch-synth")
                .with_title("Suggest Synth Patch")
                .with_description("Generate a synth patch idea for a specific instrument")
                .argument("synth_id", "Identity ID of the synthesizer", true)
                .argument("style", "Patch style (pad, lead, bass, fx)", true)
                .argument("character", "Sound character (warm, bright, dark, aggressive)", false),
            Prompt::new("sequence-idea")
                .with_title("Generate Sequence Idea")
                .with_description("Spark a MIDI sequence idea")
                .argument("style", "Musical style", true)
                .argument("key", "Musical key (C, Dm, F#, etc.)", false)
                .argument("bars", "Length in bars", false),

            // Music-Aware Prompts (Task 04)
            Prompt::new("generate-continuation")
                .with_title("Continue MIDI")
                .with_description("Suggest how to extend or continue existing MIDI")
                .argument("hash", "CAS hash of MIDI to continue", true)
                .argument("bars", "Number of bars to add (default: 4)", false)
                .argument("direction", "build, wind-down, transition, develop (default: develop)", false),
            Prompt::new("orchestrate-parts")
                .with_title("Orchestrate Parts")
                .with_description("Suggest complementary parts for an arrangement")
                .argument("base_hash", "Main MIDI track (CAS hash)", true)
                .argument("roles", "Comma-separated roles: drums, bass, melody, pad, fx", false),
            Prompt::new("explore-variations")
                .with_title("Explore Variations")
                .with_description("Suggest variation strategies for existing MIDI")
                .argument("hash", "MIDI to vary (CAS hash)", true)
                .argument("intensity", "subtle, moderate, radical (default: moderate)", false),
            Prompt::new("analyze-generation")
                .with_title("Analyze Generation")
                .with_description("Analyze characteristics and suggest improvements")
                .argument("hash", "MIDI to analyze (CAS hash)", true),
            Prompt::new("next-in-session")
                .with_title("What's Next?")
                .with_description("Suggest next creative steps based on session state"),
        ]
    }

    async fn get_prompt(
        &self,
        name: &str,
        arguments: HashMap<String, String>,
    ) -> Result<GetPromptResult, ErrorData> {
        // Get current audio graph context for dynamic prompts
        let identities = graph_find(&self.server.audio_graph_db, None, None, None)
            .unwrap_or_default();
        let connections = graph_connections(&self.server.audio_graph_db, None)
            .unwrap_or_default();

        let devices_summary = if identities.is_empty() {
            "No audio devices registered yet.".to_string()
        } else {
            identities
                .iter()
                .map(|i| format!("- {} ({})", i.name, i.id))
                .collect::<Vec<_>>()
                .join("\n")
        };

        match name {
            "ensemble-jam" => {
                let style = arguments.get("style").map(|s| s.as_str()).unwrap_or("ambient");
                let tempo = arguments.get("tempo").map(|s| s.as_str()).unwrap_or("120");
                let duration = arguments.get("duration_bars").map(|s| s.as_str()).unwrap_or("8");

                let prompt_text = format!(
                    "Let's create a {} piece at {} BPM, {} bars long.\n\n\
                    Available instruments:\n{}\n\n\
                    Connections: {} patch cables\n\n\
                    Start by establishing a groove or texture, then build from there. \
                    Use the tools to generate MIDI, play notes, and coordinate the ensemble.",
                    style, tempo, duration, devices_summary, connections.len()
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description(format!("{} jam session at {} BPM", style, tempo)))
            }
            "describe-setup" => {
                let format = arguments.get("format").map(|s| s.as_str()).unwrap_or("markdown");

                let prompt_text = format!(
                    "Please describe the current studio setup in {} format.\n\n\
                    Registered devices:\n{}\n\n\
                    Connections: {} patch cables\n\n\
                    Include device roles, signal flow, and any notable capabilities.",
                    format, devices_summary, connections.len()
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description("Studio documentation request"))
            }
            "patch-synth" => {
                let synth_id = arguments.get("synth_id")
                    .ok_or_else(|| ErrorData::invalid_params("synth_id is required"))?;
                let style = arguments.get("style").map(|s| s.as_str()).unwrap_or("pad");
                let character = arguments.get("character").map(|s| s.as_str()).unwrap_or("warm");

                let synth_name = identities
                    .iter()
                    .find(|i| i.id == *synth_id)
                    .map(|i| i.name.as_str())
                    .unwrap_or(synth_id);

                let prompt_text = format!(
                    "Create a {} patch for {} with a {} character.\n\n\
                    Consider:\n\
                    - Oscillator configuration and waveforms\n\
                    - Filter settings and modulation\n\
                    - Envelope shapes for amp and filter\n\
                    - Effects (reverb, delay, chorus)\n\n\
                    Describe the patch parameters and the sound it will produce.",
                    style, synth_name, character
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description(format!("{} {} patch for {}", character, style, synth_name)))
            }
            "sequence-idea" => {
                let style = arguments.get("style").map(|s| s.as_str()).unwrap_or("ambient");
                let key = arguments.get("key").map(|s| s.as_str()).unwrap_or("C");
                let bars = arguments.get("bars").map(|s| s.as_str()).unwrap_or("4");

                let prompt_text = format!(
                    "Generate a {} MIDI sequence idea in the key of {}, {} bars long.\n\n\
                    Available instruments:\n{}\n\n\
                    Describe:\n\
                    - The melodic or rhythmic pattern\n\
                    - Note choices and timing\n\
                    - Velocity dynamics\n\
                    - How it might interact with other parts\n\n\
                    Then use orpheus_generate to create the actual MIDI.",
                    style, key, bars, devices_summary
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description(format!("{} sequence in {} ({} bars)", style, key, bars)))
            }

            // Music-Aware Prompts (Task 04)
            "generate-continuation" => {
                use crate::artifact_store::ArtifactStore;

                let hash = arguments.get("hash")
                    .ok_or_else(|| ErrorData::invalid_params("hash is required"))?;
                let bars = arguments.get("bars").map(|s| s.as_str()).unwrap_or("4");
                let direction = arguments.get("direction").map(|s| s.as_str()).unwrap_or("develop");

                let store = self.server.artifact_store.read()
                    .map_err(|_| ErrorData::internal_error("Lock poisoned"))?;
                let artifacts = store.all().unwrap_or_default();
                let artifact = artifacts.iter()
                    .find(|a| a.content_hash.as_str() == hash);

                let artifact_info = if let Some(a) = artifact {
                    format!(
                        "Created by: {}\nTags: {}\nPhase: {}",
                        a.creator,
                        a.tags.join(", "),
                        a.phase().unwrap_or("unknown")
                    )
                } else {
                    "No metadata available".to_string()
                };

                let prompt_text = format!(
                    "Continue this MIDI for {} more bars with a '{}' direction.\n\n\
                    Source MIDI: cas://{}\n\
                    {}\n\n\
                    Available instruments:\n{}\n\n\
                    Direction guide:\n\
                    - build: Increase energy, add layers\n\
                    - wind-down: Decrease energy, thin out\n\
                    - transition: Prepare for a new section\n\
                    - develop: Evolve the existing material\n\n\
                    Use orpheus_continue with input_hash=\"{}\" to generate the continuation.",
                    bars, direction, hash, artifact_info, devices_summary, hash
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description(format!("{} continuation ({} bars)", direction, bars)))
            }

            "orchestrate-parts" => {
                use crate::artifact_store::ArtifactStore;

                let base_hash = arguments.get("base_hash")
                    .ok_or_else(|| ErrorData::invalid_params("base_hash is required"))?;
                let roles = arguments.get("roles").map(|s| s.as_str()).unwrap_or("bass, pad");

                let store = self.server.artifact_store.read()
                    .map_err(|_| ErrorData::internal_error("Lock poisoned"))?;
                let artifacts = store.all().unwrap_or_default();
                let existing_parts: HashMap<String, usize> = artifacts.iter()
                    .filter(|a| a.has_tag("type:midi"))
                    .flat_map(|a| a.tags_with_prefix("role:"))
                    .fold(HashMap::new(), |mut map, role| {
                        *map.entry(role.to_string()).or_insert(0) += 1;
                        map
                    });

                let existing_summary = if existing_parts.is_empty() {
                    "No parts tagged with roles yet.".to_string()
                } else {
                    existing_parts.iter()
                        .map(|(role, count)| format!("- {}: {} variations", role, count))
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                let prompt_text = format!(
                    "Orchestrate complementary parts for the base track.\n\n\
                    Base track: cas://{}\n\n\
                    Requested roles: {}\n\n\
                    Existing parts:\n{}\n\n\
                    Available instruments:\n{}\n\n\
                    For each role, consider:\n\
                    - How it complements the base track\n\
                    - Register and frequency range\n\
                    - Rhythmic relationship\n\
                    - Harmonic function\n\n\
                    Use orpheus_generate or orpheus_generate_seeded with appropriate tags like \"role:bass\".",
                    base_hash, roles, existing_summary, devices_summary
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description(format!("Orchestrate: {}", roles)))
            }

            "explore-variations" => {
                use crate::artifact_store::ArtifactStore;

                let hash = arguments.get("hash")
                    .ok_or_else(|| ErrorData::invalid_params("hash is required"))?;
                let intensity = arguments.get("intensity").map(|s| s.as_str()).unwrap_or("moderate");

                let store = self.server.artifact_store.read()
                    .map_err(|_| ErrorData::internal_error("Lock poisoned"))?;
                let artifacts = store.all().unwrap_or_default();
                let artifact = artifacts.iter()
                    .find(|a| a.content_hash.as_str() == hash);

                let variation_info = if let Some(a) = artifact {
                    if let Some(set_id) = &a.variation_set_id {
                        let set_count = artifacts.iter()
                            .filter(|v| v.variation_set_id.as_ref().map(|s| s.as_str()) == Some(set_id.as_str()))
                            .count();
                        format!("Part of variation set '{}' with {} existing variations.", set_id.as_str(), set_count)
                    } else {
                        "Not in a variation set yet.".to_string()
                    }
                } else {
                    "No artifact metadata.".to_string()
                };

                let intensity_guide = match intensity {
                    "subtle" => "Small changes: slight timing shifts, velocity variations, octave doubling",
                    "moderate" => "Medium changes: melodic embellishment, rhythm augmentation, harmonic recoloring",
                    "radical" => "Major changes: completely new interpretation while keeping core identity",
                    _ => "Moderate variation",
                };

                let prompt_text = format!(
                    "Create {} variations of this MIDI.\n\n\
                    Source: cas://{}\n\
                    {}\n\n\
                    Intensity: {} - {}\n\n\
                    Variation strategies:\n\
                    - Melodic: Ornament, simplify, invert, sequence\n\
                    - Rhythmic: Augment, diminish, syncopate, straighten\n\
                    - Harmonic: Reharmonize, add extensions, substitute\n\
                    - Textural: Layer, thin, transpose octave\n\n\
                    Use orpheus_generate_seeded with seed_hash=\"{}\" and num_variations=3.",
                    intensity, hash, variation_info, intensity, intensity_guide, hash
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description(format!("{} variations", intensity)))
            }

            "analyze-generation" => {
                use crate::artifact_store::ArtifactStore;

                let hash = arguments.get("hash")
                    .ok_or_else(|| ErrorData::invalid_params("hash is required"))?;

                let store = self.server.artifact_store.read()
                    .map_err(|_| ErrorData::internal_error("Lock poisoned"))?;
                let artifacts = store.all().unwrap_or_default();
                let artifact = artifacts.iter()
                    .find(|a| a.content_hash.as_str() == hash);

                let artifact_details = if let Some(a) = artifact {
                    format!(
                        "Creator: {}\nCreated: {}\nTags: {}\nModel: {}\nTemperature: {}\nTokens: {}",
                        a.creator,
                        a.created_at.to_rfc3339(),
                        a.tags.join(", "),
                        a.metadata.get("model").and_then(|v| v.as_str()).unwrap_or("unknown"),
                        a.metadata.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        a.metadata.get("tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                    )
                } else {
                    "No artifact metadata available.".to_string()
                };

                let prompt_text = format!(
                    "Analyze this generated MIDI.\n\n\
                    Hash: cas://{}\n\n\
                    Artifact info:\n{}\n\n\
                    Consider:\n\
                    - Melodic contour and range\n\
                    - Rhythmic density and patterns\n\
                    - Harmonic implications\n\
                    - Energy arc\n\
                    - Potential improvements\n\n\
                    If the MIDI feels machine-like, suggest how to humanize it.\n\
                    If it's too chaotic, suggest how to add structure.",
                    hash, artifact_details
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description("MIDI analysis"))
            }

            "next-in-session" => {
                use crate::artifact_store::ArtifactStore;

                let store = self.server.artifact_store.read()
                    .map_err(|_| ErrorData::internal_error("Lock poisoned"))?;
                let artifacts = store.all().unwrap_or_default();
                let midi_artifacts: Vec<_> = artifacts.iter()
                    .filter(|a| a.has_tag("type:midi"))
                    .collect();

                let by_phase: HashMap<String, usize> = midi_artifacts.iter()
                    .fold(HashMap::new(), |mut map, a| {
                        if let Some(phase) = a.phase() {
                            let phase_name = phase.strip_prefix("phase:").unwrap_or(phase);
                            *map.entry(phase_name.to_string()).or_insert(0) += 1;
                        }
                        map
                    });

                let mut recent: Vec<_> = midi_artifacts.to_vec();
                recent.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                let recent_summary: Vec<_> = recent.iter()
                    .take(3)
                    .map(|a| format!("- {} ({})", a.id, a.tags.join(", ")))
                    .collect();

                let prompt_text = format!(
                    "What should we do next in this session?\n\n\
                    Session state:\n\
                    - Total MIDI generated: {}\n\
                    - By phase: {:?}\n\
                    - Available instruments: {}\n\n\
                    Recent activity:\n{}\n\n\
                    Suggest next steps based on:\n\
                    - What's been created so far\n\
                    - What roles/parts might be missing\n\
                    - Opportunities for variation or refinement\n\
                    - Whether to explore new directions or develop existing material",
                    midi_artifacts.len(),
                    by_phase,
                    identities.len(),
                    if recent_summary.is_empty() { "No recent activity".to_string() } else { recent_summary.join("\n") }
                );

                Ok(GetPromptResult::new(vec![PromptMessage::user_text(prompt_text)])
                    .with_description("Session guidance"))
            }

            _ => Err(ErrorData::invalid_params(format!("Unknown prompt: {}", name))),
        }
    }
}

impl HootHandler {
    async fn read_graph_resource(&self, path: &str) -> Result<ReadResourceResult, ErrorData> {
        match path {
            "identities" => {
                match graph_find(&self.server.audio_graph_db, None, None, None) {
                    Ok(identities) => {
                        let json = serde_json::to_string_pretty(&identities)
                            .map_err(|e| ErrorData::internal_error(e.to_string()))?;
                        Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
                            "graph://identities",
                            json,
                            "application/json",
                        )))
                    }
                    Err(e) => Err(ErrorData::internal_error(e)),
                }
            }
            "connections" => {
                match graph_connections(&self.server.audio_graph_db, None) {
                    Ok(connections) => {
                        let result: Vec<_> = connections
                            .iter()
                            .map(|c| serde_json::json!({
                                "id": c.id,
                                "from": format!("{}:{}", c.from_identity.0, c.from_port),
                                "to": format!("{}:{}", c.to_identity.0, c.to_port),
                                "transport": c.transport_kind,
                            }))
                            .collect();
                        let json = serde_json::to_string_pretty(&result)
                            .map_err(|e| ErrorData::internal_error(e.to_string()))?;
                        Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
                            "graph://connections",
                            json,
                            "application/json",
                        )))
                    }
                    Err(e) => Err(ErrorData::internal_error(e)),
                }
            }
            _ if path.starts_with("identity/") => {
                let id = &path[9..]; // Strip "identity/"
                match self.server.audio_graph_db.get_identity(id) {
                    Ok(Some(identity)) => {
                        let tags = self.server.audio_graph_db.get_tags(id).unwrap_or_default();
                        let hints = self.server.audio_graph_db.get_hints(id).unwrap_or_default();
                        let result = serde_json::json!({
                            "id": identity.id.0,
                            "name": identity.name,
                            "created_at": identity.created_at,
                            "tags": tags.iter().map(|t| serde_json::json!({
                                "namespace": t.namespace,
                                "value": t.value,
                            })).collect::<Vec<_>>(),
                            "hints": hints.iter().map(|h| serde_json::json!({
                                "kind": h.kind.as_str(),
                                "value": h.value,
                                "confidence": h.confidence,
                            })).collect::<Vec<_>>(),
                        });
                        let json = serde_json::to_string_pretty(&result)
                            .map_err(|e| ErrorData::internal_error(e.to_string()))?;
                        Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
                            format!("graph://identity/{}", id),
                            json,
                            "application/json",
                        )))
                    }
                    Ok(None) => Err(ErrorData::invalid_params(format!("Identity not found: {}", id))),
                    Err(e) => Err(ErrorData::internal_error(e.to_string())),
                }
            }
            _ => Err(ErrorData::invalid_params(format!("Unknown graph resource: {}", path))),
        }
    }

    async fn read_cas_resource(&self, hash: &str) -> Result<ReadResourceResult, ErrorData> {
        // Use cas_inspect tool to get CAS content info
        let request = CasInspectRequest { hash: hash.to_string() };
        match self.server.cas_inspect(request).await {
            Ok(result) => {
                // The cas_inspect result contains metadata, return it as the resource
                if let Some(Content::Text { text, .. }) = result.content.first() {
                    return Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
                        format!("cas://{}", hash),
                        text.clone(),
                        "application/json",
                    )));
                }
                Err(ErrorData::internal_error("No text content in CAS inspect result"))
            }
            Err(e) => Err(e),
        }
    }

    async fn read_artifacts_resource(&self, path: &str) -> Result<ReadResourceResult, ErrorData> {
        use crate::artifact_store::ArtifactStore;
        use std::collections::HashSet;

        let store = self.server.artifact_store.read()
            .map_err(|_| ErrorData::internal_error("Lock poisoned"))?;

        match path {
            "summary" => {
                let all = store.all()
                    .map_err(|e| ErrorData::internal_error(e.to_string()))?;

                let mut by_type: HashMap<String, usize> = HashMap::new();
                let mut by_phase: HashMap<String, usize> = HashMap::new();
                let mut by_tool: HashMap<String, usize> = HashMap::new();

                for artifact in &all {
                    for tag in &artifact.tags {
                        if let Some(val) = tag.strip_prefix("type:") {
                            *by_type.entry(val.to_string()).or_insert(0) += 1;
                        } else if let Some(val) = tag.strip_prefix("phase:") {
                            *by_phase.entry(val.to_string()).or_insert(0) += 1;
                        } else if let Some(val) = tag.strip_prefix("tool:") {
                            *by_tool.entry(val.to_string()).or_insert(0) += 1;
                        }
                    }
                }

                let variation_sets: HashSet<_> = all.iter()
                    .filter_map(|a| a.variation_set_id.as_ref())
                    .collect();

                let result = serde_json::json!({
                    "total": all.len(),
                    "by_type": by_type,
                    "by_phase": by_phase,
                    "by_tool": by_tool,
                    "variation_sets": variation_sets.len(),
                });
                Ok(Self::as_json_resource("artifacts://summary", &result)?)
            }

            "recent" => {
                let mut all = store.all()
                    .map_err(|e| ErrorData::internal_error(e.to_string()))?;
                all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                let recent: Vec<_> = all.into_iter().take(10).collect();

                let result: Vec<_> = recent.iter()
                    .map(Self::artifact_to_json)
                    .collect();
                Ok(Self::as_json_resource("artifacts://recent", &serde_json::json!(result))?)
            }

            _ if path.starts_with("by-tag/") => {
                let tag = &path[7..];
                let all = store.all()
                    .map_err(|e| ErrorData::internal_error(e.to_string()))?;
                let filtered: Vec<_> = all.into_iter()
                    .filter(|a| a.has_tag(tag))
                    .map(|a| Self::artifact_to_json(&a))
                    .collect();
                Ok(Self::as_json_resource(&format!("artifacts://by-tag/{}", tag), &serde_json::json!(filtered))?)
            }

            _ if path.starts_with("by-creator/") => {
                let creator = &path[11..];
                let all = store.all()
                    .map_err(|e| ErrorData::internal_error(e.to_string()))?;
                let filtered: Vec<_> = all.into_iter()
                    .filter(|a| a.creator == creator)
                    .map(|a| Self::artifact_to_json(&a))
                    .collect();
                Ok(Self::as_json_resource(&format!("artifacts://by-creator/{}", creator), &serde_json::json!(filtered))?)
            }

            _ if path.starts_with("variation-set/") => {
                let set_id = &path[14..];
                let all = store.all()
                    .map_err(|e| ErrorData::internal_error(e.to_string()))?;
                let mut filtered: Vec<_> = all.into_iter()
                    .filter(|a| a.variation_set_id.as_ref().map(|s| s.as_str()) == Some(set_id))
                    .collect();
                filtered.sort_by_key(|a| a.variation_index);

                let result = serde_json::json!({
                    "set_id": set_id,
                    "count": filtered.len(),
                    "variations": filtered.iter().map(Self::artifact_to_json).collect::<Vec<_>>(),
                });
                Ok(Self::as_json_resource(&format!("artifacts://variation-set/{}", set_id), &result)?)
            }

            _ if path.starts_with("lineage/") => {
                let artifact_id = &path[8..];
                let all = store.all()
                    .map_err(|e| ErrorData::internal_error(e.to_string()))?;

                let mut chain = Vec::new();
                let mut current_id = Some(artifact_id.to_string());

                while let Some(id) = current_id {
                    if let Some(artifact) = all.iter().find(|a| a.id.as_str() == id) {
                        chain.push(Self::artifact_to_json(artifact));
                        current_id = artifact.parent_id.as_ref().map(|p| p.as_str().to_string());
                    } else {
                        break;
                    }
                }
                chain.reverse();

                let result = serde_json::json!({
                    "artifact_id": artifact_id,
                    "depth": chain.len(),
                    "chain": chain,
                });
                Ok(Self::as_json_resource(&format!("artifacts://lineage/{}", artifact_id), &result)?)
            }

            _ if path.starts_with("detail/") => {
                let artifact_id = &path[7..];
                let artifact = store.get(artifact_id)
                    .map_err(|e| ErrorData::internal_error(e.to_string()))?
                    .ok_or_else(|| ErrorData::invalid_params(format!("Artifact not found: {}", artifact_id)))?;

                let hash = artifact.content_hash.as_str();
                let cas_info = serde_json::json!({
                    "hash": hash,
                    "uri": format!("cas://{}", hash),
                    "content_url": format!("/artifact/{}", artifact.id.as_str()),
                });

                let result = serde_json::json!({
                    "artifact": Self::artifact_to_json(&artifact),
                    "cas": cas_info,
                });
                Ok(Self::as_json_resource(&format!("artifacts://detail/{}", artifact_id), &result)?)
            }

            _ => Err(ErrorData::invalid_params(format!("Unknown artifacts resource: {}", path))),
        }
    }

    fn as_json_resource(uri: &str, value: &serde_json::Value) -> Result<ReadResourceResult, ErrorData> {
        let json = serde_json::to_string_pretty(value)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;
        Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
            uri,
            json,
            "application/json",
        )))
    }

    fn artifact_to_json(a: &crate::artifact_store::Artifact) -> serde_json::Value {
        serde_json::json!({
            "id": a.id.as_str(),
            "content_hash": a.content_hash.as_str(),
            "content_url": format!("/artifact/{}", a.id.as_str()),
            "creator": a.creator,
            "created_at": a.created_at.to_rfc3339(),
            "tags": a.tags,
            "variation_set_id": a.variation_set_id.as_ref().map(|s| s.as_str()),
            "variation_index": a.variation_index,
            "parent_id": a.parent_id.as_ref().map(|s| s.as_str()),
            "metadata": a.metadata,
            "access_count": a.access_count,
            "last_accessed": a.last_accessed.map(|t| t.to_rfc3339()),
        })
    }
}
