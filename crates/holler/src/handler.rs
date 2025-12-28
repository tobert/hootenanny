//! MCP Handler implementation for ZMQ backend forwarding
//!
//! Implements rmcp::ServerHandler to bridge MCP protocol to ZMQ backends.
//! Tools are dynamically discovered from backends and calls are routed based on prefix.
//! Tool lists are cached and refreshed when backends recover from failures.

use hooteproto::{Payload, ToolInfo};
use hooteproto::request::ToolRequest;
use hooteproto::responses::ToolResponse;
use hooteproto::envelope::ResponseEnvelope;
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, Content, Implementation,
        ListToolsResult, PaginatedRequestParam, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    RoleServer,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::backend::BackendPool;
use crate::dispatch;

/// Shared tool cache for dynamic refresh across handler instances.
///
/// This allows multiple ZmqHandler instances to share the same cached tool list.
pub type ToolCache = Arc<RwLock<Vec<Tool>>>;

/// Create a new empty tool cache.
pub fn new_tool_cache() -> ToolCache {
    Arc::new(RwLock::new(Vec::new()))
}

/// Refresh tools from hootenanny into the shared cache.
///
/// Called on startup and when backend recovers from Dead → Ready.
pub async fn refresh_tools_into(cache: &ToolCache, backends: &Arc<RwLock<BackendPool>>) -> usize {
    let backends_guard = backends.read().await;
    let tools = collect_tools_async(&backends_guard).await;
    drop(backends_guard); // Release lock before writing to cache
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
#[derive(Clone)]
pub struct ZmqHandler {
    backends: Arc<RwLock<BackendPool>>,
    /// Cached tool list - shared across handler instances
    cached_tools: ToolCache,
}

impl ZmqHandler {
    /// Create a new handler with the given backend pool and a new cache.
    pub fn new(backends: Arc<RwLock<BackendPool>>) -> Self {
        Self {
            backends,
            cached_tools: new_tool_cache(),
        }
    }

    /// Create a handler with a shared cache.
    ///
    /// Use this when you need multiple handlers to share the same tool list
    /// (e.g., for recovery callbacks to update tools visible to MCP clients).
    pub fn with_shared_cache(backends: Arc<RwLock<BackendPool>>, cache: ToolCache) -> Self {
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

impl ServerHandler for ZmqHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some("Holler MCP gateway - forwards tool calls to hootenanny ZMQ backends".to_string()),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            let tools = self.cached_tools.read().await.clone();
            Ok(ListToolsResult {
                tools,
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let name = &request.name;
            let arguments = request.arguments
                .map(serde_json::Value::Object)
                .unwrap_or(serde_json::Value::Null);

            info!(tool = %name, "Tool call via ZMQ");

            let backend = {
                let backends_guard = self.backends.read().await;
                match backends_guard.route_tool(name) {
                    Some(b) => b,
                    None => {
                        return Err(McpError::invalid_params(
                            format!("No backend available for tool: {}", name),
                            None,
                        ));
                    }
                }
            };

            // Convert JSON args to typed Payload (JSON boundary is here in holler)
            let payload = match dispatch::json_to_payload(name, arguments) {
                Ok(p) => p,
                Err(e) => {
                    return Err(McpError::invalid_params(
                        format!("Failed to parse arguments for {}: {}", name, e),
                        None,
                    ));
                }
            };

            match backend.request(payload).await {
                Ok(Payload::TypedResponse(envelope)) => {
                    let result = envelope.to_json();
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
                    Ok(CallToolResult::error(vec![Content::text(error_text)]))
                }
                Ok(other) => Err(McpError::internal_error(
                    format!("Unexpected response: {:?}", other),
                    None,
                )),
                Err(e) => Err(McpError::internal_error(
                    format!("Backend error: {}", e),
                    None,
                )),
            }
        }
    }
}

/// Async helper to collect tools from hootenanny.
async fn collect_tools_async(backends: &BackendPool) -> Vec<Tool> {
    let mut all_tools = Vec::new();

    // Query hootenanny for all tools (it proxies to vibeweaver and chaosgarden)
    if let Some(ref backend) = backends.hootenanny {
        match backend.request(Payload::ToolRequest(ToolRequest::ListTools)).await {
            Ok(Payload::TypedResponse(ResponseEnvelope::Success { response, .. })) => {
                if let ToolResponse::ToolsList(list) = response {
                    debug!("Got {} tools from hootenanny", list.tools.len());
                    all_tools.extend(list.tools.into_iter().map(tool_info_to_rmcp));
                } else {
                    debug!("hootenanny returned non-ToolsList response: {:?}", response);
                }
            }
            Ok(other) => {
                debug!("hootenanny returned unexpected response to ListTools: {:?}", other);
            }
            Err(e) => {
                warn!("Failed to get tools from hootenanny: {}", e);
            }
        }
    }

    all_tools
}

/// Convert hooteproto ToolInfo to rmcp Tool.
fn tool_info_to_rmcp(info: ToolInfo) -> Tool {
    // rmcp Tool::new takes (name, description, input_schema)
    let schema = info.input_schema.as_object()
        .cloned()
        .unwrap_or_default();
    Tool::new(info.name, info.description, Arc::new(schema))
}
