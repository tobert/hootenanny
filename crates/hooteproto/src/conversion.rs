//! Tool name to Payload conversion
//!
//! Converts tool calls (name + JSON arguments) to strongly-typed Payload variants.
//! Used by both holler (gateway) and luanette (Lua scripting).

use crate::{GraphHint, Payload, PollMode};
use serde_json::Value;

// Cap'n Proto imports for reading requests
use crate::{envelope_capnp, tools_capnp};

/// Convert a tool name and JSON arguments to a Payload variant.
///
/// This is the canonical mapping from tool calls to hooteproto messages.
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

        // === Config Tools (Hootenanny) ===
        "config_get" => Ok(Payload::ConfigGet {
            section: args.get("section").and_then(|v| v.as_str()).map(String::from),
            key: args.get("key").and_then(|v| v.as_str()).map(String::from),
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

        "garden_create_region" => Ok(Payload::GardenCreateRegion {
            position: args
                .get("position")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'position' argument"))?,
            duration: args
                .get("duration")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'duration' argument"))?,
            behavior_type: args
                .get("behavior_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'behavior_type' argument"))?
                .to_string(),
            content_id: args
                .get("content_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'content_id' argument"))?
                .to_string(),
        }),

        "garden_delete_region" => Ok(Payload::GardenDeleteRegion {
            region_id: args
                .get("region_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'region_id' argument"))?
                .to_string(),
        }),

        "garden_move_region" => Ok(Payload::GardenMoveRegion {
            region_id: args
                .get("region_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing 'region_id' argument"))?
                .to_string(),
            new_position: args
                .get("new_position")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("Missing 'new_position' argument"))?,
        }),

        "garden_get_regions" => Ok(Payload::GardenGetRegions {
            start: args.get("start").and_then(|v| v.as_f64()),
            end: args.get("end").and_then(|v| v.as_f64()),
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

// =============================================================================
// Typed Protocol Conversions (Protocol v2)
// =============================================================================

use crate::envelope::ResponseEnvelope;
use crate::request::*;
use crate::ToolError;

/// Convert a Payload to a ToolRequest for typed dispatch.
///
/// Returns Ok(Some(request)) for supported tools, Ok(None) for tools that
/// should use the legacy JSON path, and Err for invalid requests.
pub fn payload_to_request(payload: &Payload) -> Result<Option<ToolRequest>, ToolError> {
    match payload {
        // === ABC Notation (Sync) ===
        Payload::AbcParse { abc } => Ok(Some(ToolRequest::AbcParse(AbcParseRequest {
            abc: abc.clone(),
        }))),
        Payload::AbcValidate { abc } => Ok(Some(ToolRequest::AbcValidate(AbcValidateRequest {
            abc: abc.clone(),
        }))),
        Payload::AbcTranspose {
            abc,
            semitones,
            target_key,
        } => Ok(Some(ToolRequest::AbcTranspose(AbcTransposeRequest {
            abc: abc.clone(),
            semitones: *semitones,
            target_key: target_key.clone(),
        }))),

        // === SoundFont (Sync) ===
        Payload::SoundfontInspect {
            soundfont_hash,
            include_drum_map,
        } => Ok(Some(ToolRequest::SoundfontInspect(SoundfontInspectRequest {
            soundfont_hash: soundfont_hash.clone(),
            include_drum_map: *include_drum_map,
        }))),
        Payload::SoundfontPresetInspect {
            soundfont_hash,
            bank,
            program,
        } => Ok(Some(ToolRequest::SoundfontPresetInspect(
            SoundfontPresetInspectRequest {
                soundfont_hash: soundfont_hash.clone(),
                bank: *bank as u16,
                program: *program as u16,
            },
        ))),

        // === Garden (Sync status, FireAndForget controls) ===
        Payload::GardenStatus => Ok(Some(ToolRequest::GardenStatus)),
        Payload::GardenPlay => Ok(Some(ToolRequest::GardenPlay)),
        Payload::GardenPause => Ok(Some(ToolRequest::GardenPause)),
        Payload::GardenStop => Ok(Some(ToolRequest::GardenStop)),
        Payload::GardenSeek { beat } => Ok(Some(ToolRequest::GardenSeek(GardenSeekRequest {
            beat: *beat,
        }))),
        Payload::GardenSetTempo { bpm } => {
            Ok(Some(ToolRequest::GardenSetTempo(GardenSetTempoRequest {
                bpm: *bpm,
            })))
        }
        Payload::GardenGetRegions { start, end } => Ok(Some(ToolRequest::GardenGetRegions(
            GardenGetRegionsRequest {
                start: *start,
                end: *end,
            },
        ))),
        Payload::GardenCreateRegion {
            position,
            duration,
            behavior_type,
            content_id,
        } => Ok(Some(ToolRequest::GardenCreateRegion(
            GardenCreateRegionRequest {
                position: *position,
                duration: *duration,
                behavior_type: behavior_type.clone(),
                content_id: content_id.clone(),
            },
        ))),
        Payload::GardenDeleteRegion { region_id } => Ok(Some(ToolRequest::GardenDeleteRegion(
            GardenDeleteRegionRequest {
                region_id: region_id.clone(),
            },
        ))),
        Payload::GardenMoveRegion {
            region_id,
            new_position,
        } => Ok(Some(ToolRequest::GardenMoveRegion(GardenMoveRegionRequest {
            region_id: region_id.clone(),
            new_position: *new_position,
        }))),
        Payload::GardenEmergencyPause => Ok(Some(ToolRequest::GardenEmergencyPause)),

        // === Jobs (Sync) ===
        Payload::JobStatus { job_id } => Ok(Some(ToolRequest::JobStatus(JobStatusRequest {
            job_id: job_id.clone(),
        }))),
        Payload::JobList { status } => Ok(Some(ToolRequest::JobList(JobListRequest {
            status: status.clone(),
        }))),

        // === Config (Sync) ===
        Payload::ConfigGet { section, key } => Ok(Some(ToolRequest::ConfigGet(ConfigGetRequest {
            section: section.clone(),
            key: key.clone(),
        }))),

        // === Admin (Sync) ===
        Payload::Ping => Ok(Some(ToolRequest::Ping)),
        Payload::ListTools => Ok(Some(ToolRequest::ListTools)),

        // === Tools not yet converted - use legacy path ===
        _ => Ok(None),
    }
}

/// Convert a ResponseEnvelope back to Payload for ZMQ transport.
pub fn envelope_to_payload(envelope: ResponseEnvelope) -> Payload {
    match envelope {
        ResponseEnvelope::Success { response } => {
            // Convert typed response to JSON for legacy Payload::Success
            let result = response.to_json();
            Payload::Success { result }
        }
        ResponseEnvelope::JobStarted { job_id, tool, .. } => Payload::Success {
            result: serde_json::json!({
                "job_id": job_id,
                "tool": tool,
                "status": "started",
            }),
        },
        ResponseEnvelope::Ack { message } => Payload::Success {
            result: serde_json::json!({
                "status": "ok",
                "message": message,
            }),
        },
        ResponseEnvelope::Error(err) => Payload::Error {
            code: err.code().to_string(),
            message: err.message().to_string(),
            details: None,
        },
    }
}

/// Convert a Cap'n Proto Envelope reader to Payload
///
/// This enables the server to read Cap'n Proto requests from Python/Lua clients
/// and convert them to the internal Payload representation for dispatch.
pub fn capnp_envelope_to_payload(
    reader: envelope_capnp::envelope::Reader,
) -> capnp::Result<Payload> {
    let payload_reader = reader.get_payload()?;

    // Check which payload variant is set
    match payload_reader.which()? {
        // === Worker Management ===
        envelope_capnp::payload::Ping(()) => Ok(Payload::Ping),

        envelope_capnp::payload::Shutdown(shutdown) => {
            let reason = shutdown?.get_reason()?.to_str()?.to_string();
            Ok(Payload::Shutdown { reason })
        }

        // === Tool Requests ===
        envelope_capnp::payload::ToolRequest(tool_req) => {
            let tool_req = tool_req?;
            capnp_tool_request_to_payload(tool_req)
        }

        // === Garden/Timeline ===
        envelope_capnp::payload::GardenStatus(()) => Ok(Payload::GardenStatus),
        envelope_capnp::payload::GardenPlay(()) => Ok(Payload::GardenPlay),
        envelope_capnp::payload::GardenPause(()) => Ok(Payload::GardenPause),
        envelope_capnp::payload::GardenStop(()) => Ok(Payload::GardenStop),

        envelope_capnp::payload::GardenSeek(seek) => {
            let seek = seek?;
            Ok(Payload::GardenSeek {
                beat: seek.get_beat(),
            })
        }

        envelope_capnp::payload::GardenSetTempo(tempo) => {
            let tempo = tempo?;
            Ok(Payload::GardenSetTempo {
                bpm: tempo.get_bpm(),
            })
        }

        envelope_capnp::payload::GardenQuery(query) => {
            let query = query?;
            let query_str = query.get_query()?.to_str()?.to_string();
            let variables_str = query.get_variables()?.to_str()?;
            let variables = if variables_str.is_empty() {
                None
            } else {
                serde_json::from_str(variables_str).ok()
            };

            Ok(Payload::GardenQuery {
                query: query_str,
                variables,
            })
        }

        envelope_capnp::payload::GardenEmergencyPause(()) => Ok(Payload::GardenEmergencyPause),

        envelope_capnp::payload::GardenCreateRegion(region) => {
            let region = region?;
            let behavior_type = region.get_behavior_type()?.to_str()?.to_string();
            let content_id = region.get_content_id()?.to_str()?.to_string();

            Ok(Payload::GardenCreateRegion {
                position: region.get_position(),
                duration: region.get_duration(),
                behavior_type,
                content_id,
            })
        }

        envelope_capnp::payload::GardenDeleteRegion(region) => {
            let region = region?;
            Ok(Payload::GardenDeleteRegion {
                region_id: region.get_region_id()?.to_str()?.to_string(),
            })
        }

        envelope_capnp::payload::GardenMoveRegion(region) => {
            let region = region?;
            Ok(Payload::GardenMoveRegion {
                region_id: region.get_region_id()?.to_str()?.to_string(),
                new_position: region.get_new_position(),
            })
        }

        envelope_capnp::payload::GardenGetRegions(regions) => {
            let regions = regions?;
            let start = regions.get_start();
            let end = regions.get_end();

            Ok(Payload::GardenGetRegions {
                start: if start == 0.0 { None } else { Some(start) },
                end: if end == 0.0 { None } else { Some(end) },
            })
        }

        // === Transport ===
        envelope_capnp::payload::TransportPlay(()) => Ok(Payload::TransportPlay),
        envelope_capnp::payload::TransportStop(()) => Ok(Payload::TransportStop),
        envelope_capnp::payload::TransportStatus(()) => Ok(Payload::TransportStatus),

        envelope_capnp::payload::TransportSeek(seek) => {
            let seek = seek?;
            Ok(Payload::TransportSeek {
                position_beats: seek.get_position_beats(),
            })
        }

        // === Timeline ===
        envelope_capnp::payload::TimelineQuery(query) => {
            let query = query?;
            Ok(Payload::TimelineQuery {
                from_beats: Some(query.get_from_beats()),
                to_beats: Some(query.get_to_beats()),
            })
        }

        envelope_capnp::payload::TimelineAddMarker(marker) => {
            let marker = marker?;
            let metadata_str = marker.get_metadata()?.to_str()?;
            let metadata = serde_json::from_str(metadata_str).unwrap_or_default();

            Ok(Payload::TimelineAddMarker {
                position_beats: marker.get_position_beats(),
                marker_type: marker.get_marker_type()?.to_str()?.to_string(),
                metadata,
            })
        }

        // === Responses (shouldn't receive these, but handle gracefully) ===
        envelope_capnp::payload::Success(success) => {
            let success = success?;
            let result_str = success.get_result()?.to_str()?;
            let result = serde_json::from_str(result_str).unwrap_or_default();
            Ok(Payload::Success { result })
        }

        envelope_capnp::payload::Error(error) => {
            let error = error?;
            Ok(Payload::Error {
                code: error.get_code()?.to_str()?.to_string(),
                message: error.get_message()?.to_str()?.to_string(),
                details: None,
            })
        }

        envelope_capnp::payload::ToolList(tool_list) => {
            let tool_list = tool_list?;
            let tools_reader = tool_list.get_tools()?;
            let mut tools = Vec::new();

            for i in 0..tools_reader.len() {
                let tool = tools_reader.get(i);
                tools.push(crate::ToolInfo {
                    name: tool.get_name()?.to_str()?.to_string(),
                    description: tool.get_description()?.to_str()?.to_string(),
                    input_schema: serde_json::from_str(tool.get_input_schema()?.to_str()?).unwrap_or_default(),
                });
            }

            Ok(Payload::ToolList { tools })
        }

        // === Stream Capture ===
        envelope_capnp::payload::StreamStart(stream) => {
            let stream = stream?;
            let def = stream.get_definition()?;
            let format = def.get_format()?;

            let stream_format = match format.which()? {
                crate::streams_capnp::stream_format::Audio(audio) => {
                    let audio = audio?;
                    let sample_format_enum = audio.get_sample_format()?;
                    let sample_format = match sample_format_enum {
                        crate::streams_capnp::SampleFormat::F32 => crate::SampleFormat::F32,
                        crate::streams_capnp::SampleFormat::I16 => crate::SampleFormat::I16,
                        crate::streams_capnp::SampleFormat::I24 => crate::SampleFormat::I24,
                    };

                    crate::StreamFormat::Audio {
                        sample_rate: audio.get_sample_rate(),
                        channels: audio.get_channels(),
                        sample_format,
                    }
                }
                crate::streams_capnp::stream_format::Midi(()) => {
                    crate::StreamFormat::Midi
                }
            };

            Ok(Payload::StreamStart {
                uri: stream.get_uri()?.to_str()?.to_string(),
                definition: crate::StreamDefinition {
                    uri: def.get_uri()?.to_str()?.to_string(),
                    device_identity: def.get_device_identity()?.to_str()?.to_string(),
                    format: stream_format,
                    chunk_size_bytes: def.get_chunk_size_bytes(),
                },
                chunk_path: stream.get_chunk_path()?.to_str()?.to_string(),
            })
        }

        envelope_capnp::payload::StreamSwitchChunk(stream) => {
            let stream = stream?;
            Ok(Payload::StreamSwitchChunk {
                uri: stream.get_uri()?.to_str()?.to_string(),
                new_chunk_path: stream.get_new_chunk_path()?.to_str()?.to_string(),
            })
        }

        envelope_capnp::payload::StreamStop(stream) => {
            let stream = stream?;
            Ok(Payload::StreamStop {
                uri: stream.get_uri()?.to_str()?.to_string(),
            })
        }

        // Variants not yet implemented
        envelope_capnp::payload::Register(_) |
        envelope_capnp::payload::Pong(_) |
        envelope_capnp::payload::TimelineEvent(_) => {
            Err(capnp::Error::failed("Payload variant not yet implemented for capnp conversion".to_string()))
        }
    }
}

/// Convert a Cap'n Proto ToolRequest to Payload
fn capnp_tool_request_to_payload(
    reader: tools_capnp::tool_request::Reader,
) -> capnp::Result<Payload> {
    match reader.which()? {
        // === CAS Tools ===
        tools_capnp::tool_request::CasStore(cas) => {
            let cas = cas?;
            Ok(Payload::CasStore {
                data: cas.get_data()?.to_vec(),
                mime_type: cas.get_mime_type()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::CasInspect(cas) => {
            let cas = cas?;
            Ok(Payload::CasInspect {
                hash: cas.get_hash()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::CasGet(cas) => {
            let cas = cas?;
            Ok(Payload::CasGet {
                hash: cas.get_hash()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::CasUploadFile(cas) => {
            let cas = cas?;
            Ok(Payload::CasUploadFile {
                file_path: cas.get_file_path()?.to_str()?.to_string(),
                mime_type: cas.get_mime_type()?.to_str()?.to_string(),
            })
        }

        // === Orpheus Tools ===
        tools_capnp::tool_request::OrpheusGenerate(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;

            Ok(Payload::OrpheusGenerate {
                model: Some(orpheus.get_model()?.to_str()?.to_string()),
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                num_variations: Some(orpheus.get_num_variations()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusGenerateSeeded(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;

            Ok(Payload::OrpheusGenerateSeeded {
                seed_hash: orpheus.get_seed_hash()?.to_str()?.to_string(),
                model: Some(orpheus.get_model()?.to_str()?.to_string()),
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                num_variations: Some(orpheus.get_num_variations()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusContinue(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;

            Ok(Payload::OrpheusContinue {
                input_hash: orpheus.get_input_hash()?.to_str()?.to_string(),
                model: Some(orpheus.get_model()?.to_str()?.to_string()),
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                num_variations: Some(orpheus.get_num_variations()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusBridge(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;
            let section_b = orpheus.get_section_b_hash()?.to_str()?;

            Ok(Payload::OrpheusBridge {
                section_a_hash: orpheus.get_section_a_hash()?.to_str()?.to_string(),
                section_b_hash: if section_b.is_empty() { None } else { Some(section_b.to_string()) },
                model: Some(orpheus.get_model()?.to_str()?.to_string()),
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusLoops(orpheus) => {
            let orpheus = orpheus?;
            let metadata = orpheus.get_metadata()?;
            let seed_hash = orpheus.get_seed_hash()?.to_str()?;

            Ok(Payload::OrpheusLoops {
                temperature: Some(orpheus.get_temperature()),
                top_p: Some(orpheus.get_top_p()),
                max_tokens: Some(orpheus.get_max_tokens()),
                num_variations: Some(orpheus.get_num_variations()),
                seed_hash: if seed_hash.is_empty() { None } else { Some(seed_hash.to_string()) },
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::OrpheusClassify(orpheus) => {
            let orpheus = orpheus?;
            Ok(Payload::OrpheusClassify {
                midi_hash: orpheus.get_midi_hash()?.to_str()?.to_string(),
            })
        }

        // === ABC Notation Tools ===
        tools_capnp::tool_request::AbcParse(abc) => {
            let abc = abc?;
            Ok(Payload::AbcParse {
                abc: abc.get_abc()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::AbcToMidi(abc) => {
            let abc = abc?;
            let metadata = abc.get_metadata()?;

            Ok(Payload::AbcToMidi {
                abc: abc.get_abc()?.to_str()?.to_string(),
                tempo_override: Some(abc.get_tempo_override()),
                transpose: Some(abc.get_transpose()),
                velocity: Some(abc.get_velocity()),
                channel: Some(abc.get_channel()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::AbcValidate(abc) => {
            let abc = abc?;
            Ok(Payload::AbcValidate {
                abc: abc.get_abc()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::AbcTranspose(abc) => {
            let abc = abc?;
            let target_key = abc.get_target_key()?.to_str()?;

            Ok(Payload::AbcTranspose {
                abc: abc.get_abc()?.to_str()?.to_string(),
                semitones: Some(abc.get_semitones()),
                target_key: if target_key.is_empty() { None } else { Some(target_key.to_string()) },
            })
        }

        // === MIDI/Audio Tools ===
        tools_capnp::tool_request::ConvertMidiToWav(convert) => {
            let convert = convert?;
            let metadata = convert.get_metadata()?;

            Ok(Payload::ConvertMidiToWav {
                input_hash: convert.get_input_hash()?.to_str()?.to_string(),
                soundfont_hash: convert.get_soundfont_hash()?.to_str()?.to_string(),
                sample_rate: Some(convert.get_sample_rate()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::SoundfontInspect(sf) => {
            let sf = sf?;
            Ok(Payload::SoundfontInspect {
                soundfont_hash: sf.get_soundfont_hash()?.to_str()?.to_string(),
                include_drum_map: sf.get_include_drum_map(),
            })
        }

        tools_capnp::tool_request::SoundfontPresetInspect(sf) => {
            let sf = sf?;
            Ok(Payload::SoundfontPresetInspect {
                soundfont_hash: sf.get_soundfont_hash()?.to_str()?.to_string(),
                bank: sf.get_bank(),
                program: sf.get_program(),
            })
        }

        // === Analysis Tools ===
        tools_capnp::tool_request::BeatthisAnalyze(beat) => {
            let beat = beat?;
            let audio_hash = beat.get_audio_hash()?.to_str()?;
            let audio_path = beat.get_audio_path()?.to_str()?;

            Ok(Payload::BeatthisAnalyze {
                audio_hash: if audio_hash.is_empty() { None } else { Some(audio_hash.to_string()) },
                audio_path: if audio_path.is_empty() { None } else { Some(audio_path.to_string()) },
                include_frames: beat.get_include_frames(),
            })
        }

        tools_capnp::tool_request::ClapAnalyze(clap) => {
            let clap = clap?;
            let audio_b_hash = clap.get_audio_b_hash()?.to_str()?;
            let tasks_reader = clap.get_tasks()?;
            let text_reader = clap.get_text_candidates()?;

            let parent_id = clap.get_parent_id()?.to_str()?;
            let creator = clap.get_creator()?.to_str()?;

            Ok(Payload::ClapAnalyze {
                audio_hash: clap.get_audio_hash()?.to_str()?.to_string(),
                tasks: capnp_string_list(tasks_reader),
                audio_b_hash: if audio_b_hash.is_empty() { None } else { Some(audio_b_hash.to_string()) },
                text_candidates: capnp_string_list(text_reader),
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
                parent_id: if parent_id.is_empty() { None } else { Some(parent_id.to_string()) },
            })
        }

        // === Generation Tools ===
        tools_capnp::tool_request::MusicgenGenerate(mg) => {
            let mg = mg?;
            let metadata = mg.get_metadata()?;
            let prompt = mg.get_prompt()?.to_str()?;

            Ok(Payload::MusicgenGenerate {
                prompt: if prompt.is_empty() { None } else { Some(prompt.to_string()) },
                duration: Some(mg.get_duration()),
                temperature: Some(mg.get_temperature()),
                top_k: Some(mg.get_top_k()),
                top_p: Some(mg.get_top_p()),
                guidance_scale: Some(mg.get_guidance_scale()),
                do_sample: Some(mg.get_do_sample()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::YueGenerate(yue) => {
            let yue = yue?;
            let metadata = yue.get_metadata()?;
            let genre = yue.get_genre()?.to_str()?;

            Ok(Payload::YueGenerate {
                lyrics: yue.get_lyrics()?.to_str()?.to_string(),
                genre: if genre.is_empty() { None } else { Some(genre.to_string()) },
                max_new_tokens: Some(yue.get_max_new_tokens()),
                run_n_segments: Some(yue.get_run_n_segments()),
                seed: Some(yue.get_seed()),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        // === Artifact Tools ===
        tools_capnp::tool_request::ArtifactUpload(artifact) => {
            let artifact = artifact?;
            let metadata = artifact.get_metadata()?;

            Ok(Payload::ArtifactUpload {
                file_path: artifact.get_file_path()?.to_str()?.to_string(),
                mime_type: artifact.get_mime_type()?.to_str()?.to_string(),
                tags: capnp_string_list(metadata.get_tags()?),
                creator: Some(metadata.get_creator()?.to_str()?.to_string()),
                parent_id: capnp_optional_string(metadata.get_parent_id()?),
                variation_set_id: capnp_optional_string(metadata.get_variation_set_id()?),
            })
        }

        tools_capnp::tool_request::ArtifactGet(artifact) => {
            let artifact = artifact?;
            Ok(Payload::ArtifactGet {
                id: artifact.get_id()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::ArtifactList(artifact) => {
            let artifact = artifact?;
            let tag = artifact.get_tag()?.to_str()?;
            let creator = artifact.get_creator()?.to_str()?;

            Ok(Payload::ArtifactList {
                tag: if tag.is_empty() { None } else { Some(tag.to_string()) },
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
            })
        }

        tools_capnp::tool_request::ArtifactCreate(artifact) => {
            let artifact = artifact?;
            let metadata_str = artifact.get_metadata()?.to_str()?;
            let metadata = serde_json::from_str(metadata_str).unwrap_or_default();
            let creator = artifact.get_creator()?.to_str()?;

            Ok(Payload::ArtifactCreate {
                cas_hash: artifact.get_cas_hash()?.to_str()?.to_string(),
                tags: capnp_string_list(artifact.get_tags()?),
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
                metadata,
            })
        }

        // === Graph Tools ===
        tools_capnp::tool_request::GraphQuery(query) => {
            let query = query?;
            let variables_str = query.get_variables()?.to_str()?;
            let variables = serde_json::from_str(variables_str).unwrap_or_default();

            Ok(Payload::GraphQuery {
                query: query.get_query()?.to_str()?.to_string(),
                limit: Some(query.get_limit() as usize),
                variables,
            })
        }

        tools_capnp::tool_request::GraphBind(bind) => {
            let bind = bind?;
            let hints_reader = bind.get_hints()?;
            let mut hints = Vec::new();

            for i in 0..hints_reader.len() {
                let hint = hints_reader.get(i);
                hints.push(crate::GraphHint {
                    kind: hint.get_kind()?.to_str()?.to_string(),
                    value: hint.get_value()?.to_str()?.to_string(),
                    confidence: hint.get_confidence(),
                });
            }

            Ok(Payload::GraphBind {
                id: bind.get_id()?.to_str()?.to_string(),
                name: bind.get_name()?.to_str()?.to_string(),
                hints,
            })
        }

        tools_capnp::tool_request::GraphTag(tag) => {
            let tag = tag?;
            Ok(Payload::GraphTag {
                identity_id: tag.get_identity_id()?.to_str()?.to_string(),
                namespace: tag.get_namespace()?.to_str()?.to_string(),
                value: tag.get_value()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::GraphConnect(connect) => {
            let connect = connect?;
            let transport = connect.get_transport()?.to_str()?;

            Ok(Payload::GraphConnect {
                from_identity: connect.get_from_identity()?.to_str()?.to_string(),
                from_port: connect.get_from_port()?.to_str()?.to_string(),
                to_identity: connect.get_to_identity()?.to_str()?.to_string(),
                to_port: connect.get_to_port()?.to_str()?.to_string(),
                transport: if transport.is_empty() { None } else { Some(transport.to_string()) },
            })
        }

        tools_capnp::tool_request::GraphFind(find) => {
            let find = find?;
            let name = find.get_name()?.to_str()?;
            let tag_namespace = find.get_tag_namespace()?.to_str()?;
            let tag_value = find.get_tag_value()?.to_str()?;

            Ok(Payload::GraphFind {
                name: if name.is_empty() { None } else { Some(name.to_string()) },
                tag_namespace: if tag_namespace.is_empty() { None } else { Some(tag_namespace.to_string()) },
                tag_value: if tag_value.is_empty() { None } else { Some(tag_value.to_string()) },
            })
        }

        tools_capnp::tool_request::GraphContext(context) => {
            let context = context?;
            let tag = context.get_tag()?.to_str()?;
            let creator = context.get_creator()?.to_str()?;
            let vibe_search = context.get_vibe_search()?.to_str()?;

            Ok(Payload::GraphContext {
                limit: Some(context.get_limit() as usize),
                tag: if tag.is_empty() { None } else { Some(tag.to_string()) },
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
                vibe_search: if vibe_search.is_empty() { None } else { Some(vibe_search.to_string()) },
                include_metadata: context.get_include_metadata(),
                include_annotations: context.get_include_annotations(),
            })
        }

        tools_capnp::tool_request::AddAnnotation(annotation) => {
            let annotation = annotation?;
            let source = annotation.get_source()?.to_str()?;
            let vibe = annotation.get_vibe()?.to_str()?;

            Ok(Payload::AddAnnotation {
                artifact_id: annotation.get_artifact_id()?.to_str()?.to_string(),
                message: annotation.get_message()?.to_str()?.to_string(),
                source: if source.is_empty() { None } else { Some(source.to_string()) },
                vibe: if vibe.is_empty() { None } else { Some(vibe.to_string()) },
            })
        }

        // === Config Tools ===
        tools_capnp::tool_request::ConfigGet(config) => {
            let config = config?;
            let section = config.get_section()?.to_str()?;
            let key = config.get_key()?.to_str()?;

            Ok(Payload::ConfigGet {
                section: if section.is_empty() { None } else { Some(section.to_string()) },
                key: if key.is_empty() { None } else { Some(key.to_string()) },
            })
        }

        // === Lua Tools ===
        tools_capnp::tool_request::LuaEval(lua) => {
            let lua = lua?;
            let params_str = lua.get_params()?.to_str()?;
            let params = if params_str.is_empty() {
                None
            } else {
                serde_json::from_str(params_str).ok()
            };

            Ok(Payload::LuaEval {
                code: lua.get_code()?.to_str()?.to_string(),
                params,
            })
        }

        tools_capnp::tool_request::LuaDescribe(lua) => {
            let lua = lua?;
            Ok(Payload::LuaDescribe {
                script_hash: lua.get_script_hash()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::ScriptStore(script) => {
            let script = script?;
            let tags_reader = script.get_tags()?;
            let creator = script.get_creator()?.to_str()?;

            Ok(Payload::ScriptStore {
                content: script.get_content()?.to_str()?.to_string(),
                tags: Some(capnp_string_list(tags_reader)),
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
            })
        }

        tools_capnp::tool_request::ScriptSearch(script) => {
            let script = script?;
            let tag = script.get_tag()?.to_str()?;
            let creator = script.get_creator()?.to_str()?;
            let vibe = script.get_vibe()?.to_str()?;

            Ok(Payload::ScriptSearch {
                tag: if tag.is_empty() { None } else { Some(tag.to_string()) },
                creator: if creator.is_empty() { None } else { Some(creator.to_string()) },
                vibe: if vibe.is_empty() { None } else { Some(vibe.to_string()) },
            })
        }

        // === Job Tools ===
        tools_capnp::tool_request::JobExecute(job) => {
            let job = job?;
            let params_str = job.get_params()?.to_str()?;
            let params = serde_json::from_str(params_str).unwrap_or_default();
            let tags_reader = job.get_tags()?;

            Ok(Payload::JobExecute {
                script_hash: job.get_script_hash()?.to_str()?.to_string(),
                params,
                tags: Some(capnp_string_list(tags_reader)),
            })
        }

        tools_capnp::tool_request::JobStatus(job) => {
            let job = job?;
            Ok(Payload::JobStatus {
                job_id: job.get_job_id()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::JobPoll(job) => {
            let job = job?;
            let job_ids_reader = job.get_job_ids()?;
            let mode_enum = job.get_mode()?;
            let mode = match mode_enum {
                crate::common_capnp::PollMode::All => PollMode::All,
                crate::common_capnp::PollMode::Any => PollMode::Any,
            };

            Ok(Payload::JobPoll {
                job_ids: capnp_string_list(job_ids_reader),
                timeout_ms: job.get_timeout_ms(),
                mode,
            })
        }

        tools_capnp::tool_request::JobCancel(job) => {
            let job = job?;
            Ok(Payload::JobCancel {
                job_id: job.get_job_id()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::JobList(job) => {
            let job = job?;
            let status = job.get_status()?.to_str()?;

            Ok(Payload::JobList {
                status: if status.is_empty() { None } else { Some(status.to_string()) },
            })
        }

        tools_capnp::tool_request::JobSleep(job) => {
            let job = job?;
            Ok(Payload::JobSleep {
                milliseconds: job.get_milliseconds(),
            })
        }

        // === Resource Tools ===
        tools_capnp::tool_request::ReadResource(resource) => {
            let resource = resource?;
            Ok(Payload::ReadResource {
                uri: resource.get_uri()?.to_str()?.to_string(),
            })
        }

        tools_capnp::tool_request::ListResources(()) => {
            Ok(Payload::ListResources)
        }

        // === Completion Tools ===
        tools_capnp::tool_request::Complete(complete) => {
            let complete = complete?;

            Ok(Payload::Complete {
                context: complete.get_context()?.to_str()?.to_string(),
                partial: complete.get_partial()?.to_str()?.to_string(),
            })
        }

        // === Misc Tools ===
        tools_capnp::tool_request::SampleLlm(llm) => {
            let llm = llm?;
            let system_prompt = llm.get_system_prompt()?.to_str()?;

            Ok(Payload::SampleLlm {
                prompt: llm.get_prompt()?.to_str()?.to_string(),
                max_tokens: Some(llm.get_max_tokens()),
                temperature: Some(llm.get_temperature()),
                system_prompt: if system_prompt.is_empty() { None } else { Some(system_prompt.to_string()) },
            })
        }

        tools_capnp::tool_request::ListTools(()) => {
            Ok(Payload::ListTools)
        }
    }
}

/// Helper: Convert capnp text list to Vec<String>
fn capnp_string_list(reader: capnp::text_list::Reader) -> Vec<String> {
    let mut result = Vec::new();
    for i in 0..reader.len() {
        if let Ok(s) = reader.get(i) {
            if let Ok(s_str) = s.to_str() {
                result.push(s_str.to_string());
            }
        }
    }
    result
}

/// Helper: Convert optional capnp text to Option<String>
fn capnp_optional_string(text: capnp::text::Reader) -> Option<String> {
    match text.to_str() {
        Ok(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

/// Convert a Payload response to Cap'n Proto Envelope
pub fn payload_to_capnp_envelope(
    request_id: uuid::Uuid,
    payload: &Payload,
) -> capnp::Result<capnp::message::Builder<capnp::message::HeapAllocator>> {
    let mut message = capnp::message::Builder::new_default();

    {
        let mut envelope = message.init_root::<envelope_capnp::envelope::Builder>();

        // Set request ID
        let mut id = envelope.reborrow().init_id();
        let bytes = request_id.as_bytes();
        id.set_low(u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]));
        id.set_high(u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]));

        // Empty traceparent for now
        envelope.reborrow().set_traceparent("");

        // Set payload
        let mut payload_builder = envelope.init_payload();
        payload_to_capnp_payload(&mut payload_builder, payload)?;
    }

    Ok(message)
}

/// Convert Payload to Cap'n Proto Payload builder
fn payload_to_capnp_payload(
    builder: &mut envelope_capnp::payload::Builder,
    payload: &Payload,
) -> capnp::Result<()> {
    match payload {
        Payload::Success { result } => {
            let mut success = builder.reborrow().init_success();
            success.set_result(&serde_json::to_string(result).unwrap_or_default());
        }

        Payload::Error { code, message, details } => {
            let mut error = builder.reborrow().init_error();
            error.set_code(code);
            error.set_message(message);
            if let Some(ref d) = details {
                error.set_details(&serde_json::to_string(d).unwrap_or_default());
            } else {
                error.set_details("");
            }
        }

        Payload::ToolList { tools } => {
            let tool_list = builder.reborrow().init_tool_list();
            let mut tools_builder = tool_list.init_tools(tools.len() as u32);

            for (i, tool) in tools.iter().enumerate() {
                let mut tool_builder = tools_builder.reborrow().get(i as u32);
                tool_builder.set_name(&tool.name);
                tool_builder.set_description(&tool.description);
                tool_builder.set_input_schema(&serde_json::to_string(&tool.input_schema).unwrap_or_default());
            }
        }

        // Tool requests - serialize as toolRequest variant
        Payload::Ping => {
            builder.reborrow().set_ping(());
        }

        Payload::ListTools => {
            builder.reborrow().init_tool_request().set_list_tools(());
        }

        Payload::ListResources => {
            builder.reborrow().init_tool_request().set_list_resources(());
        }

        // Garden/Timeline payloads - direct envelope variants
        Payload::GardenStatus => {
            builder.reborrow().set_garden_status(());
        }

        Payload::GardenPlay => {
            builder.reborrow().set_garden_play(());
        }

        Payload::GardenPause => {
            builder.reborrow().set_garden_pause(());
        }

        Payload::GardenStop => {
            builder.reborrow().set_garden_stop(());
        }

        Payload::GardenSeek { beat } => {
            let mut seek = builder.reborrow().init_garden_seek();
            seek.set_beat(*beat);
        }

        Payload::GardenSetTempo { bpm } => {
            let mut tempo = builder.reborrow().init_garden_set_tempo();
            tempo.set_bpm(*bpm);
        }

        Payload::GardenQuery { query, variables } => {
            let mut q = builder.reborrow().init_garden_query();
            q.set_query(query);
            if let Some(ref vars) = variables {
                q.set_variables(&serde_json::to_string(vars).unwrap_or_default());
            } else {
                q.set_variables("");
            }
        }

        Payload::GardenEmergencyPause => {
            builder.reborrow().set_garden_emergency_pause(());
        }

        Payload::GardenCreateRegion { position, duration, behavior_type, content_id } => {
            let mut region = builder.reborrow().init_garden_create_region();
            region.set_position(*position);
            region.set_duration(*duration);
            region.set_behavior_type(behavior_type);
            region.set_content_id(content_id);
        }

        Payload::GardenDeleteRegion { region_id } => {
            let mut region = builder.reborrow().init_garden_delete_region();
            region.set_region_id(region_id);
        }

        Payload::GardenMoveRegion { region_id, new_position } => {
            let mut region = builder.reborrow().init_garden_move_region();
            region.set_region_id(region_id);
            region.set_new_position(*new_position);
        }

        Payload::GardenGetRegions { start, end } => {
            let mut regions = builder.reborrow().init_garden_get_regions();
            regions.set_start(start.unwrap_or(0.0));
            regions.set_end(end.unwrap_or(0.0));
        }

        // For other payloads, we'd need to handle each variant
        // For now, convert to JSON in Success wrapper
        other => {
            let mut success = builder.reborrow().init_success();
            success.set_result(&serde_json::to_string(other).unwrap_or_default());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abc_parse_conversion() {
        let payload = Payload::AbcParse {
            abc: "X:1\nT:Test\nK:C\nCDEF".to_string(),
        };

        let request = payload_to_request(&payload).unwrap();
        assert!(matches!(request, Some(ToolRequest::AbcParse(_))));

        if let Some(ToolRequest::AbcParse(req)) = request {
            assert_eq!(req.abc, "X:1\nT:Test\nK:C\nCDEF");
        }
    }

    #[test]
    fn test_garden_status_conversion() {
        let payload = Payload::GardenStatus;
        let request = payload_to_request(&payload).unwrap();
        assert!(matches!(request, Some(ToolRequest::GardenStatus)));
    }

    #[test]
    fn test_unsupported_returns_none() {
        let payload = Payload::OrpheusGenerate {
            max_tokens: Some(1024),
            num_variations: Some(1),
            temperature: None,
            top_p: None,
            model: None,
            tags: vec![],
            creator: None,
            parent_id: None,
            variation_set_id: None,
        };

        let request = payload_to_request(&payload).unwrap();
        assert!(request.is_none());
    }

    #[test]
    fn test_envelope_to_payload_ack() {
        let envelope = ResponseEnvelope::ack("test");
        let payload = envelope_to_payload(envelope);

        match payload {
            Payload::Success { result } => {
                assert_eq!(result["status"], "ok");
                assert_eq!(result["message"], "test");
            }
            _ => panic!("Expected Success payload"),
        }
    }
}
