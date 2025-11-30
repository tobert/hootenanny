use serde::{Deserialize, Serialize};

/// Configuration for the LLM-MCP bridge
#[derive(Debug, Clone, Deserialize)]
pub struct BridgeConfig {
    /// URL of the MCP server (hootenanny) for tool calls
    pub mcp_url: String,

    /// Configured LLM backends
    pub backends: Vec<BackendConfig>,
}

/// Configuration for a single LLM backend
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackendConfig {
    /// Unique identifier (e.g., "deepseek", "ollama")
    pub id: String,

    /// Human-readable name for tool descriptions
    pub display_name: String,

    /// Base URL for the OpenAI-compatible API
    pub base_url: String,

    /// API key (optional for local models)
    #[serde(default)]
    pub api_key: Option<String>,

    /// Default model to use
    pub default_model: String,

    /// Model to use for quick summaries (can be same or smaller)
    #[serde(default)]
    pub summary_model: Option<String>,

    /// Whether this backend supports tool/function calling
    #[serde(default = "default_true")]
    pub supports_tools: bool,

    /// Maximum tokens for responses
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Default temperature
    #[serde(default)]
    pub default_temperature: Option<f32>,
}

impl BackendConfig {
    /// Get the model to use for summaries
    pub fn summary_model(&self) -> &str {
        self.summary_model.as_deref().unwrap_or(&self.default_model)
    }
}

fn default_true() -> bool {
    true
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            mcp_url: "http://127.0.0.1:8080".to_string(),
            backends: vec![],
        }
    }
}
