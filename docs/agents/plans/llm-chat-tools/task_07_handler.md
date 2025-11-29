# Task 07: Implement MCP handler and integrate with hootenanny

## Goal

Expose the agent_chat_* tools via MCP and integrate with hootenanny.

## Files to Create/Modify

- `crates/llm-mcp-bridge/src/manager.rs`
- `crates/llm-mcp-bridge/src/handler.rs`
- `crates/hootenanny/src/main.rs` (integration)

## Agent Manager (manager.rs)

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use anyhow::Result;

use llmchat::ConversationDb;
use crate::config::{BridgeConfig, BackendConfig};
use crate::mcp_client::McpToolClient;
use crate::provider::OpenAiProvider;
use crate::session::{AgentSession, SessionHandle};
use crate::r#loop::run_agent_loop;
use crate::types::*;

pub struct AgentManager {
    /// Configured backends
    backends: HashMap<String, BackendState>,

    /// Active sessions
    sessions: Arc<RwLock<HashMap<String, SessionHandle>>>,

    /// Background task handles
    handles: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,

    /// MCP client for tool calls
    mcp_client: Arc<McpToolClient>,

    /// Conversation database
    db: Arc<ConversationDb>,
}

struct BackendState {
    config: BackendConfig,
    provider: Arc<OpenAiProvider>,
}

impl AgentManager {
    pub fn new(config: BridgeConfig, db: ConversationDb) -> Result<Self> {
        let mcp_client = Arc::new(McpToolClient::new(&config.mcp_url));

        let mut backends = HashMap::new();
        for backend_config in config.backends {
            let provider = OpenAiProvider::new(&backend_config)?;
            backends.insert(
                backend_config.id.clone(),
                BackendState {
                    config: backend_config,
                    provider: Arc::new(provider),
                },
            );
        }

        Ok(Self {
            backends,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(RwLock::new(HashMap::new())),
            mcp_client,
            db: Arc::new(db),
        })
    }

    /// List configured backends
    pub fn list_backends(&self) -> Vec<&BackendConfig> {
        self.backends.values().map(|s| &s.config).collect()
    }

    /// Create a new agent session
    pub async fn create_session(&self, request: AgentChatNewRequest) -> Result<AgentChatNewResponse> {
        let backend = self.backends.get(&request.backend)
            .ok_or_else(|| anyhow::anyhow!("Unknown backend: {}", request.backend))?;

        // Create conversation in database
        let conv = self.db.create_conversation(
            &request.backend,
            Some(&backend.config.default_model),
            request.system_prompt.as_deref(),
        )?;

        // Add system prompt as first message if provided
        if let Some(prompt) = &request.system_prompt {
            self.db.append_message(&conv.id.0, llmchat::Role::System, Some(prompt))?;
        }

        let session = AgentSession::new(
            &request.backend,
            &conv.id.0,
            request.enable_tools,
            request.max_tool_iterations.unwrap_or(5),
        );

        let session_id = session.id.0.clone();
        let handle: SessionHandle = Arc::new(RwLock::new(session));

        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), handle);
        }

        Ok(AgentChatNewResponse {
            session_id,
            backend: request.backend,
            status: "idle".to_string(),
        })
    }

    /// Send a message to a session (starts async agent loop)
    pub async fn send_message(&self, request: AgentChatSendRequest) -> Result<()> {
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(&request.session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", request.session_id))?
        };

        // Get backend provider
        let backend_id = {
            let s = session.read().await;

            // Check session is idle
            if s.status != SessionStatus::Idle {
                anyhow::bail!("Session is not idle, current status: {:?}", s.status);
            }

            s.backend_id.clone()
        };

        let backend = self.backends.get(&backend_id)
            .ok_or_else(|| anyhow::anyhow!("Backend not found: {}", backend_id))?;

        // Update temperature if provided
        if let Some(temp) = request.temperature {
            let mut s = session.write().await;
            s.temperature = Some(temp);
        }

        // Spawn agent loop
        let session_clone = session.clone();
        let provider = backend.provider.clone();
        let mcp_client = self.mcp_client.clone();
        let db = self.db.clone();
        let message = request.message.clone();
        let session_id = request.session_id.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = run_agent_loop(
                session_clone.clone(),
                provider,
                mcp_client,
                db,
                &message,
            ).await {
                let mut s = session_clone.write().await;
                s.set_failed(&e.to_string());
            }
        });

        // Store handle for potential cancellation
        {
            let mut handles = self.handles.write().await;
            handles.insert(session_id, handle);
        }

        Ok(())
    }

    /// Poll for output chunks
    pub async fn poll(&self, request: AgentChatPollRequest) -> Result<AgentChatPollResponse> {
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(&request.session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", request.session_id))?
        };

        // Wait for new chunks or timeout
        let timeout = std::time::Duration::from_millis(request.timeout_ms.min(30000));
        let start = std::time::Instant::now();

        loop {
            let (chunks, next_index, status) = {
                let s = session.read().await;
                let chunks = s.chunks_since(request.since_index).to_vec();
                let next_index = s.chunk_count();
                let status = s.status;
                (chunks, next_index, status)
            };

            // Return if we have new chunks or session is done
            if !chunks.is_empty() || status == SessionStatus::Finished
                || status == SessionStatus::Failed || status == SessionStatus::Cancelled {
                return Ok(AgentChatPollResponse {
                    session_id: request.session_id,
                    chunks,
                    next_index,
                    status,
                });
            }

            // Check timeout
            if start.elapsed() >= timeout {
                return Ok(AgentChatPollResponse {
                    session_id: request.session_id,
                    chunks: vec![],
                    next_index,
                    status,
                });
            }

            // Short sleep before checking again
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Cancel a session
    pub async fn cancel(&self, request: AgentChatCancelRequest) -> Result<()> {
        // Abort the task
        {
            let mut handles = self.handles.write().await;
            if let Some(handle) = handles.remove(&request.session_id) {
                handle.abort();
            }
        }

        // Update session status
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(&request.session_id) {
                let mut s = session.write().await;
                s.set_cancelled();
            }
        }

        Ok(())
    }

    /// Get session status
    pub async fn get_status(&self, request: AgentChatStatusRequest) -> Result<SessionStatusResponse> {
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(&request.session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", request.session_id))?
        };

        let s = session.read().await;
        let messages = self.db.get_messages(&s.conversation_id)?;

        Ok(SessionStatusResponse {
            session_id: request.session_id,
            backend: s.backend_id.clone(),
            status: s.status,
            message_count: messages.len(),
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        })
    }

    /// Get session history
    pub async fn get_history(&self, request: AgentChatHistoryRequest) -> Result<Vec<llmchat::Message>> {
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(&request.session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", request.session_id))?
        };

        let conv_id = {
            let s = session.read().await;
            s.conversation_id.clone()
        };

        let mut messages = self.db.get_messages(&conv_id)?;

        if let Some(limit) = request.limit {
            messages.truncate(limit);
        }

        Ok(messages)
    }

    /// Generate conversation summary
    pub async fn summarize(&self, request: AgentChatSummaryRequest) -> Result<String> {
        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(&request.session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", request.session_id))?
        };

        let (conv_id, backend_id) = {
            let s = session.read().await;
            (s.conversation_id.clone(), s.backend_id.clone())
        };

        let backend = self.backends.get(&backend_id)
            .ok_or_else(|| anyhow::anyhow!("Backend not found: {}", backend_id))?;

        let db_messages = self.db.get_messages(&conv_id)?;
        let messages = OpenAiProvider::convert_messages(
            &db_messages.iter().map(|m| crate::types::ChatMessage {
                role: m.role.as_str().to_string(),
                content: m.content.clone(),
                tool_calls: None,
                tool_call_id: None,
            }).collect::<Vec<_>>()
        )?;

        backend.provider.summarize(messages).await
    }

    /// List sessions
    pub async fn list_sessions(&self, request: AgentChatListRequest) -> Result<Vec<SessionInfo>> {
        let sessions = self.sessions.read().await;

        let mut infos: Vec<SessionInfo> = Vec::new();

        for (id, handle) in sessions.iter() {
            let s = handle.read().await;

            if request.active_only && (s.status == SessionStatus::Finished
                || s.status == SessionStatus::Failed
                || s.status == SessionStatus::Cancelled) {
                continue;
            }

            infos.push(SessionInfo {
                session_id: id.clone(),
                backend: s.backend_id.clone(),
                status: s.status,
                created_at: s.created_at.to_rfc3339(),
            });

            if infos.len() >= request.limit {
                break;
            }
        }

        // Sort by created_at descending
        infos.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(infos)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStatusResponse {
    pub session_id: String,
    pub backend: String,
    pub status: SessionStatus,
    pub message_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub backend: String,
    pub status: SessionStatus,
    pub created_at: String,
}
```

## Handler (handler.rs)

```rust
use std::sync::Arc;
use async_trait::async_trait;
use baton::{Handler, Tool, CallToolResult, Content, ErrorData, Implementation};
use serde_json::Value;

use crate::manager::AgentManager;
use crate::types::*;

fn schema_for<T: schemars::JsonSchema>() -> baton::ToolSchema {
    let settings = schemars::generate::SchemaSettings::draft07().with(|s| {
        s.inline_subschemas = true;
    });
    let gen = settings.into_generator();
    let schema = gen.into_root_schema_for::<T>();
    let value = serde_json::to_value(&schema).unwrap_or_default();
    baton::ToolSchema::from_value(value)
}

pub struct AgentChatHandler {
    manager: Arc<AgentManager>,
}

impl AgentChatHandler {
    pub fn new(manager: Arc<AgentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Handler for AgentChatHandler {
    fn tools(&self) -> Vec<Tool> {
        vec![
            Tool::new("agent_chat_new", "Create a new agent chat session")
                .with_input_schema(schema_for::<AgentChatNewRequest>()),
            Tool::new("agent_chat_send", "Send a message to an agent session")
                .with_input_schema(schema_for::<AgentChatSendRequest>()),
            Tool::new("agent_chat_poll", "Poll for agent output chunks")
                .with_input_schema(schema_for::<AgentChatPollRequest>())
                .read_only(),
            Tool::new("agent_chat_cancel", "Cancel a running agent session")
                .with_input_schema(schema_for::<AgentChatCancelRequest>()),
            Tool::new("agent_chat_status", "Get status of an agent session")
                .with_input_schema(schema_for::<AgentChatStatusRequest>())
                .read_only(),
            Tool::new("agent_chat_history", "Get message history for a session")
                .with_input_schema(schema_for::<AgentChatHistoryRequest>())
                .read_only(),
            Tool::new("agent_chat_summary", "Get AI-generated summary of conversation")
                .with_input_schema(schema_for::<AgentChatSummaryRequest>())
                .read_only(),
            Tool::new("agent_chat_list", "List agent sessions")
                .with_input_schema(schema_for::<AgentChatListRequest>())
                .read_only(),
            Tool::new("agent_chat_backends", "List available LLM backends")
                .read_only(),
        ]
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<CallToolResult, ErrorData> {
        match name {
            "agent_chat_new" => {
                let request: AgentChatNewRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.create_session(request).await {
                    Ok(response) => Ok(CallToolResult::text(
                        serde_json::to_string_pretty(&response).unwrap()
                    )),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_send" => {
                let request: AgentChatSendRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.send_message(request).await {
                    Ok(()) => Ok(CallToolResult::text(r#"{"status": "started"}"#)),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_poll" => {
                let request: AgentChatPollRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.poll(request).await {
                    Ok(response) => Ok(CallToolResult::text(
                        serde_json::to_string_pretty(&response).unwrap()
                    )),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_cancel" => {
                let request: AgentChatCancelRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.cancel(request).await {
                    Ok(()) => Ok(CallToolResult::text(r#"{"status": "cancelled"}"#)),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_status" => {
                let request: AgentChatStatusRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.get_status(request).await {
                    Ok(response) => Ok(CallToolResult::text(
                        serde_json::to_string_pretty(&response).unwrap()
                    )),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_history" => {
                let request: AgentChatHistoryRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.get_history(request).await {
                    Ok(messages) => Ok(CallToolResult::text(
                        serde_json::to_string_pretty(&messages).unwrap()
                    )),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_summary" => {
                let request: AgentChatSummaryRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.summarize(request).await {
                    Ok(summary) => Ok(CallToolResult::text(summary)),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_list" => {
                let request: AgentChatListRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.list_sessions(request).await {
                    Ok(sessions) => Ok(CallToolResult::text(
                        serde_json::to_string_pretty(&sessions).unwrap()
                    )),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_backends" => {
                let backends: Vec<_> = self.manager.list_backends()
                    .iter()
                    .map(|b| serde_json::json!({
                        "id": b.id,
                        "display_name": b.display_name,
                        "default_model": b.default_model,
                        "supports_tools": b.supports_tools,
                    }))
                    .collect();

                Ok(CallToolResult::text(
                    serde_json::to_string_pretty(&backends).unwrap()
                ))
            }

            _ => Err(ErrorData::tool_not_found(name)),
        }
    }

    fn server_info(&self) -> Implementation {
        Implementation::new("llm-mcp-bridge", env!("CARGO_PKG_VERSION"))
    }
}
```

## Hootenanny Integration

Two options for integration:

### Option A: Compose handlers
Create a composite handler that delegates to both HootHandler and AgentChatHandler.

### Option B: Add tools to existing handler
Add agent_chat_* tools directly to HootHandler.

Recommend **Option A** for cleaner separation.

## Acceptance Criteria

- [ ] AgentManager manages backends and sessions
- [ ] All 9 agent_chat_* tools registered and working
- [ ] Sessions persist across polls
- [ ] Cancellation aborts running tasks
- [ ] Backend list shows configured backends
- [ ] Summary uses backend's summary_model
- [ ] Integration with hootenanny works
