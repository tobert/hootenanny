//! MCP Handler implementation for ZMQ backend forwarding
//!
//! Implements rmcp::ServerHandler to bridge MCP protocol to ZMQ backends.
//! Tools are dynamically discovered from backends and calls are routed based on prefix.
//! Tool lists are cached and refreshed when backends recover from failures.
//!
//! Now also supports MCP Resources and Prompts for richer agent interactions.

use hooteproto::{Payload, ToolInfo};
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    model::{
        CallToolRequestParam, CallToolResult, Content, GetPromptRequestParam, GetPromptResult,
        Implementation, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParam, ReadResourceRequestParam, ReadResourceResult,
        ResourceContents, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    RoleServer,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::backend::BackendPool;
use crate::dispatch;
use crate::prompts::{self, PromptRegistry};
use crate::resources::ResourceRegistry;

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

/// DAW tool names - high-level abstractions over model-specific tools.
pub const DAW_TOOLS: &[&str] = &[
    "sample",
    "extend",
    "analyze",
    "bridge",
    "project",
    "schedule",
];

/// MCP Handler that forwards tool calls to ZMQ backends.
///
/// Maintains a cached list of tools that can be refreshed dynamically
/// when backends recover from failure (Dead → Ready transition).
///
/// Also handles MCP Resources and Prompts for richer agent interactions.
#[derive(Clone)]
pub struct ZmqHandler {
    backends: Arc<RwLock<BackendPool>>,
    /// Cached tool list - shared across handler instances
    cached_tools: ToolCache,
    /// Only expose DAW tools
    daw_only: bool,
    /// Base URL for artifact access (e.g., "http://localhost:8082")
    artifact_base_url: Option<String>,
    /// Resource registry for MCP Resources
    resources: Arc<ResourceRegistry>,
}

impl ZmqHandler {
    /// Create a new handler with the given backend pool and a new cache.
    pub fn new(backends: Arc<RwLock<BackendPool>>) -> Self {
        let resources = Arc::new(ResourceRegistry::new(Arc::clone(&backends)));
        Self {
            backends,
            cached_tools: new_tool_cache(),
            daw_only: false,
            artifact_base_url: None,
            resources,
        }
    }

    /// Create a handler with a shared cache.
    ///
    /// Use this when you need multiple handlers to share the same tool list
    /// (e.g., for recovery callbacks to update tools visible to MCP clients).
    pub fn with_shared_cache(
        backends: Arc<RwLock<BackendPool>>,
        cache: ToolCache,
        daw_only: bool,
        artifact_base_url: Option<String>,
    ) -> Self {
        let resources = Arc::new(ResourceRegistry::new(Arc::clone(&backends)));
        Self {
            backends,
            cached_tools: cache,
            daw_only,
            artifact_base_url,
            resources,
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
                .enable_resources()
                .enable_prompts()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Holler MCP gateway - forwards tool calls to hootenanny ZMQ backends. \
                 Use resources to explore session context, artifacts, and soundfonts. \
                 Use prompts for Trustfall query templates."
                    .to_string(),
            ),
        }
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        async move {
            Ok(ListResourcesResult {
                resources: ResourceRegistry::list_resources(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_
    {
        async move {
            Ok(ListResourceTemplatesResult {
                resource_templates: ResourceRegistry::list_resource_templates(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        async move {
            let uri = &request.uri;
            debug!(uri = %uri, "Reading resource");

            match self.resources.read(uri).await {
                Ok(content) => Ok(ReadResourceResult {
                    contents: vec![ResourceContents::text(content, uri.clone())],
                }),
                Err(e) => Err(McpError::resource_not_found(e.to_string(), None)),
            }
        }
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + Send + '_ {
        async move {
            Ok(ListPromptsResult {
                prompts: PromptRegistry::list(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResult, McpError>> + Send + '_ {
        async move {
            let args = prompts::args_to_hashmap(request.arguments.as_ref());
            PromptRegistry::get(&request.name, &args)
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            let mut tools = self.cached_tools.read().await.clone();

            // Filter to DAW tools only if requested
            if self.daw_only {
                tools.retain(|t| DAW_TOOLS.contains(&t.name.as_ref()));
                debug!("DAW-only mode: exposing {} tools", tools.len());
            }

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

            info!(tool = %name, args = ?arguments, "📥 Tool call received");

            // Handle help tool locally (doesn't need backend)
            if name == "help" {
                let help_args: crate::help::HelpArgs = serde_json::from_value(arguments).unwrap_or_default();
                let response = crate::help::help(help_args);
                let text = serde_json::to_string_pretty(&response).unwrap_or_default();
                return Ok(CallToolResult::success(vec![Content::text(text)]));
            }

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
                Ok(p) => {
                    debug!("✅ JSON to Payload conversion succeeded for {}", name);
                    p
                }
                Err(e) => {
                    warn!("❌ JSON to Payload conversion failed for {}: {}", name, e);
                    return Err(McpError::invalid_params(
                        format!("Failed to parse arguments for {}: {}", name, e),
                        None,
                    ));
                }
            };

            debug!("📤 Sending {} to backend", name);
            match backend.request(payload).await {
                Ok(Payload::TypedResponse(envelope)) => {
                    let mut result = envelope.to_json();
                    // Augment response with artifact URLs if base URL is configured
                    if let Some(ref base_url) = self.artifact_base_url {
                        augment_artifact_urls(&mut result, base_url);
                    }
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

/// Collect tools from local registry.
///
/// All tools are defined statically in tools_registry - no ZMQ round-trip needed.
async fn collect_tools_async(_backends: &BackendPool) -> Vec<Tool> {
    let tools = crate::tools_registry::list_tools();
    debug!("Loaded {} tools from local registry", tools.len());
    tools.into_iter().map(tool_info_to_rmcp).collect()
}

/// Convert hooteproto ToolInfo to rmcp Tool.
fn tool_info_to_rmcp(info: ToolInfo) -> Tool {
    // rmcp Tool::new takes (name, description, input_schema)
    let schema = info.input_schema.as_object()
        .cloned()
        .unwrap_or_default();
    Tool::new(info.name, info.description, Arc::new(schema))
}

/// Augment JSON response with artifact URLs.
///
/// Walks the JSON tree and adds `artifact_url` field next to any `artifact_id` field.
/// Also handles `artifact_ids` arrays by adding `artifact_urls` array.
fn augment_artifact_urls(value: &mut serde_json::Value, base_url: &str) {
    match value {
        serde_json::Value::Object(map) => {
            // Check for artifact_id field and add artifact_url
            if let Some(serde_json::Value::String(id)) = map.get("artifact_id") {
                let url = format!("{}/artifact/{}", base_url, id);
                map.insert("artifact_url".to_string(), serde_json::Value::String(url));
            }

            // Check for id field (artifact_get returns "id" not "artifact_id")
            if let Some(serde_json::Value::String(id)) = map.get("id") {
                if id.starts_with("artifact_") {
                    let url = format!("{}/artifact/{}", base_url, id);
                    map.insert("url".to_string(), serde_json::Value::String(url));
                }
            }

            // Check for artifact_ids array and add artifact_urls array
            if let Some(serde_json::Value::Array(ids)) = map.get("artifact_ids") {
                let urls: Vec<serde_json::Value> = ids
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|id| serde_json::Value::String(format!("{}/artifact/{}", base_url, id)))
                    .collect();
                if !urls.is_empty() {
                    map.insert("artifact_urls".to_string(), serde_json::Value::Array(urls));
                }
            }

            // Recurse into nested objects
            for (_, v) in map.iter_mut() {
                augment_artifact_urls(v, base_url);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                augment_artifact_urls(item, base_url);
            }
        }
        _ => {}
    }
}
