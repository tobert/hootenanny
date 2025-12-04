//! MCP Protocol Types
//!
//! Types for the MCP initialization handshake and capability negotiation.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The current MCP protocol version.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Server or client implementation info.
/// Per MCP 2025-06-18 schema lines 800-820.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Implementation {
    /// Programmatic name of the implementation.
    pub name: String,

    /// Version string.
    pub version: String,

    /// Human-readable title (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl Implementation {
    /// Create a new implementation info.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            title: None,
        }
    }

    /// Set the human-readable title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Initialize request params from client.
/// Per MCP 2025-06-18 schema lines 821-854.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Protocol version the client supports.
    pub protocol_version: String,

    /// Client capabilities.
    pub capabilities: ClientCapabilities,

    /// Client implementation info.
    pub client_info: Implementation,
}

/// Initialize result from server.
/// Per MCP 2025-06-18 schema lines 855-884.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Protocol version the server wants to use.
    pub protocol_version: String,

    /// Server capabilities.
    pub capabilities: ServerCapabilities,

    /// Server implementation info.
    pub server_info: Implementation,

    /// Optional instructions for the LLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

impl InitializeResult {
    /// Create a new initialize result.
    pub fn new(server_info: Implementation, capabilities: ServerCapabilities) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities,
            server_info,
            instructions: None,
        }
    }

    /// Set instructions for the LLM.
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }
}

/// Client capabilities.
/// Per MCP 2025-06-18 schema lines 215-251.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    /// Client supports sampling requests from server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingCapability>,

    /// Client supports elicitation requests from server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elicitation: Option<ElicitationCapability>,

    /// Client supports roots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,

    /// Experimental capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,
}

/// Sampling capability marker.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SamplingCapability {}

/// Elicitation capability marker.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ElicitationCapability {}

/// Roots capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    /// Client supports roots list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Server capabilities.
/// Per MCP 2025-06-18 schema lines 2077-2137.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Server offers tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,

    /// Server offers resources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,

    /// Server offers prompts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,

    /// Server supports logging.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingCapability>,

    /// Server supports completions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completions: Option<CompletionsCapability>,

    /// Experimental capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Value>,
}

impl ServerCapabilities {
    /// Create capabilities with tools enabled.
    pub fn with_tools() -> Self {
        Self {
            tools: Some(ToolsCapability::default()),
            ..Default::default()
        }
    }

    /// Enable tools.
    pub fn enable_tools(mut self) -> Self {
        self.tools = Some(ToolsCapability::default());
        self
    }

    /// Enable resources.
    pub fn enable_resources(mut self) -> Self {
        self.resources = Some(ResourcesCapability::default());
        self
    }

    /// Enable prompts.
    pub fn enable_prompts(mut self) -> Self {
        self.prompts = Some(PromptsCapability::default());
        self
    }

    /// Enable logging.
    pub fn enable_logging(mut self) -> Self {
        self.logging = Some(LoggingCapability::default());
        self
    }

    /// Enable completions.
    pub fn enable_completions(mut self) -> Self {
        self.completions = Some(CompletionsCapability::default());
        self
    }
}

/// Tools capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    /// Server supports tool list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Resources capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    /// Server supports resource list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,

    /// Server supports resource subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
}

/// Prompts capability.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsCapability {
    /// Server supports prompt list changed notifications.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Logging capability marker.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingCapability {}

/// Completions capability marker.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompletionsCapability {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_implementation() {
        let impl_info = Implementation::new("test-server", "1.0.0")
            .with_title("Test Server");

        let json = serde_json::to_value(&impl_info).unwrap();
        assert_eq!(json["name"], "test-server");
        assert_eq!(json["version"], "1.0.0");
        assert_eq!(json["title"], "Test Server");
    }

    #[test]
    fn test_server_capabilities() {
        let caps = ServerCapabilities::default()
            .enable_tools()
            .enable_resources();

        let json = serde_json::to_value(&caps).unwrap();
        assert!(json["tools"].is_object());
        assert!(json["resources"].is_object());
        assert!(json.get("prompts").is_none());
    }

    #[test]
    fn test_initialize_result() {
        let result = InitializeResult::new(
            Implementation::new("baton", "0.1.0"),
            ServerCapabilities::with_tools(),
        )
        .with_instructions("This is a test server.");

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(json["serverInfo"]["name"], "baton");
        assert!(json["capabilities"]["tools"].is_object());
        assert_eq!(json["instructions"], "This is a test server.");
    }

    #[test]
    fn test_initialize_params_roundtrip() {
        let params = InitializeParams {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation::new("test-client", "1.0.0"),
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: InitializeParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.protocol_version, PROTOCOL_VERSION);
        assert_eq!(parsed.client_info.name, "test-client");
    }
}
