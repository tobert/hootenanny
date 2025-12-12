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

        let payload = match hooteproto::tool_to_payload(name, &arguments) {
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

    // Query each backend dynamically - skip those that don't support ListTools
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
                    debug!("{} returned non-tool response to ListTools: {:?}", name, other);
                }
                Err(e) => {
                    // Backend doesn't support ListTools or isn't available - skip silently
                    debug!("Skipping {} for tool discovery: {}", name, e);
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

