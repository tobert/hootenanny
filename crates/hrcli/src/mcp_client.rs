//! MCP Client wrapper for hrcli.
//!
//! Wraps baton::client::SseClient with CLI-specific formatting helpers.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Re-export baton client types
pub use baton::client::{
    ClientOptions, CompletionResult, LogLevel, Notification, NotificationCallback,
    ProgressNotification, ProgressToken, SseClient, ToolAnnotations,
};

/// Tool information with CLI-friendly formatting.
///
/// Extends baton's ToolInfo with a pre-formatted `parameters` string for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: String,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Option<Value>,
    #[serde(default)]
    pub annotations: Option<ToolAnnotations>,
}

impl ToolInfo {
    /// Convert from baton's ToolInfo with formatted parameters.
    pub fn from_baton(tool: baton::client::ToolInfo) -> Self {
        let parameters = if let Some(props) = tool.input_schema.get("properties") {
            props
                .as_object()
                .map(|obj| {
                    obj.keys()
                        .map(|k| k.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        Self {
            name: tool.name,
            description: tool.description.unwrap_or_default(),
            parameters,
            input_schema: Some(tool.input_schema),
            annotations: tool.annotations,
        }
    }
}

/// MCP Client for hrcli using SSE transport.
///
/// Wraps baton's SseClient with CLI-specific helpers.
pub struct McpClient {
    inner: SseClient,
}

impl McpClient {
    /// Connect to the MCP server via SSE.
    pub async fn connect(base_url: &str) -> Result<Self> {
        let options = ClientOptions::with_name("hrcli", env!("CARGO_PKG_VERSION"));
        let inner = SseClient::connect_with_options(base_url, options, None)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect: {}", e))?;

        Ok(Self { inner })
    }

    /// Connect with an optional notification callback.
    pub async fn connect_with_callback(
        base_url: &str,
        callback: Option<NotificationCallback>,
    ) -> Result<Self> {
        let options = ClientOptions::with_name("hrcli", env!("CARGO_PKG_VERSION"));
        let inner = SseClient::connect_with_options(base_url, options, callback)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect: {}", e))?;

        Ok(Self { inner })
    }

    /// List available tools with CLI-friendly formatting.
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        let tools = self
            .inner
            .list_tools()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list tools: {}", e))?;

        Ok(tools.into_iter().map(ToolInfo::from_baton).collect())
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        self.inner
            .call_tool(tool_name, arguments)
            .await
            .map_err(|e| anyhow::anyhow!("Tool call failed: {}", e))
    }

    /// Request argument completions from the server.
    pub async fn complete_argument(
        &self,
        tool_name: &str,
        argument_name: &str,
        partial: &str,
    ) -> Result<CompletionResult> {
        self.inner
            .complete_argument(tool_name, argument_name, partial)
            .await
            .map_err(|e| anyhow::anyhow!("Completion failed: {}", e))
    }
}
