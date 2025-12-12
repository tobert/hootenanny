//! Tool name to Payload conversion
//!
//! Converts MCP-style tool calls (name + JSON arguments) to strongly-typed Payload variants.
//! Used by both holler (MCP gateway) and luanette (Lua scripting).

use crate::{GraphHint, Payload, PollMode};
use serde_json::Value;

/// Convert a tool name and JSON arguments to a Payload variant.
///
/// This is the canonical mapping from MCP tool calls to hooteproto messages.
pub fn tool_to_payload(name: &str, args: &Value) -> anyhow::Result<Payload> {
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
                "all" => PollMode::All,
                _ => PollMode::Any,
            };

            Ok(Payload::JobPoll {
                job_ids,
                timeout_ms,
                mode,
            })
        }

        "job_sleep" => Ok(Payload::JobSleep {
            milliseconds: args
                .get("milliseconds")
                .or_else(|| args.get("duration_ms"))
                .and_then(|v| v.as_u64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'milliseconds' argument"))?,
        }),

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
                .or_else(|| args.get("content_base64"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'data' or 'content_base64' argument"))?;
            let data = STANDARD
                .decode(data_str)
                .map_err(|e| anyhow::anyhow!("Invalid base64 data: {}", e))?;
            Ok(Payload::CasStore {
                data,
                mime_type: args.get("mime_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("application/octet-stream")
                    .to_string(),
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

        "cas_upload_file" => Ok(Payload::CasUploadFile {
            file_path: args
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' argument"))?
                .to_string(),
            mime_type: args
                .get("mime_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'mime_type' argument"))?
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

        "artifact_upload" => Ok(Payload::ArtifactUpload {
            file_path: args
                .get("file_path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' argument"))?
                .to_string(),
            mime_type: args
                .get("mime_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'mime_type' argument"))?
                .to_string(),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
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
            limit: args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize),
        }),

        "graph_bind" => {
            let hints = args
                .get("hints")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            let obj = v.as_object()?;
                            Some(GraphHint {
                                kind: obj.get("kind")?.as_str()?.to_string(),
                                value: obj.get("value")?.as_str()?.to_string(),
                                confidence: obj.get("confidence").and_then(|c| c.as_f64()).unwrap_or(1.0),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            Ok(Payload::GraphBind {
                id: args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'id' argument"))?
                    .to_string(),
                name: args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing 'name' argument"))?
                    .to_string(),
                hints,
            })
        }

        "graph_tag" => Ok(Payload::GraphTag {
            identity_id: args
                .get("identity_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'identity_id' argument"))?
                .to_string(),
            namespace: args
                .get("namespace")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'namespace' argument"))?
                .to_string(),
            value: args
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'value' argument"))?
                .to_string(),
        }),

        "graph_connect" => Ok(Payload::GraphConnect {
            from_identity: args
                .get("from_identity")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'from_identity' argument"))?
                .to_string(),
            from_port: args
                .get("from_port")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'from_port' argument"))?
                .to_string(),
            to_identity: args
                .get("to_identity")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'to_identity' argument"))?
                .to_string(),
            to_port: args
                .get("to_port")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'to_port' argument"))?
                .to_string(),
            transport: args.get("transport").and_then(|v| v.as_str()).map(String::from),
        }),

        "graph_find" => Ok(Payload::GraphFind {
            name: args.get("name").and_then(|v| v.as_str()).map(String::from),
            tag_namespace: args.get("tag_namespace").and_then(|v| v.as_str()).map(String::from),
            tag_value: args.get("tag_value").and_then(|v| v.as_str()).map(String::from),
        }),

        "graph_context" => Ok(Payload::GraphContext {
            tag: args.get("tag").and_then(|v| v.as_str()).map(String::from),
            vibe_search: args.get("vibe_search").or_else(|| args.get("vibe")).and_then(|v| v.as_str()).map(String::from),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
            limit: args.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize),
            include_metadata: args.get("include_metadata").and_then(|v| v.as_bool()).unwrap_or(false),
            include_annotations: args.get("include_annotations").and_then(|v| v.as_bool()).unwrap_or(true),
        }),

        "add_annotation" => Ok(Payload::AddAnnotation {
            artifact_id: args
                .get("artifact_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'artifact_id' argument"))?
                .to_string(),
            message: args
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'message' argument"))?
                .to_string(),
            vibe: args.get("vibe").and_then(|v| v.as_str()).map(String::from),
            source: args.get("source").and_then(|v| v.as_str()).map(String::from),
        }),

        // === Orpheus Tools (Hootenanny) ===
        "orpheus_generate" => Ok(Payload::OrpheusGenerate {
            model: args.get("model").and_then(|v| v.as_str()).map(String::from),
            temperature: args.get("temperature").and_then(|v| v.as_f64()).map(|v| v as f32),
            top_p: args.get("top_p").and_then(|v| v.as_f64()).map(|v| v as f32),
            max_tokens: args.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
            num_variations: args.get("num_variations").and_then(|v| v.as_u64()).map(|v| v as u32),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "orpheus_generate_seeded" => Ok(Payload::OrpheusGenerateSeeded {
            seed_hash: args
                .get("seed_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'seed_hash' argument"))?
                .to_string(),
            model: args.get("model").and_then(|v| v.as_str()).map(String::from),
            temperature: args.get("temperature").and_then(|v| v.as_f64()).map(|v| v as f32),
            top_p: args.get("top_p").and_then(|v| v.as_f64()).map(|v| v as f32),
            max_tokens: args.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
            num_variations: args.get("num_variations").and_then(|v| v.as_u64()).map(|v| v as u32),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "orpheus_continue" => Ok(Payload::OrpheusContinue {
            input_hash: args
                .get("input_hash")
                .or_else(|| args.get("midi_hash"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'input_hash' argument"))?
                .to_string(),
            model: args.get("model").and_then(|v| v.as_str()).map(String::from),
            temperature: args.get("temperature").and_then(|v| v.as_f64()).map(|v| v as f32),
            top_p: args.get("top_p").and_then(|v| v.as_f64()).map(|v| v as f32),
            max_tokens: args.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
            num_variations: args.get("num_variations").and_then(|v| v.as_u64()).map(|v| v as u32),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "orpheus_bridge" => Ok(Payload::OrpheusBridge {
            section_a_hash: args
                .get("section_a_hash")
                .or_else(|| args.get("from_hash"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'section_a_hash' argument"))?
                .to_string(),
            section_b_hash: args.get("section_b_hash").or_else(|| args.get("to_hash")).and_then(|v| v.as_str()).map(String::from),
            model: args.get("model").and_then(|v| v.as_str()).map(String::from),
            temperature: args.get("temperature").and_then(|v| v.as_f64()).map(|v| v as f32),
            top_p: args.get("top_p").and_then(|v| v.as_f64()).map(|v| v as f32),
            max_tokens: args.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "orpheus_loops" => Ok(Payload::OrpheusLoops {
            temperature: args.get("temperature").and_then(|v| v.as_f64()).map(|v| v as f32),
            top_p: args.get("top_p").and_then(|v| v.as_f64()).map(|v| v as f32),
            max_tokens: args.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
            num_variations: args.get("num_variations").and_then(|v| v.as_u64()).map(|v| v as u32),
            seed_hash: args.get("seed_hash").and_then(|v| v.as_str()).map(String::from),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
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
            input_hash: args
                .get("input_hash")
                .or_else(|| args.get("midi_hash"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'input_hash' argument"))?
                .to_string(),
            soundfont_hash: args
                .get("soundfont_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'soundfont_hash' argument"))?
                .to_string(),
            sample_rate: args.get("sample_rate").and_then(|v| v.as_u64()).map(|v| v as u32),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "soundfont_inspect" => Ok(Payload::SoundfontInspect {
            soundfont_hash: args
                .get("soundfont_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'soundfont_hash' argument"))?
                .to_string(),
            include_drum_map: args.get("include_drum_map").and_then(|v| v.as_bool()).unwrap_or(false),
        }),

        "soundfont_preset_inspect" => Ok(Payload::SoundfontPresetInspect {
            soundfont_hash: args
                .get("soundfont_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'soundfont_hash' argument"))?
                .to_string(),
            bank: args
                .get("bank")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'bank' argument"))? as i32,
            program: args
                .get("program")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'program' argument"))? as i32,
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
            tempo_override: args.get("tempo_override").and_then(|v| v.as_u64()).map(|v| v as u16),
            transpose: args.get("transpose").and_then(|v| v.as_i64()).map(|v| v as i8),
            velocity: args.get("velocity").and_then(|v| v.as_u64()).map(|v| v as u8),
            channel: args.get("channel").and_then(|v| v.as_u64()).map(|v| v as u8),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
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
            semitones: args.get("semitones").and_then(|v| v.as_i64()).map(|v| v as i8),
            target_key: args.get("target_key").and_then(|v| v.as_str()).map(String::from),
        }),

        // === Analysis Tools (Hootenanny) ===
        "beatthis_analyze" => Ok(Payload::BeatthisAnalyze {
            audio_path: args.get("audio_path").and_then(|v| v.as_str()).map(String::from),
            audio_hash: args.get("audio_hash").and_then(|v| v.as_str()).map(String::from),
            include_frames: args.get("include_frames").and_then(|v| v.as_bool()).unwrap_or(false),
        }),

        "clap_analyze" => Ok(Payload::ClapAnalyze {
            audio_hash: args
                .get("audio_hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'audio_hash' argument"))?
                .to_string(),
            tasks: extract_string_array(args, "tasks"),
            audio_b_hash: args.get("audio_b_hash").and_then(|v| v.as_str()).map(String::from),
            text_candidates: extract_string_array(args, "text_candidates"),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        // === Generation Tools (Hootenanny) ===
        "musicgen_generate" => Ok(Payload::MusicgenGenerate {
            prompt: args.get("prompt").and_then(|v| v.as_str()).map(String::from),
            duration: args.get("duration").and_then(|v| v.as_f64()).map(|v| v as f32),
            temperature: args.get("temperature").and_then(|v| v.as_f64()).map(|v| v as f32),
            top_k: args.get("top_k").and_then(|v| v.as_u64()).map(|v| v as u32),
            top_p: args.get("top_p").and_then(|v| v.as_f64()).map(|v| v as f32),
            guidance_scale: args.get("guidance_scale").and_then(|v| v.as_f64()).map(|v| v as f32),
            do_sample: args.get("do_sample").and_then(|v| v.as_bool()),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
        }),

        "yue_generate" => Ok(Payload::YueGenerate {
            lyrics: args
                .get("lyrics")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'lyrics' argument"))?
                .to_string(),
            genre: args.get("genre").or_else(|| args.get("style")).and_then(|v| v.as_str()).map(String::from),
            max_new_tokens: args.get("max_new_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
            run_n_segments: args.get("run_n_segments").and_then(|v| v.as_u64()).map(|v| v as u32),
            seed: args.get("seed").and_then(|v| v.as_u64()),
            variation_set_id: args.get("variation_set_id").and_then(|v| v.as_str()).map(String::from),
            parent_id: args.get("parent_id").and_then(|v| v.as_str()).map(String::from),
            tags: extract_string_array(args, "tags"),
            creator: args.get("creator").and_then(|v| v.as_str()).map(String::from),
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

        // === LLM Tools ===
        "sample_llm" => Ok(Payload::SampleLlm {
            prompt: args
                .get("prompt")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'prompt' argument"))?
                .to_string(),
            max_tokens: args.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32),
            temperature: args.get("temperature").and_then(|v| v.as_f64()),
            system_prompt: args.get("system_prompt").and_then(|v| v.as_str()).map(String::from),
        }),

        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}

/// Helper to extract string arrays from JSON arguments
fn extract_string_array(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
