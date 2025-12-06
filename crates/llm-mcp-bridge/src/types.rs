use baton::schema_helpers::*;
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

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
    pub arguments: String,
}

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

fn default_true() -> bool {
    true
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
    #[schemars(schema_with = "usize_schema")]
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
    #[schemars(schema_with = "optional_usize_schema")]
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
    #[schemars(schema_with = "usize_schema")]
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

/// Response from agent_chat_new
#[derive(Debug, Clone, Serialize)]
pub struct AgentChatNewResponse {
    pub session_id: String,
    pub backend: String,
    pub status: String,
}

/// Output chunk for polling
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputChunk {
    TextDelta {
        delta: String,
        index: usize,
    },
    ToolCallStarted {
        tool_call_id: String,
        name: String,
        index: usize,
    },
    ToolCallCompleted {
        tool_call_id: String,
        result: String,
        is_error: bool,
        index: usize,
    },
    TurnComplete {
        finish_reason: String,
        content: Option<String>,
        index: usize,
    },
    Error {
        message: String,
        index: usize,
    },
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
