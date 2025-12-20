//! ZMQ client for upstream hootenanny server.
//!
//! Connects directly to hootenanny via ZMQ DEALER socket using HOOT01 + Cap'n Proto.
//! Uses the shared HootClient from hooteproto for connection management.

use anyhow::{Context, Result};
use hooteproto::{ClientConfig, HootClient, Payload, ToolInfo};
use std::sync::Arc;
use tracing::info;

/// Configuration for the upstream hootenanny connection.
#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    /// Namespace prefix for tools (e.g., "hootenanny" -> hootenanny.*)
    pub namespace: String,
    /// ZMQ endpoint (e.g., "tcp://localhost:5580")
    pub endpoint: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
}

/// Manages the connection to upstream hootenanny.
pub struct ClientManager {
    client: Option<Arc<HootClient>>,
    namespace: String,
    tools: Vec<ToolInfo>,
}

impl ClientManager {
    /// Create a new empty client manager.
    pub fn new() -> Self {
        Self {
            client: None,
            namespace: String::new(),
            tools: Vec::new(),
        }
    }

    /// Connect to upstream hootenanny via ZMQ.
    ///
    /// Uses lazy connection pattern - ZMQ connect is non-blocking and peer doesn't
    /// need to exist. Tool discovery is attempted but failure is non-fatal.
    #[tracing::instrument(skip(self), fields(namespace = %config.namespace, endpoint = %config.endpoint))]
    pub async fn add_upstream(&mut self, config: UpstreamConfig) -> Result<()> {
        info!("Connecting to upstream hootenanny (lazy)");

        let client_config = ClientConfig::new("hootenanny", &config.endpoint)
            .with_timeout(config.timeout_ms);

        // ZMQ connect is non-blocking - always succeeds
        let client = HootClient::new(client_config).await;

        // Try to discover tools, but don't fail if backend isn't ready yet
        match self.discover_tools_from(&client).await {
            Ok(tools) => {
                info!("Discovered {} tools from hootenanny", tools.len());
                self.tools = tools;
            }
            Err(e) => {
                info!("Tool discovery deferred (backend not ready): {}", e);
                self.tools = Vec::new();
            }
        }

        self.namespace = config.namespace;
        self.client = Some(client);

        Ok(())
    }

    /// Discover tools from a HootClient
    async fn discover_tools_from(&self, client: &HootClient) -> Result<Vec<ToolInfo>> {
        match client.request(Payload::ListTools).await? {
            Payload::ToolList { tools } => {
                info!("Discovered {} tools from hootenanny", tools.len());
                Ok(tools)
            }
            Payload::Error { code, message, .. } => {
                anyhow::bail!("ListTools failed: {} - {}", code, message)
            }
            other => {
                anyhow::bail!("Unexpected response to ListTools: {:?}", other)
            }
        }
    }

    /// Remove the upstream connection.
    pub async fn remove_upstream(&mut self, _namespace: &str) -> bool {
        self.client.take().is_some()
    }

    /// Get all available tools.
    pub async fn all_tools(&self) -> Vec<(String, ToolInfo)> {
        self.tools
            .iter()
            .map(|t| (format!("{}.{}", self.namespace, t.name), t.clone()))
            .collect()
    }

    /// Get tools for a specific namespace.
    pub async fn tools_for_namespace(&self, namespace: &str) -> Option<Vec<ToolInfo>> {
        if namespace == self.namespace {
            Some(self.tools.clone())
        } else {
            None
        }
    }

    /// Call a tool on hootenanny.
    #[tracing::instrument(skip(self, arguments), fields(tool = %tool_name))]
    pub async fn call_tool(
        &self,
        namespace: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.call_tool_with_traceparent(namespace, tool_name, arguments, None).await
    }

    /// Call a tool with explicit traceparent.
    #[tracing::instrument(skip(self, arguments, traceparent), fields(tool = %tool_name))]
    pub async fn call_tool_with_traceparent(
        &self,
        namespace: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        traceparent: Option<&str>,
    ) -> Result<serde_json::Value> {
        if namespace != self.namespace {
            anyhow::bail!("Unknown namespace: {}", namespace);
        }

        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected to hootenanny"))?;

        let payload = Payload::ToolCall {
            name: tool_name.to_string(),
            args: arguments,
        };

        // Send request with optional traceparent
        let response = client.request_with_trace(payload, traceparent.map(String::from))
            .await
            .context("Failed to call tool")?;

        match response {
            Payload::Success { result } => Ok(result),
            Payload::Error { code, message, details } => {
                let error_msg = if let Some(d) = details {
                    format!("{}: {}\n{}", code, message, serde_json::to_string_pretty(&d)?)
                } else {
                    format!("{}: {}", code, message)
                };
                anyhow::bail!(error_msg)
            }
            other => {
                anyhow::bail!("Unexpected response: {:?}", other)
            }
        }
    }

    /// Parse a fully qualified tool name into (namespace, tool_name).
    pub fn parse_qualified_name(qualified: &str) -> Option<(&str, &str)> {
        qualified.split_once('.')
    }

    /// Get the list of connected namespaces.
    pub async fn namespaces(&self) -> Vec<String> {
        if self.client.is_some() {
            vec![self.namespace.clone()]
        } else {
            vec![]
        }
    }

    /// Check if a namespace is connected.
    pub async fn has_namespace(&self, namespace: &str) -> bool {
        self.client.is_some() && namespace == self.namespace
    }

    /// Refresh tools (re-discover).
    pub async fn refresh_tools(&mut self, namespace: &str) -> Result<()> {
        if namespace != self.namespace {
            anyhow::bail!("Unknown namespace: {}", namespace);
        }

        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected to hootenanny"))?;

        self.tools = self.discover_tools_from(client).await?;
        Ok(())
    }
}

impl Default for ClientManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_qualified_name() {
        assert_eq!(
            ClientManager::parse_qualified_name("hootenanny.orpheus_generate"),
            Some(("hootenanny", "orpheus_generate"))
        );

        assert_eq!(
            ClientManager::parse_qualified_name("ns.sub.tool"),
            Some(("ns", "sub.tool"))
        );

        assert_eq!(ClientManager::parse_qualified_name("no_namespace"), None);
    }
}
