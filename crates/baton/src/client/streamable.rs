//! Streamable HTTP MCP Client.
//!
//! This is the recommended transport for MCP clients. It uses simple HTTP POST
//! requests with JSON-RPC payloads. Responses are returned directly in the HTTP
//! response body.

use std::sync::atomic::{AtomicU64, Ordering};

use reqwest::Client;
use serde_json::Value;

use super::{ClientOptions, InitializeResult, ToolInfo};
use crate::types::completion::CompletionResult;

/// MCP client using Streamable HTTP transport.
///
/// This client communicates with MCP servers using HTTP POST requests.
/// It's simpler than SSE and is the recommended transport for most use cases.
pub struct McpClient {
    base_url: String,
    client: Client,
    session_id: String,
    request_id: AtomicU64,
    options: ClientOptions,
}

impl McpClient {
    /// Create a new MCP client for the given URL.
    ///
    /// The URL should be the MCP endpoint (e.g., "http://localhost:8080/mcp").
    pub fn new(base_url: &str) -> Self {
        Self::with_options(base_url, ClientOptions::default())
    }

    /// Create a new MCP client with custom options.
    pub fn with_options(base_url: &str, options: ClientOptions) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::new(),
            session_id: uuid::Uuid::new_v4().to_string(),
            request_id: AtomicU64::new(1),
            options,
        }
    }

    /// Get the base URL of this client.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Initialize the MCP session.
    ///
    /// This must be called before any other methods. It performs the MCP
    /// handshake and returns server information.
    #[tracing::instrument(skip(self), fields(mcp.url = %self.base_url))]
    pub async fn initialize(&self) -> Result<InitializeResult, ClientError> {
        let mut capabilities = serde_json::json!({});
        if self.options.enable_sampling {
            capabilities["sampling"] = serde_json::json!({});
        }

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": capabilities,
                "clientInfo": {
                    "name": self.options.client_name,
                    "version": self.options.client_version
                }
            }
        });

        let response = self.send_request(request).await?;

        let result: InitializeResult = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .ok_or_else(|| ClientError::Protocol("Missing result in initialize response".into()))?,
        )
        .map_err(|e| ClientError::Protocol(format!("Invalid initialize response: {}", e)))?;

        // Send initialized notification
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });

        self.send_notification(notification).await?;

        tracing::info!(
            server = %result.server_info.name,
            version = ?result.server_info.version,
            "MCP session initialized"
        );

        Ok(result)
    }

    /// List available tools from the MCP server.
    #[tracing::instrument(skip(self))]
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>, ClientError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/list",
            "params": {}
        });

        let response = self.send_request(request).await?;

        let tools = response
            .get("result")
            .and_then(|r| r.get("tools"))
            .ok_or_else(|| ClientError::Protocol("Missing tools in response".into()))?;

        serde_json::from_value(tools.clone())
            .map_err(|e| ClientError::Protocol(format!("Failed to parse tools: {}", e)))
    }

    /// Call a tool on the MCP server.
    ///
    /// Returns the tool's result, extracting structured content if available.
    #[tracing::instrument(
        skip(self, arguments),
        fields(
            tool.name = %name,
            mcp.session_id = %self.session_id,
        )
    )]
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, ClientError> {
        self.call_tool_with_traceparent(name, arguments, None).await
    }

    /// Call a tool on the MCP server with explicit traceparent.
    ///
    /// Use this when calling from a blocking context where the current span
    /// isn't available (e.g., from `spawn_blocking`).
    #[tracing::instrument(
        skip(self, arguments, traceparent),
        fields(
            tool.name = %name,
            mcp.session_id = %self.session_id,
        )
    )]
    pub async fn call_tool_with_traceparent(
        &self,
        name: &str,
        arguments: Value,
        traceparent: Option<&str>,
    ) -> Result<Value, ClientError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });

        let response = self.send_request_with_traceparent(request, traceparent).await?;

        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            return Err(ClientError::ToolCall {
                name: name.to_string(),
                code,
                message: message.to_string(),
            });
        }

        let result = response
            .get("result")
            .ok_or_else(|| ClientError::Protocol("Missing result in response".into()))?;

        // Extract structured content if available
        if let Some(structured) = result.get("structuredContent") {
            return Ok(structured.clone());
        }

        // Fall back to content array
        if let Some(content) = result.get("content") {
            if let Some(arr) = content.as_array() {
                if let Some(first) = arr.first() {
                    if let Some(text) = first.get("text").and_then(|t| t.as_str()) {
                        // Try to parse as JSON
                        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                            return Ok(parsed);
                        }
                        return Ok(Value::String(text.to_string()));
                    }
                }
            }
        }

        Ok(result.clone())
    }

    /// Request argument completions from the server.
    #[tracing::instrument(skip(self))]
    pub async fn complete_argument(
        &self,
        tool_name: &str,
        argument_name: &str,
        partial: &str,
    ) -> Result<CompletionResult, ClientError> {
        let params = serde_json::json!({
            "ref": {
                "type": "ref/argument",
                "name": tool_name,
                "argumentName": argument_name
            },
            "argument": {
                "name": argument_name,
                "value": partial
            }
        });

        let response = self.request("completion/complete", params).await?;

        let result = response
            .get("completion")
            .ok_or_else(|| ClientError::Protocol("Missing completion field".into()))?;

        serde_json::from_value(result.clone())
            .map_err(|e| ClientError::Protocol(format!("Failed to parse completion: {}", e)))
    }

    /// Generic MCP request.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": method,
            "params": params
        });

        let response = self.send_request(request).await?;

        response
            .get("result")
            .cloned()
            .ok_or_else(|| ClientError::Protocol("Missing result in response".into()))
    }

    /// Send a JSON-RPC request with trace context.
    async fn send_request(&self, request: Value) -> Result<Value, ClientError> {
        self.send_request_with_traceparent(request, None).await
    }

    /// Send a JSON-RPC request with explicit or automatic trace context.
    ///
    /// If `traceparent` is None, attempts to extract from current span.
    async fn send_request_with_traceparent(
        &self,
        request: Value,
        traceparent: Option<&str>,
    ) -> Result<Value, ClientError> {
        let traceparent = traceparent
            .map(|s| s.to_string())
            .or_else(|| self.current_traceparent());

        let mut req_builder = self
            .client
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .header("Mcp-Session-Id", &self.session_id)
            .timeout(std::time::Duration::from_secs(self.options.timeout_secs));

        if let Some(tp) = traceparent {
            req_builder = req_builder.header("traceparent", tp);
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ClientError::Http {
                status: status.as_u16(),
                body,
            });
        }

        response
            .json::<Value>()
            .await
            .map_err(|e| ClientError::Transport(format!("Failed to parse response: {}", e)))
    }

    /// Send a notification (no response expected).
    async fn send_notification(&self, notification: Value) -> Result<(), ClientError> {
        let mut req_builder = self
            .client
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .header("Mcp-Session-Id", &self.session_id)
            .timeout(std::time::Duration::from_secs(self.options.timeout_secs));

        if let Some(tp) = self.current_traceparent() {
            req_builder = req_builder.header("traceparent", tp);
        }

        let response = req_builder
            .json(&notification)
            .send()
            .await
            .map_err(|e| ClientError::Transport(e.to_string()))?;

        let status = response.status();
        if status != reqwest::StatusCode::ACCEPTED && !status.is_success() {
            return Err(ClientError::Http {
                status: status.as_u16(),
                body: format!("Notification failed with status {}", status),
            });
        }

        Ok(())
    }

    /// Extract traceparent from current span for distributed tracing.
    fn current_traceparent(&self) -> Option<String> {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        let span = tracing::Span::current();
        let context = span.context();
        let ctx_span = context.span();
        let span_context = ctx_span.span_context();

        if span_context.is_valid() {
            let trace_id = span_context.trace_id();
            let span_id = span_context.span_id();
            let flags = if span_context.is_sampled() {
                "01"
            } else {
                "00"
            };

            Some(format!("00-{}-{}-{}", trace_id, span_id, flags))
        } else {
            None
        }
    }
}

/// Errors that can occur when using the MCP client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// HTTP transport error
    #[error("Transport error: {0}")]
    Transport(String),

    /// HTTP status error
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    /// Protocol error (invalid response format)
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Tool call error
    #[error("Tool '{name}' failed (code {code}): {message}")]
    ToolCall {
        name: String,
        code: i64,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_options_default() {
        let opts = ClientOptions::default();
        assert_eq!(opts.client_name, "baton-client");
        assert_eq!(opts.timeout_secs, 30);
        assert!(!opts.enable_sampling);
    }

    #[test]
    fn test_client_options_with_name() {
        let opts = ClientOptions::with_name("my-app", "1.0.0");
        assert_eq!(opts.client_name, "my-app");
        assert_eq!(opts.client_version, "1.0.0");
    }

    #[test]
    fn test_new_client() {
        let client = McpClient::new("http://localhost:8080/mcp");
        assert_eq!(client.base_url(), "http://localhost:8080/mcp");
        assert!(!client.session_id().is_empty());
    }

    #[test]
    fn test_url_trailing_slash_stripped() {
        let client = McpClient::new("http://localhost:8080/mcp/");
        assert_eq!(client.base_url(), "http://localhost:8080/mcp");
    }
}
