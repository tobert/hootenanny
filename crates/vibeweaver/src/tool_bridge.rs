//! Bridge between Python API and hootenanny tools.
//! 
//! Provides synchronous access to async ZMQ tools from Python context.
//! Uses tokio's block_on() to bridge the sync/async boundary.

use anyhow::Result;
use hooteproto::request::{GardenSeekRequest, GardenSetTempoRequest, ToolRequest};
use hooteproto::Payload;
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

        _ => anyhow::bail!(
            "Unknown tool: {}. Add typed dispatch for this tool in tool_bridge.rs",
            name
        ),
    }
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