//! Client manager for upstream MCP servers.
//!
//! Manages connections to multiple MCP servers with namespace mapping.

use anyhow::{Context, Result};
use baton::client::{ClientOptions, McpClient, ToolInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for an upstream MCP server.
#[derive(Debug, Clone)]
pub struct UpstreamConfig {
    /// Namespace prefix for tools (e.g., "hootenanny" -> mcp.hootenanny.*)
    pub namespace: String,

    /// URL of the MCP server
    pub url: String,
}

/// Cached tool information for an upstream server.
pub struct CachedUpstream {
    /// The MCP client
    pub client: Arc<McpClient>,

    /// Namespace for this upstream
    pub namespace: String,

    /// Discovered tools
    pub tools: Vec<ToolInfo>,
}

/// Manages connections to multiple upstream MCP servers.
pub struct ClientManager {
    /// Upstreams indexed by namespace
    upstreams: RwLock<HashMap<String, CachedUpstream>>,
}

impl ClientManager {
    /// Create a new empty client manager.
    pub fn new() -> Self {
        Self {
            upstreams: RwLock::new(HashMap::new()),
        }
    }

    /// Add an upstream MCP server.
    ///
    /// This will connect to the server, initialize the session, and discover tools.
    #[tracing::instrument(skip(self), fields(namespace = %config.namespace, url = %config.url))]
    pub async fn add_upstream(&self, config: UpstreamConfig) -> Result<()> {
        tracing::info!("Connecting to upstream MCP server");

        let options = ClientOptions::with_name("luanette", env!("CARGO_PKG_VERSION"));
        let client = McpClient::with_options(&config.url, options);

        // Initialize the connection
        client
            .initialize()
            .await
            .context("Failed to initialize upstream MCP session")?;

        // Discover available tools
        let tools = client
            .list_tools()
            .await
            .context("Failed to list tools from upstream")?;

        tracing::info!(
            tool_count = tools.len(),
            "Discovered tools from upstream"
        );

        let cached = CachedUpstream {
            client: Arc::new(client),
            namespace: config.namespace.clone(),
            tools,
        };

        self.upstreams.write().await.insert(config.namespace, cached);

        Ok(())
    }

    /// Remove an upstream by namespace.
    pub async fn remove_upstream(&self, namespace: &str) -> bool {
        self.upstreams.write().await.remove(namespace).is_some()
    }

    /// Get all available tools across all upstreams.
    ///
    /// Returns tools with namespaced names (e.g., "hootenanny.orpheus_generate").
    pub async fn all_tools(&self) -> Vec<(String, ToolInfo)> {
        let upstreams = self.upstreams.read().await;
        let mut result = Vec::new();

        for (namespace, cached) in upstreams.iter() {
            for tool in &cached.tools {
                let namespaced_name = format!("{}.{}", namespace, tool.name);
                result.push((namespaced_name, tool.clone()));
            }
        }

        result
    }

    /// Get tools for a specific namespace.
    pub async fn tools_for_namespace(&self, namespace: &str) -> Option<Vec<ToolInfo>> {
        let upstreams = self.upstreams.read().await;
        upstreams.get(namespace).map(|c| c.tools.clone())
    }

    /// Call a tool on an upstream server.
    ///
    /// The tool name can be either:
    /// - Fully qualified: "hootenanny.orpheus_generate"
    /// - Namespace + tool: namespace="hootenanny", name="orpheus_generate"
    #[tracing::instrument(skip(self, arguments), fields(tool = %tool_name))]
    pub async fn call_tool(
        &self,
        namespace: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let upstreams = self.upstreams.read().await;

        let cached = upstreams
            .get(namespace)
            .ok_or_else(|| anyhow::anyhow!("Unknown namespace: {}", namespace))?;

        // Verify the tool exists
        if !cached.tools.iter().any(|t| t.name == tool_name) {
            anyhow::bail!(
                "Tool '{}' not found in namespace '{}'. Available: {:?}",
                tool_name,
                namespace,
                cached.tools.iter().map(|t| &t.name).collect::<Vec<_>>()
            );
        }

        cached.client.call_tool(tool_name, arguments)
            .await
            .context("Failed to call upstream tool")
    }

    /// Parse a fully qualified tool name into (namespace, tool_name).
    ///
    /// Example: "hootenanny.orpheus_generate" -> ("hootenanny", "orpheus_generate")
    pub fn parse_qualified_name(qualified: &str) -> Option<(&str, &str)> {
        qualified.split_once('.')
    }

    /// Get the list of connected namespaces.
    pub async fn namespaces(&self) -> Vec<String> {
        self.upstreams.read().await.keys().cloned().collect()
    }

    /// Check if a namespace is connected.
    pub async fn has_namespace(&self, namespace: &str) -> bool {
        self.upstreams.read().await.contains_key(namespace)
    }

    /// Get the URL for a namespace.
    pub async fn url_for_namespace(&self, namespace: &str) -> Option<String> {
        self.upstreams
            .read()
            .await
            .get(namespace)
            .map(|c| c.client.base_url().to_string())
    }

    /// Refresh tools for a namespace (re-discover).
    pub async fn refresh_tools(&self, namespace: &str) -> Result<()> {
        let mut upstreams = self.upstreams.write().await;

        let cached = upstreams
            .get_mut(namespace)
            .ok_or_else(|| anyhow::anyhow!("Unknown namespace: {}", namespace))?;

        let tools = cached
            .client
            .list_tools()
            .await
            .context("Failed to refresh tools")?;

        tracing::info!(
            namespace = %namespace,
            tool_count = tools.len(),
            "Refreshed tools"
        );

        cached.tools = tools;

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
