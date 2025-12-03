//! MCP Protocol Dispatch
//!
//! Routes JSON-RPC methods to their handlers.
//!
//! Implements OpenTelemetry JSON-RPC semantic conventions for observability.
//! See: https://opentelemetry.io/docs/specs/semconv/rpc/json-rpc/

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::Instrument;

use crate::transport::McpState;
use crate::types::error::ErrorData;
use crate::types::jsonrpc::JsonRpcMessage;
use crate::types::progress::{ProgressNotification, ProgressToken};
use crate::types::prompt::{GetPromptResult, ListPromptsResult, Prompt};
use crate::types::protocol::{
    InitializeParams, InitializeResult, Implementation, ServerCapabilities,
};
use crate::types::resource::{
    ListResourceTemplatesResult, ListResourcesResult, ReadResourceResult,
    Resource, ResourceTemplate,
};
use crate::types::tool::{CallToolParams, CallToolResult, ListToolsResult, Tool};

/// Sender for progress notifications.
///
/// Tools can use this to send progress updates back to the client during
/// long-running operations.
pub type ProgressSender = tokio::sync::mpsc::Sender<ProgressNotification>;

/// Context passed to tool calls for sending progress updates and accessing session info.
#[derive(Clone)]
pub struct ToolContext {
    /// Session ID for this request.
    pub session_id: String,

    /// Progress token from the request metadata (if client requested progress).
    pub progress_token: Option<ProgressToken>,

    /// Sender for progress notifications (if client requested progress).
    pub progress_sender: Option<ProgressSender>,

    /// Sampler for requesting LLM inference from the client (if supported).
    pub sampler: Option<Sampler>,
}

/// Handle for making sampling requests to the connected client's LLM.
#[derive(Clone)]
pub struct Sampler {
    client: Arc<crate::transport::SamplingClient>,
    sessions: Arc<dyn crate::session::SessionStore>,
    session_id: String,
}

impl Sampler {
    /// Create a new sampler for the given session.
    pub fn new(
        client: Arc<crate::transport::SamplingClient>,
        sessions: Arc<dyn crate::session::SessionStore>,
        session_id: String,
    ) -> Self {
        Self {
            client,
            sessions,
            session_id,
        }
    }

    /// Request a simple text completion from the client's LLM.
    ///
    /// This is a convenience wrapper that creates a simple user message request.
    pub async fn ask(&self, question: impl Into<String>) -> Result<String, crate::transport::SamplingError> {
        use crate::types::sampling::{SamplingMessage, SamplingRequest};

        let request = SamplingRequest {
            messages: vec![SamplingMessage::user(question)],
            max_tokens: Some(500),
            ..Default::default()
        };

        let response = self.sample(request).await?;

        // Extract text from response content
        if let Some(text_content) = response.content.as_text() {
            Ok(text_content.to_string())
        } else {
            Ok(String::new())
        }
    }

    /// Request sampling with full control over parameters.
    pub async fn sample(
        &self,
        request: crate::types::sampling::SamplingRequest,
    ) -> Result<crate::types::sampling::SamplingResponse, crate::transport::SamplingError> {
        // Get the session
        let session = self
            .sessions
            .get(&self.session_id)
            .ok_or(crate::transport::SamplingError::SessionNotFound)?;

        // Send sampling request through the client
        self.client.sample(session, request, None).await
    }
}

impl ToolContext {
    /// Send a progress notification to the client.
    ///
    /// Does nothing if no progress sender is available.
    pub async fn send_progress(&self, progress: ProgressNotification) {
        if let Some(ref sender) = self.progress_sender {
            let _ = sender.send(progress).await;
        }
    }

    /// Check if progress reporting is enabled for this request.
    pub fn has_progress(&self) -> bool {
        self.progress_token.is_some()
    }
}

/// Handler trait for MCP server implementations.
///
/// Implement this trait to provide tools, resources, and prompts.
#[async_trait]
pub trait Handler: Send + Sync + 'static {
    // === Required: Tools ===

    /// Return the list of available tools.
    fn tools(&self) -> Vec<Tool>;

    /// Execute a tool call.
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<CallToolResult, ErrorData>;

    /// Execute a tool call with context for progress reporting.
    ///
    /// Default implementation calls `call_tool` (ignoring context).
    /// Override this to support progress notifications for long-running operations.
    async fn call_tool_with_context(
        &self,
        name: &str,
        arguments: Value,
        _context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        self.call_tool(name, arguments).await
    }

    // === Required: Server Info ===

    /// Return server implementation info.
    fn server_info(&self) -> Implementation;

    // === Optional: Resources ===

    /// Return the list of available resources.
    fn resources(&self) -> Vec<Resource> {
        vec![]
    }

    /// Return the list of resource templates.
    fn resource_templates(&self) -> Vec<ResourceTemplate> {
        vec![]
    }

    /// Read a resource by URI.
    async fn read_resource(&self, _uri: &str) -> Result<ReadResourceResult, ErrorData> {
        Err(ErrorData::method_not_found("resources/read"))
    }

    // === Optional: Prompts ===

    /// Return the list of available prompts.
    fn prompts(&self) -> Vec<Prompt> {
        vec![]
    }

    /// Get a prompt by name with arguments.
    async fn get_prompt(
        &self,
        _name: &str,
        _arguments: HashMap<String, String>,
    ) -> Result<GetPromptResult, ErrorData> {
        Err(ErrorData::method_not_found("prompts/get"))
    }

    // === Optional: Metadata ===

    /// Return instructions for the LLM.
    fn instructions(&self) -> Option<String> {
        None
    }

    /// Return server capabilities.
    fn capabilities(&self) -> ServerCapabilities {
        let mut caps = ServerCapabilities::default().enable_tools();

        if !self.resources().is_empty() || !self.resource_templates().is_empty() {
            caps = caps.enable_resources();
        }

        if !self.prompts().is_empty() {
            caps = caps.enable_prompts();
        }

        caps
    }
}

/// Dispatch a JSON-RPC message to the appropriate handler.
///
/// Creates an OpenTelemetry span following JSON-RPC semantic conventions:
/// - `rpc.system` = "jsonrpc"
/// - `rpc.method` = the JSON-RPC method name
/// - `rpc.jsonrpc.version` = "2.0"
/// - `rpc.jsonrpc.request_id` = the request ID (if present)
/// - `mcp.session_id` = the MCP session identifier
pub async fn dispatch<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    message: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    // Format request_id for OTEL (cast to string per spec)
    let request_id_str = message
        .id
        .as_ref()
        .map(|id| format!("{}", id))
        .unwrap_or_default();

    // Create span following JSON-RPC semantic conventions
    // Span name format: mcp/{method}
    let span = tracing::info_span!(
        "mcp.dispatch",
        rpc.system = "jsonrpc",
        rpc.method = %message.method,
        rpc.jsonrpc.version = "2.0",
        rpc.jsonrpc.request_id = %request_id_str,
        mcp.session_id = %session_id,
        // Error fields - recorded on failure
        error.type = tracing::field::Empty,
        rpc.jsonrpc.error_code = tracing::field::Empty,
        rpc.jsonrpc.error_message = tracing::field::Empty,
    );

    async {
        let result = dispatch_inner(state, session_id, message).await;

        // Record error on span if dispatch failed
        if let Err(ref error) = result {
            record_error_on_span(error);
        }

        result
    }
    .instrument(span)
    .await
}

/// Record JSON-RPC error on the current span following OTEL conventions.
fn record_error_on_span(error: &ErrorData) {
    let span = tracing::Span::current();
    span.record("error.type", error_type_for_code(error.code));
    span.record("rpc.jsonrpc.error_code", error.code);
    span.record("rpc.jsonrpc.error_message", error.message.as_str());
}

/// Map JSON-RPC error codes to error.type values.
fn error_type_for_code(code: i32) -> &'static str {
    match code {
        -32700 => "parse_error",
        -32600 => "invalid_request",
        -32601 => "method_not_found",
        -32602 => "invalid_params",
        -32603 => "internal_error",
        _ => "application_error",
    }
}

/// Inner dispatch without span (called from instrumented outer function).
async fn dispatch_inner<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    message: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    match message.method.as_str() {
        // Lifecycle
        "initialize" => handle_initialize(state, session_id, message).await,
        "notifications/initialized" => Ok(Value::Null),
        "ping" => Ok(serde_json::json!({})),

        // Tools
        "tools/list" => handle_list_tools(state).await,
        "tools/call" => handle_call_tool(state, session_id, message).await,

        // Resources
        "resources/list" => handle_list_resources(state).await,
        "resources/templates/list" => handle_list_resource_templates(state).await,
        "resources/read" => handle_read_resource(state, message).await,

        // Prompts
        "prompts/list" => handle_list_prompts(state).await,
        "prompts/get" => handle_get_prompt(state, message).await,

        // Unknown
        _ => Err(ErrorData::method_not_found(&message.method)),
    }
}

async fn handle_initialize<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    let params: InitializeParams = request
        .params
        .as_ref()
        .map(|p| serde_json::from_value(p.clone()))
        .transpose()
        .map_err(|e| ErrorData::invalid_params(format!("Invalid initialize params: {}", e)))?
        .ok_or_else(|| ErrorData::invalid_params("Missing initialize params"))?;

    // Store client info in session
    state.sessions.set_initialized(session_id, params.client_info);

    let result = InitializeResult::new(
        Implementation::new(&state.server_name, &state.server_version),
        state.handler.capabilities(),
    );

    let result = if let Some(instructions) = state.handler.instructions() {
        result.with_instructions(instructions)
    } else {
        result
    };

    serde_json::to_value(&result)
        .map_err(|e| ErrorData::internal_error(format!("Failed to serialize result: {}", e)))
}

async fn handle_list_tools<H: Handler>(state: &Arc<McpState<H>>) -> Result<Value, ErrorData> {
    let tools = state.handler.tools();
    let result = ListToolsResult::all(tools);

    serde_json::to_value(&result)
        .map_err(|e| ErrorData::internal_error(format!("Failed to serialize result: {}", e)))
}

async fn handle_call_tool<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    let params: CallToolParams = request
        .params
        .as_ref()
        .map(|p| serde_json::from_value(p.clone()))
        .transpose()
        .map_err(|e| ErrorData::invalid_params(format!("Invalid call params: {}", e)))?
        .ok_or_else(|| ErrorData::invalid_params("Missing call params"))?;

    let arguments = params
        .arguments
        .map(Value::Object)
        .unwrap_or(Value::Object(serde_json::Map::new()));

    // Extract progress token from _meta field if present
    let progress_token = request
        .params
        .as_ref()
        .and_then(|p| p.get("_meta"))
        .and_then(|m| m.get("progressToken"))
        .and_then(|t| serde_json::from_value::<ProgressToken>(t.clone()).ok());

    // Create progress channel if token is present
    let (progress_tx, progress_rx) = if progress_token.is_some() {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    // Spawn task to forward progress notifications to the client's SSE channel
    if let (Some(_token), Some(mut rx)) = (progress_token.clone(), progress_rx) {
        let session = state.sessions.get(session_id);
        let session_tx = session.and_then(|s| s.tx.clone());

        tokio::spawn(async move {
            while let Some(progress) = rx.recv().await {
                // Send as a JSON-RPC notification
                let notification = JsonRpcMessage::notification(
                    "notifications/progress",
                    serde_json::to_value(&progress).unwrap_or_default(),
                );

                if let Some(ref tx) = session_tx {
                    // Convert to SSE event
                    let event_data = serde_json::to_string(&notification).unwrap_or_default();
                    let event = axum::response::sse::Event::default()
                        .event("message")
                        .data(event_data);

                    let _ = tx.send(Ok(event)).await;
                }
            }
        });
    }

    // Create tool context
    // TODO: Add sampler when client capabilities support sampling
    let context = ToolContext {
        session_id: session_id.to_string(),
        progress_token,
        progress_sender: progress_tx,
        sampler: None, // Not yet checking client capabilities
    };

    // Create child span for tool execution with MCP-specific attributes
    let tool_span = tracing::info_span!(
        "mcp.tool.call",
        mcp.tool.name = %params.name,
        mcp.session_id = %session_id,
        mcp.has_progress = %context.has_progress(),
    );

    async {
        let result = state.handler.call_tool_with_context(&params.name, arguments, context).await?;

        serde_json::to_value(&result)
            .map_err(|e| ErrorData::internal_error(format!("Failed to serialize result: {}", e)))
    }
    .instrument(tool_span)
    .await
}

async fn handle_list_resources<H: Handler>(state: &Arc<McpState<H>>) -> Result<Value, ErrorData> {
    let resources = state.handler.resources();
    let result = ListResourcesResult::all(resources);

    serde_json::to_value(&result)
        .map_err(|e| ErrorData::internal_error(format!("Failed to serialize result: {}", e)))
}

async fn handle_list_resource_templates<H: Handler>(
    state: &Arc<McpState<H>>,
) -> Result<Value, ErrorData> {
    let templates = state.handler.resource_templates();
    let result = ListResourceTemplatesResult::all(templates);

    serde_json::to_value(&result)
        .map_err(|e| ErrorData::internal_error(format!("Failed to serialize result: {}", e)))
}

async fn handle_read_resource<H: Handler>(
    state: &Arc<McpState<H>>,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    #[derive(serde::Deserialize)]
    struct Params {
        uri: String,
    }

    let params: Params = request
        .params
        .as_ref()
        .map(|p| serde_json::from_value(p.clone()))
        .transpose()
        .map_err(|e| ErrorData::invalid_params(format!("Invalid read params: {}", e)))?
        .ok_or_else(|| ErrorData::invalid_params("Missing read params"))?;

    // Create child span for resource read with MCP-specific attributes
    let resource_span = tracing::info_span!(
        "mcp.resource.read",
        mcp.resource.uri = %params.uri,
    );

    async {
        let result = state.handler.read_resource(&params.uri).await?;

        serde_json::to_value(&result)
            .map_err(|e| ErrorData::internal_error(format!("Failed to serialize result: {}", e)))
    }
    .instrument(resource_span)
    .await
}

async fn handle_list_prompts<H: Handler>(state: &Arc<McpState<H>>) -> Result<Value, ErrorData> {
    let prompts = state.handler.prompts();
    let result = ListPromptsResult::all(prompts);

    serde_json::to_value(&result)
        .map_err(|e| ErrorData::internal_error(format!("Failed to serialize result: {}", e)))
}

async fn handle_get_prompt<H: Handler>(
    state: &Arc<McpState<H>>,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    #[derive(serde::Deserialize)]
    struct Params {
        name: String,
        #[serde(default)]
        arguments: Option<HashMap<String, String>>,
    }

    let params: Params = request
        .params
        .as_ref()
        .map(|p| serde_json::from_value(p.clone()))
        .transpose()
        .map_err(|e| ErrorData::invalid_params(format!("Invalid get params: {}", e)))?
        .ok_or_else(|| ErrorData::invalid_params("Missing get params"))?;

    // Create child span for prompt get with MCP-specific attributes
    let prompt_span = tracing::info_span!(
        "mcp.prompt.get",
        mcp.prompt.name = %params.name,
    );

    async {
        let arguments = params.arguments.unwrap_or_default();
        let result = state.handler.get_prompt(&params.name, arguments).await?;

        serde_json::to_value(&result)
            .map_err(|e| ErrorData::internal_error(format!("Failed to serialize result: {}", e)))
    }
    .instrument(prompt_span)
    .await
}
