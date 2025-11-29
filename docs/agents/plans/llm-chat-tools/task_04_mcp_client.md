# Task 04: Implement MCP tool client with tracing

## Goal

HTTP client that calls hootenanny MCP tools with trace context propagation.

## Files to Create

- `crates/llm-mcp-bridge/src/mcp_client.rs`

## Key Requirements

1. Call tools via MCP JSON-RPC over HTTP
2. Propagate traceparent header for distributed tracing
3. List available tools for schema conversion
4. Convert MCP tool schemas to OpenAI function format

## Implementation

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing;

/// MCP tool information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

/// Lightweight MCP client for tool calls (POST-only, no SSE)
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

        self.send_request(request).await?;
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

        // Extract result from JSON-RPC response
        if let Some(error) = response.get("error") {
            let message = error.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("MCP tool call failed: {}", message);
        }

        response.get("result")
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

        serde_json::from_value(tools.clone())
            .context("Failed to parse tools list")
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

        let mut req_builder = self.client
            .post(&format!("{}/mcp", self.base_url))
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
            let flags = if span_context.is_sampled() { "01" } else { "00" };

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
        let client = McpToolClient::new("http://localhost:8080");

        let tools = vec![
            ToolInfo {
                name: "orpheus_generate".to_string(),
                description: Some("Generate MIDI with Orpheus".to_string()),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "model": { "type": "string" }
                    }
                }),
            },
        ];

        let functions = client.to_openai_functions(&tools);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "orpheus_generate");
    }
}
```

## Reference Files

- `crates/hootenanny/tests/common/mcp_client.rs` - Existing MCP client implementation
- `crates/hootenanny/src/mcp_tools/local_models.rs:121-140` - traceparent injection pattern

## Testing

Use `wiremock` to mock MCP server responses:

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path, header_exists};

    #[tokio::test]
    async fn test_list_tools() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": "test",
                "result": {
                    "tools": [
                        {
                            "name": "test_tool",
                            "description": "A test tool",
                            "inputSchema": { "type": "object" }
                        }
                    ]
                }
            })))
            .mount(&mock_server)
            .await;

        let client = McpToolClient::new(&mock_server.uri());
        let tools = client.list_tools().await.unwrap();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "test_tool");
    }

    #[tokio::test]
    async fn test_call_tool_propagates_traceparent() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/mcp"))
            .and(header_exists("traceparent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": "test",
                "result": { "content": [{ "type": "text", "text": "ok" }] }
            })))
            .mount(&mock_server)
            .await;

        // Would need to set up tracing context to test traceparent
    }
}
```

## Acceptance Criteria

- [ ] Can initialize MCP session
- [ ] Can list tools from MCP server
- [ ] Can call tools with arguments
- [ ] Traceparent header propagated when span context available
- [ ] Converts MCP tool schemas to OpenAI function format
- [ ] Handles JSON-RPC errors gracefully
- [ ] Integration tests with wiremock pass
