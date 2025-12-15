//! ZMQ client for upstream hootenanny server.
//!
//! Connects directly to hootenanny via ZMQ DEALER socket using hooteproto.

use anyhow::{Context, Result};
use hooteproto::{Envelope, Payload, ToolInfo};
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

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

/// ZMQ client for hootenanny.
struct HootClient {
    socket: RwLock<DealerSocket>,
    timeout: Duration,
    tools: Vec<ToolInfo>,
}

impl HootClient {
    async fn connect(endpoint: &str, timeout_ms: u64) -> Result<Self> {
        debug!("Creating DEALER socket for hootenanny");
        let mut socket = DealerSocket::new();

        // Wrap in timeout because zeromq-rs connect() can block indefinitely
        tokio::time::timeout(Duration::from_secs(5), socket.connect(endpoint))
            .await
            .with_context(|| format!("Timeout connecting to hootenanny at {}", endpoint))?
            .with_context(|| format!("Failed to connect to hootenanny at {}", endpoint))?;

        info!("Connected to hootenanny at {}", endpoint);

        Ok(Self {
            socket: RwLock::new(socket),
            timeout: Duration::from_millis(timeout_ms),
            tools: Vec::new(),
        })
    }

    async fn request(&self, payload: Payload) -> Result<Payload> {
        let envelope = Envelope::new(payload);
        let bytes = rmp_serde::to_vec(&envelope)?;

        debug!("Sending {} bytes to hootenanny", bytes.len());

        let mut socket = self.socket.write().await;

        // Send
        let msg = ZmqMessage::from(bytes);
        tokio::time::timeout(self.timeout, socket.send(msg))
            .await
            .context("Send timeout")?
            .context("Failed to send")?;

        // Receive
        let response = tokio::time::timeout(self.timeout, socket.recv())
            .await
            .context("Receive timeout")?
            .context("Failed to receive")?;

        let response_bytes = response.get(0).context("Empty response")?;
        let response_envelope: Envelope = rmp_serde::from_slice(response_bytes)
            .context("Failed to deserialize response")?;

        Ok(response_envelope.payload)
    }

    async fn discover_tools(&mut self) -> Result<()> {
        match self.request(Payload::ListTools).await? {
            Payload::ToolList { tools } => {
                info!("Discovered {} tools from hootenanny", tools.len());
                self.tools = tools;
                Ok(())
            }
            Payload::Error { code, message, .. } => {
                anyhow::bail!("ListTools failed: {} - {}", code, message)
            }
            other => {
                anyhow::bail!("Unexpected response to ListTools: {:?}", other)
            }
        }
    }
}

/// Manages the connection to upstream hootenanny.
pub struct ClientManager {
    client: Option<HootClient>,
    namespace: String,
}

impl ClientManager {
    /// Create a new empty client manager.
    pub fn new() -> Self {
        Self {
            client: None,
            namespace: String::new(),
        }
    }

    /// Connect to upstream hootenanny via ZMQ.
    #[tracing::instrument(skip(self), fields(namespace = %config.namespace, endpoint = %config.endpoint))]
    pub async fn add_upstream(&mut self, config: UpstreamConfig) -> Result<()> {
        info!("Connecting to upstream hootenanny");

        let mut client = HootClient::connect(&config.endpoint, config.timeout_ms).await?;
        client.discover_tools().await?;

        self.namespace = config.namespace;
        self.client = Some(client);

        Ok(())
    }

    /// Remove the upstream connection.
    pub async fn remove_upstream(&mut self, _namespace: &str) -> bool {
        self.client.take().is_some()
    }

    /// Get all available tools.
    pub async fn all_tools(&self) -> Vec<(String, ToolInfo)> {
        match &self.client {
            Some(client) => {
                client.tools.iter()
                    .map(|t| (format!("{}.{}", self.namespace, t.name), t.clone()))
                    .collect()
            }
            None => Vec::new(),
        }
    }

    /// Get tools for a specific namespace.
    pub async fn tools_for_namespace(&self, namespace: &str) -> Option<Vec<ToolInfo>> {
        if namespace == self.namespace {
            self.client.as_ref().map(|c| c.tools.clone())
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
    #[tracing::instrument(skip(self, arguments, _traceparent), fields(tool = %tool_name))]
    pub async fn call_tool_with_traceparent(
        &self,
        namespace: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        _traceparent: Option<&str>,
    ) -> Result<serde_json::Value> {
        if namespace != self.namespace {
            anyhow::bail!("Unknown namespace: {}", namespace);
        }

        let client = self.client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not connected to hootenanny"))?;

        // Convert tool name + args to Payload using shared function
        let payload = hooteproto::tool_to_payload(tool_name, &arguments)
            .with_context(|| format!("Failed to convert tool '{}' to payload", tool_name))?;

        // Send request
        let response = client.request(payload).await?;

        // Handle response
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

        if let Some(ref mut client) = self.client {
            client.discover_tools().await
        } else {
            anyhow::bail!("Not connected to hootenanny")
        }
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
