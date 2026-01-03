//! Bridge between Python API and hootenanny tools.
//! 
//! Provides synchronous access to async ZMQ tools from Python context.
//! Uses tokio's block_on() to bridge the sync/async boundary.

use anyhow::Result;
use hooteproto::request::{GardenSeekRequest, GardenSetTempoRequest, SampleRequest, ScheduleRequest, ToolRequest};
use hooteproto::{Encoding, InferenceContext, Payload, Space};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};
use tokio::runtime::Handle;

use crate::zmq_client::ZmqClient;

/// Global bridge context (set once at startup)
static BRIDGE: OnceLock<ToolBridge> = OnceLock::new();

/// Bridge context for calling hootenanny tools from Python.
#[derive(Clone)]
pub struct ToolBridge {
    /// ZMQ client connected to hootenanny (already Arc-wrapped by HootClient)
    client: Arc<ZmqClient>,
    /// Tokio runtime handle for block_on
    runtime: Handle,
}

impl ToolBridge {
    /// Create a new tool bridge.
    pub fn new(client: Arc<ZmqClient>, runtime: Handle) -> Self {
        Self { client, runtime }
    }

    /// Initialize the global bridge (call once at startup).
    pub fn init_global(bridge: ToolBridge) -> Result<()> {
        BRIDGE
            .set(bridge)
            .map_err(|_| anyhow::anyhow!("Bridge already initialized"))
    }

    /// Get the global bridge.
    pub fn global() -> Option<&'static ToolBridge> {
        BRIDGE.get()
    }

    /// Call a hootenanny tool synchronously.
    ///
    /// Uses block_in_place to allow blocking within the tokio runtime,
    /// avoiding deadlocks when called from async context (via Python).
    pub fn call_tool(&self, name: &str, args: JsonValue) -> Result<JsonValue> {
        // Convert tool name + JSON args to typed Payload
        let payload = args_to_payload(name, args)?;

        // block_in_place allows blocking in a multi-threaded runtime
        // by moving the current task to a blocking thread
        tokio::task::block_in_place(|| {
            self.runtime.block_on(async {
                match self.client.request(payload).await? {
                    Payload::TypedResponse(envelope) => Ok(envelope.to_json()),
                    Payload::Error {
                        code,
                        message,
                        details,
                    } => {
                        let error_msg = if let Some(d) = details {
                            format!(
                                "{}: {}
{}",
                                code,
                                message,
                                serde_json::to_string_pretty(&d)?
                            )
                        } else {
                            format!("{}: {}", code, message)
                        };
                        anyhow::bail!(error_msg)
                    }
                    other => anyhow::bail!("Unexpected response: {:?}", other),
                }
            })
        })
    }
}

/// Convert tool name + JSON args to typed Payload.
fn args_to_payload(name: &str, args: JsonValue) -> Result<Payload> {
    match name {
        // Garden transport tools (used from Python API)
        "garden_play" => Ok(Payload::ToolRequest(ToolRequest::GardenPlay)),
        "garden_pause" => Ok(Payload::ToolRequest(ToolRequest::GardenPause)),
        "garden_stop" => Ok(Payload::ToolRequest(ToolRequest::GardenStop)),
        "garden_status" => Ok(Payload::ToolRequest(ToolRequest::GardenStatus)),
        "garden_seek" => {
            let beat = args
                .get("beat")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("garden_seek requires 'beat' parameter"))?;
            Ok(Payload::ToolRequest(ToolRequest::GardenSeek(GardenSeekRequest { beat })))
        }
        "garden_set_tempo" => {
            let bpm = args
                .get("bpm")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("garden_set_tempo requires 'bpm' parameter"))?;
            Ok(Payload::ToolRequest(ToolRequest::GardenSetTempo(GardenSetTempoRequest { bpm })))
        }

        // Generative tools
        "sample" => {
            let space_str = args
                .get("space")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("sample requires 'space' parameter"))?;

            let space = parse_space(space_str)?;

            let inference = parse_inference(&args);

            let prompt = args.get("prompt").and_then(|v| v.as_str()).map(String::from);
            let as_loop = args.get("as_loop").and_then(|v| v.as_bool()).unwrap_or(false);
            let num_variations = args.get("num_variations").and_then(|v| v.as_u64()).map(|n| n as u32);

            Ok(Payload::ToolRequest(ToolRequest::Sample(SampleRequest {
                space,
                inference,
                num_variations,
                prompt,
                seed: None,
                as_loop,
                creator: Some("vibeweaver".to_string()),
                parent_id: None,
                tags: vec![],
                variation_set_id: None,
            })))
        }

        // Timeline scheduling
        "schedule" => {
            let encoding = parse_encoding(&args)?;
            let at = args
                .get("at")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| anyhow::anyhow!("schedule requires 'at' parameter"))?;

            let duration = args.get("duration").and_then(|v| v.as_f64());
            let gain = args.get("gain").and_then(|v| v.as_f64());
            let rate = args.get("rate").and_then(|v| v.as_f64());

            Ok(Payload::ToolRequest(ToolRequest::Schedule(ScheduleRequest {
                encoding,
                at,
                duration,
                gain,
                rate,
            })))
        }

        _ => anyhow::bail!(
            "Unknown tool: {}. Add typed dispatch for this tool in tool_bridge.rs",
            name
        ),
    }
}

/// Parse space string to Space enum
fn parse_space(s: &str) -> Result<Space> {
    match s.to_lowercase().as_str() {
        "orpheus" => Ok(Space::Orpheus),
        "orpheus_loops" | "loops" => Ok(Space::OrpheusLoops),
        "orpheus_children" | "children" => Ok(Space::OrpheusChildren),
        "orpheus_mono_melodies" | "mono_melodies" => Ok(Space::OrpheusMonoMelodies),
        "orpheus_bridge" | "bridge" => Ok(Space::OrpheusBridge),
        "musicgen" | "music_gen" => Ok(Space::MusicGen),
        "yue" => Ok(Space::Yue),
        "abc" => Ok(Space::Abc),
        _ => anyhow::bail!("Unknown space: {}", s),
    }
}

/// Parse inference context from JSON args
fn parse_inference(args: &JsonValue) -> InferenceContext {
    let inference = args.get("inference").unwrap_or(args);

    InferenceContext {
        temperature: inference.get("temperature").and_then(|v| v.as_f64()).map(|f| f as f32),
        top_p: inference.get("top_p").and_then(|v| v.as_f64()).map(|f| f as f32),
        top_k: inference.get("top_k").and_then(|v| v.as_u64()).map(|n| n as u32),
        max_tokens: inference.get("max_tokens").and_then(|v| v.as_u64()).map(|n| n as u32),
        seed: inference.get("seed").and_then(|v| v.as_u64()),
        guidance_scale: inference.get("guidance_scale").and_then(|v| v.as_f64()).map(|f| f as f32),
        variant: inference.get("variant").and_then(|v| v.as_str()).map(String::from),
        duration_seconds: inference.get("duration_seconds").and_then(|v| v.as_f64()).map(|f| f as f32),
    }
}

/// Parse encoding from JSON args
fn parse_encoding(args: &JsonValue) -> Result<Encoding> {
    // Check for artifact_id (most common case)
    if let Some(artifact_id) = args.get("artifact_id").and_then(|v| v.as_str()) {
        // Determine type from artifact_id prefix or explicit type
        let encoding_type = args
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("audio");

        return match encoding_type {
            "midi" => Ok(Encoding::Midi { artifact_id: artifact_id.to_string() }),
            "audio" | _ => Ok(Encoding::Audio { artifact_id: artifact_id.to_string() }),
        };
    }

    // Check for encoding sub-object
    if let Some(encoding) = args.get("encoding") {
        if let Some(artifact_id) = encoding.get("artifact_id").and_then(|v| v.as_str()) {
            let encoding_type = encoding.get("type").and_then(|v| v.as_str()).unwrap_or("audio");
            return match encoding_type {
                "midi" => Ok(Encoding::Midi { artifact_id: artifact_id.to_string() }),
                "audio" | _ => Ok(Encoding::Audio { artifact_id: artifact_id.to_string() }),
            };
        }

        if let Some(notation) = encoding.get("notation").and_then(|v| v.as_str()) {
            return Ok(Encoding::Abc { notation: notation.to_string() });
        }

        if let Some(hash) = encoding.get("content_hash").and_then(|v| v.as_str()) {
            let format = encoding.get("format").and_then(|v| v.as_str()).unwrap_or("audio/wav");
            return Ok(Encoding::Hash {
                content_hash: hash.to_string(),
                format: format.to_string(),
            });
        }
    }

    anyhow::bail!("schedule requires 'artifact_id' or 'encoding' parameter")
}

/// Call a hootenanny tool from anywhere (uses global bridge).
pub fn call_tool(name: &str, args: JsonValue) -> Result<JsonValue> {
    let bridge = ToolBridge::global().ok_or_else(|| {
        anyhow::anyhow!("Tool bridge not initialized - vibeweaver not connected to hootenanny")
    })?;
    bridge.call_tool(name, args)
}

/// Check if the bridge is initialized.
pub fn is_connected() -> bool {
    BRIDGE.get().is_some()
}