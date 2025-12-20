//! MCP Handler implementation for ZMQ backend forwarding
//!
//! Implements baton::Handler to bridge MCP protocol to ZMQ backends.
//! Tools are dynamically discovered from backends and calls are routed based on prefix.
//! Tool lists are cached and refreshed when backends recover from failures.

use async_trait::async_trait;
use baton::{CallToolResult, Content, ErrorData, Handler, Implementation, Tool, ToolSchema};
use hooteproto::{Payload, ToolInfo};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::backend::BackendPool;
use crate::dispatch;

/// Shared tool cache for dynamic refresh across handler instances.
///
/// This allows multiple ZmqHandler instances (one for initial refresh,
/// one owned by baton's McpState) to share the same cached tool list.
pub type ToolCache = Arc<RwLock<Vec<Tool>>>;

/// Create a new empty tool cache.
pub fn new_tool_cache() -> ToolCache {
    Arc::new(RwLock::new(Vec::new()))
}

/// Refresh tools from hootenanny into the shared cache.
///
/// Called on startup and when backend recovers from Dead → Ready.
pub async fn refresh_tools_into(cache: &ToolCache, backends: &BackendPool) -> usize {
    let tools = collect_tools_async(backends).await;
    let count = tools.len();

    if count > 0 {
        info!("🔧 Refreshed {} tools from hootenanny", count);
    }

    *cache.write().await = tools;
    count
}

/// MCP Handler that forwards tool calls to ZMQ backends.
///
/// Maintains a cached list of tools that can be refreshed dynamically
/// when backends recover from failure (Dead → Ready transition).
pub struct ZmqHandler {
    backends: Arc<BackendPool>,
    /// Cached tool list - shared across handler instances
    cached_tools: ToolCache,
}

impl ZmqHandler {
    /// Create a new handler with the given backend pool and a new cache.
    pub fn new(backends: Arc<BackendPool>) -> Self {
        Self {
            backends,
            cached_tools: new_tool_cache(),
        }
    }

    /// Create a handler with a shared cache.
    ///
    /// Use this when you need multiple handlers to share the same tool list
    /// (e.g., for recovery callbacks to update tools visible to MCP clients).
    pub fn with_shared_cache(backends: Arc<BackendPool>, cache: ToolCache) -> Self {
        Self {
            backends,
            cached_tools: cache,
        }
    }

    /// Refresh tools from hootenanny and update the cache.
    ///
    /// Called on startup and when backend recovers from Dead → Ready.
    pub async fn refresh_tools(&self) -> usize {
        refresh_tools_into(&self.cached_tools, &self.backends).await
    }

    /// Get a clone of the cached tools (for async contexts).
    pub async fn get_cached_tools(&self) -> Vec<Tool> {
        self.cached_tools.read().await.clone()
    }
}

#[async_trait]
impl Handler for ZmqHandler {
    fn tools(&self) -> Vec<Tool> {
        // Return cached tools synchronously.
        //
        // The cache is populated:
        // 1. On server startup via refresh_tools()
        // 2. When backend recovers from Dead → Ready
        //
        // This avoids the blocking spawn hack and provides consistent tool lists.
        let cached_tools = Arc::clone(&self.cached_tools);

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            std::thread::spawn(move || handle.block_on(async { cached_tools.read().await.clone() }))
                .join()
                .unwrap_or_default()
        } else {
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

        // Convert JSON args to typed Payload (JSON boundary is here in holler)
        let payload = match dispatch::json_to_payload(name, arguments) {
            Ok(p) => p,
            Err(e) => {
                return Err(ErrorData::invalid_params(format!(
                    "Failed to parse arguments for {}: {}",
                    name, e
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

/// Async helper to collect tools from hootenanny.
async fn collect_tools_async(backends: &BackendPool) -> Vec<Tool> {
    let mut all_tools = Vec::new();

    // Query hootenanny for all tools (it proxies to luanette and chaosgarden)
    if let Some(ref backend) = backends.hootenanny {
        match backend.request(Payload::ListTools).await {
            Ok(Payload::ToolList { tools }) => {
                debug!("Got {} tools from hootenanny", tools.len());
                all_tools.extend(tools.into_iter().map(tool_info_to_baton));
            }
            Ok(other) => {
                debug!("hootenanny returned non-tool response to ListTools: {:?}", other);
            }
            Err(e) => {
                warn!("Failed to get tools from hootenanny: {}", e);
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

