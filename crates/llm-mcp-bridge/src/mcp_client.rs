use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP tool information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Lightweight MCP client for tool calls (Streamable HTTP, POST-only)
pub struct McpToolClient {
    base_url: String,
    client: reqwest::Client,
    session_id: String,
}

impl McpToolClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            session_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Initialize the MCP session
    pub async fn initialize(&self) -> Result<()> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {
                    "name": "llm-mcp-bridge",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        let _response = self.send_request(request).await?;

        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });

        self.send_notification(notification).await?;

        Ok(())
    }

    /// Call a tool via MCP HTTP, propagating trace context
    #[tracing::instrument(
        skip(self, arguments),
        fields(
            tool.name = %name,
            mcp.session_id = %self.session_id,
        )
    )]
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });

        let response = self.send_request(request).await?;

        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("MCP tool call failed: {}", message);
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing result in MCP response"))
    }

    /// List available tools from the MCP server
    #[tracing::instrument(skip(self))]
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": "tools/list",
            "params": {}
        });

        let response = self.send_request(request).await?;

        let tools = response
            .get("result")
            .and_then(|r| r.get("tools"))
            .ok_or_else(|| anyhow::anyhow!("Missing tools in response"))?;

        serde_json::from_value(tools.clone()).context("Failed to parse tools list")
    }

    /// Convert MCP tools to OpenAI function format
    pub fn to_openai_functions(&self, tools: &[ToolInfo]) -> Vec<OpenAiFunction> {
        tools
            .iter()
            .map(|tool| OpenAiFunction {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.input_schema.clone(),
            })
            .collect()
    }

    /// Send a JSON-RPC request with trace context
    async fn send_request(&self, request: Value) -> Result<Value> {
        let traceparent = self.current_traceparent();

        let mut req_builder = self
            .client
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .header("Mcp-Session-Id", &self.session_id);

        if let Some(tp) = traceparent {
            req_builder = req_builder.header("traceparent", tp);
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .context("Failed to send MCP request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("MCP request failed with status {}: {}", status, body);
        }

        response
            .json::<Value>()
            .await
            .context("Failed to parse MCP response")
    }

    /// Send a notification (no response expected)
    async fn send_notification(&self, notification: Value) -> Result<()> {
        let mut req_builder = self
            .client
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .header("Mcp-Session-Id", &self.session_id);

        if let Some(tp) = self.current_traceparent() {
            req_builder = req_builder.header("traceparent", tp);
        }

        let response = req_builder
            .json(&notification)
            .send()
            .await
            .context("Failed to send MCP notification")?;

        let status = response.status();
        if status != reqwest::StatusCode::ACCEPTED && !status.is_success() {
            anyhow::bail!("MCP notification failed with status {}", status);
        }

        Ok(())
    }

    /// Extract traceparent from current span
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

/// OpenAI function definition format
#[derive(Debug, Clone, Serialize)]
pub struct OpenAiFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_openai_functions() {
        let client = McpToolClient::new("http://localhost:8080/mcp");

        let tools = vec![ToolInfo {
            name: "orpheus_generate".to_string(),
            description: Some("Generate MIDI with Orpheus".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "model": { "type": "string" }
                }
            }),
        }];

        let functions = client.to_openai_functions(&tools);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "orpheus_generate");
    }
}
