# Task 03: Create llm-mcp-bridge crate scaffold

## Goal

Set up the bridge crate with configuration and request/response types.

## Files to Create

- `crates/llm-mcp-bridge/Cargo.toml`
- `crates/llm-mcp-bridge/src/lib.rs`
- `crates/llm-mcp-bridge/src/config.rs`
- `crates/llm-mcp-bridge/src/types.rs`

## Cargo.toml

```toml
[package]
name = "llm-mcp-bridge"
version = "0.1.0"
edition = "2021"

[dependencies]
# OpenAI API
async-openai = "0.28"

# MCP integration
baton = { path = "../baton" }
llmchat = { path = "../llmchat" }

# Async runtime
tokio = { version = "1", features = ["sync", "time", "rt"] }
async-trait = "0.1"
futures = "0.3"

# HTTP client for MCP calls
reqwest = { version = "0.12", features = ["json"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1"

# Error handling
anyhow = "1"
thiserror = "1"

# Utilities
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }

# Tracing
tracing = "0.1"
opentelemetry = "0.22"
tracing-opentelemetry = "0.23"

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
wiremock = "0.6"
```

## Configuration (config.rs)

```rust
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
```

## Types (types.rs)

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Session identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Chat message in OpenAI format
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChatMessage {
    /// Role: "system", "user", "assistant", "tool"
    pub role: String,

    /// Message content (may be None for tool_calls-only messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// Tool calls from assistant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,

    /// Tool call ID this message is responding to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Tool call in a chat message
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ChatFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,  // JSON string
}

// ============ MCP Tool Requests ============

/// Request to create a new agent session
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AgentChatNewRequest {
    /// Backend ID to use (e.g., "deepseek")
    #[schemars(description = "Backend ID (e.g., 'deepseek', 'ollama')")]
    pub backend: String,

    /// Optional system prompt
    #[schemars(description = "System prompt for the conversation")]
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Enable MCP tool calling
    #[schemars(description = "Allow the agent to call MCP tools")]
    #[serde(default = "default_true")]
    pub enable_tools: bool,

    /// Maximum tool call iterations per turn
    #[schemars(description = "Max tool iterations per turn (default: 5)")]
    #[serde(default)]
    pub max_tool_iterations: Option<u32>,
}

/// Request to send a message to an agent session
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AgentChatSendRequest {
    /// Session ID
    #[schemars(description = "Session ID from agent_chat_new")]
    pub session_id: String,

    /// Message content
    #[schemars(description = "User message to send")]
    pub message: String,

    /// Override temperature for this turn
    #[schemars(description = "Sampling temperature (0.0-2.0)")]
    #[serde(default)]
    pub temperature: Option<f32>,
}

/// Request to poll for agent output
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AgentChatPollRequest {
    /// Session ID
    #[schemars(description = "Session ID")]
    pub session_id: String,

    /// Index to start from (for incremental polling)
    #[schemars(description = "Output index to poll from (default: 0)")]
    #[serde(default)]
    pub since_index: usize,

    /// Timeout in milliseconds
    #[schemars(description = "Timeout in ms (default: 5000, max: 30000)")]
    #[serde(default = "default_poll_timeout")]
    pub timeout_ms: u64,
}

fn default_poll_timeout() -> u64 {
    5000
}

/// Request to cancel a session
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AgentChatCancelRequest {
    /// Session ID
    #[schemars(description = "Session ID to cancel")]
    pub session_id: String,
}

/// Request to get session status
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AgentChatStatusRequest {
    /// Session ID
    #[schemars(description = "Session ID")]
    pub session_id: String,
}

/// Request to get session history
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AgentChatHistoryRequest {
    /// Session ID
    #[schemars(description = "Session ID")]
    pub session_id: String,

    /// Maximum messages to return
    #[schemars(description = "Max messages (default: all)")]
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Request for conversation summary
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AgentChatSummaryRequest {
    /// Session ID
    #[schemars(description = "Session ID")]
    pub session_id: String,
}

/// Request to list sessions
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AgentChatListRequest {
    /// Max sessions to return
    #[schemars(description = "Max sessions (default: 20)")]
    #[serde(default = "default_list_limit")]
    pub limit: usize,

    /// Only show active sessions
    #[schemars(description = "Only show active (non-finished) sessions")]
    #[serde(default)]
    pub active_only: bool,
}

fn default_list_limit() -> usize {
    20
}

// ============ Responses ============

/// Response from agent_chat_new
#[derive(Debug, Clone, Serialize)]
pub struct AgentChatNewResponse {
    pub session_id: String,
    pub backend: String,
    pub status: String,
}

/// Output chunk for polling
#[derive(Debug, Clone, Serialize)]
pub enum OutputChunk {
    TextDelta { delta: String, index: usize },
    ToolCallStarted { tool_call_id: String, name: String, index: usize },
    ToolCallCompleted { tool_call_id: String, result: String, is_error: bool, index: usize },
    TurnComplete { finish_reason: String, content: Option<String>, index: usize },
    Error { message: String, index: usize },
}

/// Response from agent_chat_poll
#[derive(Debug, Clone, Serialize)]
pub struct AgentChatPollResponse {
    pub session_id: String,
    pub chunks: Vec<OutputChunk>,
    pub next_index: usize,
    pub status: SessionStatus,
}

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Idle,
    Generating,
    ExecutingTools,
    Finished,
    Failed,
    Cancelled,
}

fn default_true() -> bool {
    true
}
```

## lib.rs

```rust
pub mod config;
pub mod types;

// Will be added in later tasks:
// pub mod provider;
// pub mod mcp_client;
// pub mod session;
// pub mod manager;
// pub mod handler;

pub use config::{BackendConfig, BridgeConfig};
pub use types::*;
```

## Acceptance Criteria

- [ ] Crate compiles and is added to workspace Cargo.toml
- [ ] All request types have JsonSchema derives for MCP tool schemas
- [ ] Configuration supports multiple backends
- [ ] Types match OpenAI chat completion format
- [ ] OutputChunk enum covers all streaming scenarios
- [ ] SessionStatus covers all session states
