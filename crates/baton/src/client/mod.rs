//! MCP Client for connecting to MCP servers.
//!
//! This module provides a client for communicating with MCP servers using
//! either the Streamable HTTP transport (recommended) or the legacy SSE transport.
//!
//! # Example
//!
//! ```rust,ignore
//! use baton::client::McpClient;
//!
//! let client = McpClient::new("http://localhost:8080/mcp");
//! client.initialize().await?;
//!
//! let tools = client.list_tools().await?;
//! let result = client.call_tool("my_tool", json!({"arg": "value"})).await?;
//! ```

mod streamable;
mod sse;

pub use streamable::{McpClient, ClientError};
pub use sse::{SseClient, SseClientError};

// Re-export types used by clients
pub use crate::types::completion::CompletionResult;
pub use crate::types::logging::LogLevel;
pub use crate::types::progress::{ProgressNotification, ProgressToken};
pub use crate::types::tool::ToolAnnotations;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Server information returned from initialize.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// Server name
    pub name: String,
    /// Server version
    #[serde(default)]
    pub version: Option<String>,
}

/// Result of MCP initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Protocol version
    pub protocol_version: String,
    /// Server capabilities
    pub capabilities: serde_json::Value,
    /// Server info
    pub server_info: ServerInfo,
    /// Optional instructions for the LLM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// Tool information from tools/list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    #[serde(default)]
    pub description: Option<String>,
    /// Input schema
    pub input_schema: serde_json::Value,
    /// Tool annotations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<crate::types::tool::ToolAnnotations>,
}

/// Log message from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    /// Log level
    pub level: LogLevel,
    /// Optional logger name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    /// Message data
    #[serde(rename = "data")]
    pub message: serde_json::Value,
}

/// Notification from the server.
#[derive(Debug, Clone)]
pub enum Notification {
    /// Progress update for a long-running operation
    Progress(ProgressNotification),
    /// Log message from the server
    Log(LogMessage),
}

/// Callback type for receiving notifications.
pub type NotificationCallback = Arc<dyn Fn(Notification) + Send + Sync>;

/// Options for configuring the MCP client.
#[derive(Debug, Clone)]
pub struct ClientOptions {
    /// Client name for initialization
    pub client_name: String,
    /// Client version for initialization
    pub client_version: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Enable sampling capability
    pub enable_sampling: bool,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            client_name: "baton-client".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            timeout_secs: 30,
            enable_sampling: false,
        }
    }
}

impl ClientOptions {
    /// Create options with custom client name.
    pub fn with_name(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            client_name: name.into(),
            client_version: version.into(),
            ..Default::default()
        }
    }
}
