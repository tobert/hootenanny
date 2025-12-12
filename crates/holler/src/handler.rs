//! MCP Handler implementation for ZMQ backend forwarding
//!
//! Implements baton::Handler to bridge MCP protocol to ZMQ backends.
//! Tools are dynamically discovered from backends and calls are routed based on prefix.

use async_trait::async_trait;
use baton::{CallToolResult, Content, ErrorData, Handler, Implementation, Tool, ToolSchema};
use hooteproto::{Payload, ToolInfo};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::backend::BackendPool;

/// MCP Handler that forwards tool calls to ZMQ backends.
pub struct ZmqHandler {
    backends: Arc<BackendPool>,
}

impl ZmqHandler {
    /// Create a new handler with the given backend pool.
    pub fn new(backends: Arc<BackendPool>) -> Self {
        Self { backends }
    }
}

#[async_trait]
impl Handler for ZmqHandler {
    fn tools(&self) -> Vec<Tool> {
        // Tools are fetched dynamically, but baton's Handler trait expects
        // a synchronous list. We'll cache the last known tools or return empty
        // and rely on the actual call routing. For now, return empty and override
        // tool listing via a custom approach.
        //
        // Actually, we need to block on the async call here. That's problematic.
        // Let's use tokio's Handle to block within the sync context.
        let backends = Arc::clone(&self.backends);

        // Try to get runtime handle - if we're in async context this works
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We're in an async context, spawn a blocking task
            std::thread::spawn(move || {
                handle.block_on(async {
                    collect_tools_async(&backends).await
                })
            })
            .join()
            .unwrap_or_default()
        } else {
            // Not in async context, return empty
            vec![]
        }
    }

    async fn call_tool(&self, name: &str, arguments: Value) -> Result<CallToolResult, ErrorData> {
        self.call_tool_with_context(name, arguments, baton::ToolContext {
            session_id: String::new(),
            progress_token: None,
            progress_sender: None,
            sampler: None,
            logger: baton::transport::McpLogger::new(Arc::new(baton::InMemorySessionStore::new())),
        }).await
    }

    async fn call_tool_with_context(
        &self,
        name: &str,
        arguments: Value,
        context: baton::ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        info!(tool = %name, session = %context.session_id, "Tool call via ZMQ");

        let backend = match self.backends.route_tool(name) {
            Some(b) => b,
            None => {
                return Err(ErrorData::invalid_params(format!(
                    "No backend available for tool: {}",
                    name
                )));
            }
        };

        let payload = match tool_to_payload(name, &arguments) {
            Ok(p) => p,
            Err(e) => {
                return Err(ErrorData::invalid_params(format!(
                    "Invalid tool arguments: {}",
                    e
                )));
            }
        };

        // TODO: Extract traceparent from context if available
        match backend.request(payload).await {
            Ok(Payload::Success { result }) => {
                let text = serde_json::to_string_pretty(&result).unwrap_or_default();
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Ok(Payload::Error { code, message, details }) => {
                let error_text = if let Some(d) = details {
                    format!(
                        "{}: {}\n{}",
                        code,
                        message,
                        serde_json::to_string_pretty(&d).unwrap_or_default()
                    )
                } else {
                    format!("{}: {}", code, message)
                };
                Ok(CallToolResult::error(error_text))
            }
            Ok(other) => Err(ErrorData::internal_error(format!(
                "Unexpected response: {:?}",
                other
            ))),
            Err(e) => Err(ErrorData::internal_error(format!("Backend error: {}", e))),
        }
    }

    fn server_info(&self) -> Implementation {
        Implementation::new("holler", env!("CARGO_PKG_VERSION"))
    }
}

/// Async helper to collect tools from backends.
async fn collect_tools_async(backends: &BackendPool) -> Vec<Tool> {
    let mut all_tools = Vec::new();

    for (name, backend_opt) in [
        ("luanette", &backends.luanette),
        ("hootenanny", &backends.hootenanny),
        ("chaosgarden", &backends.chaosgarden),
    ] {
        if let Some(ref backend) = backend_opt {
            match backend.request(Payload::ListTools).await {
                Ok(Payload::ToolList { tools }) => {
                    debug!("Got {} tools from {}", tools.len(), name);
                    all_tools.extend(tools.into_iter().map(tool_info_to_baton));
                }
                Ok(other) => {
                    error!("{} returned unexpected response to ListTools: {:?}", name, other);
                }
                Err(e) => {
                    error!("Failed to list tools from {}: {}", name, e);
                }
            }
        }
    }

    all_tools
}

/// Convert hooteproto ToolInfo to baton Tool.
fn tool_info_to_baton(info: ToolInfo) -> Tool {
    Tool::new(&info.name, &info.description)
        .with_input_schema(ToolSchema::from_value(info.input_schema))
}

/// Convert an MCP tool call to a hooteproto Payload.
fn tool_to_payload(name: &str, args: &Value) -> anyhow::Result<Payload> {
    match name {
        // === Lua Tools (Luanette) ===
        "lua_eval" => Ok(Payload::LuaEval {
            code: args
                .get("code")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'code' argument"))?
                .to_string(),
            params: args.get("params").cloned(),
        }),

        "lua_describe" => Ok(Payload::LuaDescribe {
            script_hash: args
                .get("script_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'script_hash' argument"))?
                .to_string(),
        }),

        // === Job Tools (Luanette) ===
        "job_execute" => Ok(Payload::JobExecute {
            script_hash: args
                .get("script_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'script_hash' argument"))?
                .to_string(),
            params: args
                .get("params")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
            tags: args.get("tags").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
        }),

        "job_status" => Ok(Payload::JobStatus {
            job_id: args
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'job_id' argument"))?
                .to_string(),
        }),

        "job_list" => Ok(Payload::JobList {
            status: args.get("status").and_then(|v| v.as_str()).map(String::from),
        }),

        "job_cancel" => Ok(Payload::JobCancel {
            job_id: args
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'job_id' argument"))?
                .to_string(),
        }),

        "job_poll" => {
            let job_ids = args
                .get("job_ids")
                .and_then(|v| v.as_array())
                .ok_or_else(|| anyhow::anyhow!("Missing 'job_ids' argument"))?
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            let timeout_ms = args.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(30000);

            let mode = match args.get("mode").and_then(|v| v.as_str()).unwrap_or("any") {
                "all" => hooteproto::PollMode::All,
                _ => hooteproto::PollMode::Any,
            };

            Ok(Payload::JobPoll {
                job_ids,
                timeout_ms,
                mode,
            })
        }

        // === Script Tools (Luanette) ===
        "script_store" => Ok(Payload::ScriptStore {
            content: args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'content' argument"))?
                .to_string(),
            tags: args.get("tags").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "script_search" => Ok(Payload::ScriptSearch {
            tag: args.get("tag").and_then(|v| v.as_str()).map(String::from),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
            vibe: args.get("vibe").and_then(|v| v.as_str()).map(String::from),
        }),

        // === CAS Tools (Hootenanny) ===
        "cas_store" => {
            use base64::{engine::general_purpose::STANDARD, Engine};
            let data_str = args
                .get("data")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'data' argument (base64)"))?;
            let data = STANDARD
                .decode(data_str)
                .map_err(|e| anyhow::anyhow!("Invalid base64 data: {}", e))?;
            Ok(Payload::CasStore {
                data,
                mime_type: args.get("mime_type").and_then(|v| v.as_str()).map(String::from),
            })
        }

        "cas_inspect" => Ok(Payload::CasInspect {
            hash: args
                .get("hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'hash' argument"))?
                .to_string(),
        }),

        "cas_get" => Ok(Payload::CasGet {
            hash: args
                .get("hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'hash' argument"))?
                .to_string(),
        }),

        // === Artifact Tools (Hootenanny) ===
        "artifact_get" => Ok(Payload::ArtifactGet {
            id: args
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'id' argument"))?
                .to_string(),
        }),

        "artifact_list" => Ok(Payload::ArtifactList {
            tag: args.get("tag").and_then(|v| v.as_str()).map(String::from),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "artifact_create" => Ok(Payload::ArtifactCreate {
            cas_hash: args
                .get("cas_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'cas_hash' argument"))?
                .to_string(),
            tags: args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
            metadata: args
                .get("metadata")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
        }),

        // === Graph Tools (Hootenanny) ===
        "graph_query" => Ok(Payload::GraphQuery {
            query: args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?
                .to_string(),
            variables: args
                .get("variables")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
        }),

        "graph_bind" => Ok(Payload::GraphBind {
            identity: args
                .get("identity")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'identity' argument"))?
                .to_string(),
            hints: args
                .get("hints")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        }),

        // === Transport Tools (Chaosgarden) ===
        "transport_play" => Ok(Payload::TransportPlay),
        "transport_stop" => Ok(Payload::TransportStop),
        "transport_status" => Ok(Payload::TransportStatus),

        "transport_seek" => Ok(Payload::TransportSeek {
            position_beats: args
                .get("position_beats")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'position_beats' argument"))?,
        }),

        // === Timeline Tools (Chaosgarden) ===
        "timeline_query" => Ok(Payload::TimelineQuery {
            from_beats: args.get("from_beats").and_then(|v| v.as_f64()),
            to_beats: args.get("to_beats").and_then(|v| v.as_f64()),
        }),

        "timeline_add_marker" => Ok(Payload::TimelineAddMarker {
            position_beats: args
                .get("position_beats")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'position_beats' argument"))?,
            marker_type: args
                .get("marker_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'marker_type' argument"))?
                .to_string(),
            metadata: args
                .get("metadata")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
        }),

        // === Additional Graph Tools (Hootenanny) ===
        "graph_tag" => Ok(Payload::GraphTag {
            identity: args
                .get("identity")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'identity' argument"))?
                .to_string(),
            tag: args
                .get("tag")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'tag' argument"))?
                .to_string(),
        }),

        "graph_connect" => Ok(Payload::GraphConnect {
            source_identity: args
                .get("source_identity")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'source_identity' argument"))?
                .to_string(),
            sink_identity: args
                .get("sink_identity")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'sink_identity' argument"))?
                .to_string(),
            transport: args.get("transport").and_then(|v| v.as_str()).map(String::from),
        }),

        "graph_find" => Ok(Payload::GraphFind {
            hint_pattern: args.get("hint_pattern").and_then(|v| v.as_str()).map(String::from),
            tag: args.get("tag").and_then(|v| v.as_str()).map(String::from),
            limit: args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize),
        }),

        "graph_context" => Ok(Payload::GraphContext {
            tag: args.get("tag").and_then(|v| v.as_str()).map(String::from),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
            vibe: args.get("vibe").and_then(|v| v.as_str()).map(String::from),
            limit: args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize),
        }),

        "add_annotation" => Ok(Payload::AddAnnotation {
            artifact_id: args
                .get("artifact_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'artifact_id' argument"))?
                .to_string(),
            vibe: args
                .get("vibe")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'vibe' argument"))?
                .to_string(),
        }),

        // === CAS Upload (Hootenanny) ===
        "cas_upload_file" => Ok(Payload::CasUploadFile {
            file_path: args
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' argument"))?
                .to_string(),
        }),

        // === Artifact Upload (Hootenanny) ===
        "artifact_upload" => Ok(Payload::ArtifactUpload {
            file_path: args
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' argument"))?
                .to_string(),
            tags: args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
        }),

        // === Orpheus Tools (Hootenanny) ===
        "orpheus_generate" => Ok(Payload::OrpheusGenerate {
            temperature: args.get("temperature").and_then(|v| v.as_f64()),
            top_p: args.get("top_p").and_then(|v| v.as_f64()),
            cfg_coef: args.get("cfg_coef").and_then(|v| v.as_f64()),
            seed: args.get("seed").and_then(|v| v.as_u64()),
        }),

        "orpheus_generate_seeded" => Ok(Payload::OrpheusGenerateSeeded {
            seed_hash: args
                .get("seed_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'seed_hash' argument"))?
                .to_string(),
            temperature: args.get("temperature").and_then(|v| v.as_f64()),
            top_p: args.get("top_p").and_then(|v| v.as_f64()),
            cfg_coef: args.get("cfg_coef").and_then(|v| v.as_f64()),
        }),

        "orpheus_continue" => Ok(Payload::OrpheusContinue {
            midi_hash: args
                .get("midi_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'midi_hash' argument"))?
                .to_string(),
            temperature: args.get("temperature").and_then(|v| v.as_f64()),
            num_tokens: args.get("num_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
        }),

        "orpheus_bridge" => Ok(Payload::OrpheusBridge {
            from_hash: args
                .get("from_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'from_hash' argument"))?
                .to_string(),
            to_hash: args
                .get("to_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'to_hash' argument"))?
                .to_string(),
            temperature: args.get("temperature").and_then(|v| v.as_f64()),
        }),

        "orpheus_loops" => Ok(Payload::OrpheusLoops {
            num_loops: args.get("num_loops").and_then(|v| v.as_u64()).map(|v| v as u32),
            temperature: args.get("temperature").and_then(|v| v.as_f64()),
            density: args.get("density").and_then(|v| v.as_str()).map(String::from),
        }),

        "orpheus_classify" => Ok(Payload::OrpheusClassify {
            midi_hash: args
                .get("midi_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'midi_hash' argument"))?
                .to_string(),
        }),

        // === MIDI/Audio Tools (Hootenanny) ===
        "convert_midi_to_wav" => Ok(Payload::ConvertMidiToWav {
            midi_hash: args
                .get("midi_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'midi_hash' argument"))?
                .to_string(),
            soundfont_hash: args.get("soundfont_hash").and_then(|v| v.as_str()).map(String::from),
            sample_rate: args.get("sample_rate").and_then(|v| v.as_u64()).map(|v| v as u32),
        }),

        "soundfont_inspect" => Ok(Payload::SoundfontInspect {
            soundfont_hash: args
                .get("soundfont_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'soundfont_hash' argument"))?
                .to_string(),
        }),

        "soundfont_preset_inspect" => Ok(Payload::SoundfontPresetInspect {
            soundfont_hash: args
                .get("soundfont_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'soundfont_hash' argument"))?
                .to_string(),
            bank: args
                .get("bank")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'bank' argument"))? as u16,
            program: args
                .get("program")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'program' argument"))? as u16,
        }),

        // === ABC Notation Tools (Hootenanny) ===
        "abc_parse" => Ok(Payload::AbcParse {
            abc: args
                .get("abc")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'abc' argument"))?
                .to_string(),
        }),

        "abc_to_midi" => Ok(Payload::AbcToMidi {
            abc: args
                .get("abc")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'abc' argument"))?
                .to_string(),
        }),

        "abc_validate" => Ok(Payload::AbcValidate {
            abc: args
                .get("abc")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'abc' argument"))?
                .to_string(),
        }),

        "abc_transpose" => Ok(Payload::AbcTranspose {
            abc: args
                .get("abc")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'abc' argument"))?
                .to_string(),
            semitones: args.get("semitones").and_then(|v| v.as_i64()).map(|v| v as i32),
            target_key: args.get("target_key").and_then(|v| v.as_str()).map(String::from),
        }),

        // === Analysis Tools (Hootenanny) ===
        "beatthis_analyze" => Ok(Payload::BeatthisAnalyze {
            audio_hash: args
                .get("audio_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'audio_hash' argument"))?
                .to_string(),
            include_probabilities: args.get("include_probabilities").and_then(|v| v.as_bool()),
        }),

        "clap_analyze" => Ok(Payload::ClapAnalyze {
            audio_hash: args
                .get("audio_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'audio_hash' argument"))?
                .to_string(),
            mode: args.get("mode").and_then(|v| v.as_str()).map(String::from),
            compare_to: args.get("compare_to").and_then(|v| v.as_str()).map(String::from),
        }),

        // === Generation Tools (Hootenanny) ===
        "musicgen_generate" => Ok(Payload::MusicgenGenerate {
            prompt: args
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'prompt' argument"))?
                .to_string(),
            duration_secs: args.get("duration_secs").and_then(|v| v.as_f64()),
            temperature: args.get("temperature").and_then(|v| v.as_f64()),
        }),

        "yue_generate" => Ok(Payload::YueGenerate {
            lyrics: args
                .get("lyrics")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'lyrics' argument"))?
                .to_string(),
            style: args.get("style").and_then(|v| v.as_str()).map(String::from),
        }),

        // === Garden Tools (Hootenanny → Chaosgarden) ===
        "garden_status" => Ok(Payload::GardenStatus),
        "garden_play" => Ok(Payload::GardenPlay),
        "garden_pause" => Ok(Payload::GardenPause),
        "garden_stop" => Ok(Payload::GardenStop),
        "garden_emergency_pause" => Ok(Payload::GardenEmergencyPause),

        "garden_seek" => Ok(Payload::GardenSeek {
            beat: args
                .get("beat")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'beat' argument"))?,
        }),

        "garden_set_tempo" => Ok(Payload::GardenSetTempo {
            bpm: args
                .get("bpm")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'bpm' argument"))?,
        }),

        "garden_query" => Ok(Payload::GardenQuery {
            query: args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'query' argument"))?
                .to_string(),
            variables: args.get("variables").cloned(),
        }),

        // === Misc Tools ===
        "job_sleep" => Ok(Payload::JobSleep {
            duration_ms: args
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'duration_ms' argument"))?,
        }),

        "sample_llm" => Ok(Payload::SampleLlm {
            prompt: args
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'prompt' argument"))?
                .to_string(),
            max_tokens: args.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
        }),

        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}
