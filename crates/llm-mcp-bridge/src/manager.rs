use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use llmchat::ConversationDb;

use crate::agent_loop::run_agent_loop;
use crate::config::{BackendConfig, BridgeConfig};
use crate::mcp_client::McpToolClient;
use crate::provider::OpenAiProvider;
use crate::session::{AgentSession, SessionHandle};
use crate::types::*;

struct BackendState {
    config: BackendConfig,
    provider: Arc<OpenAiProvider>,
}

pub struct AgentManager {
    backends: HashMap<String, BackendState>,
    sessions: Arc<RwLock<HashMap<String, SessionHandle>>>,
    handles: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
    mcp_client: Arc<McpToolClient>,
    mcp_initialized: Arc<RwLock<bool>>,
    db: Arc<ConversationDb>,
}

impl AgentManager {
    /// Create a new agent manager.
    ///
    /// Note: MCP client initialization is deferred until first use to avoid
    /// circular dependencies when running inside hootenanny.
    pub fn new(config: BridgeConfig, db: ConversationDb) -> Result<Self> {
        let mcp_client = McpToolClient::new(&config.mcp_url);

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
            mcp_client: Arc::new(mcp_client),
            mcp_initialized: Arc::new(RwLock::new(false)),
            db: Arc::new(db),
        })
    }

    /// Ensure the MCP client is initialized.
    ///
    /// Called lazily before first MCP operation to avoid circular dependencies
    /// when running inside hootenanny (which would try to connect to itself).
    ///
    /// TODO: Revisit this design - consider whether the client should support
    /// explicit lazy initialization, or if hootenanny's architecture should change.
    async fn ensure_mcp_initialized(&self) -> Result<()> {
        let mut initialized = self.mcp_initialized.write().await;
        if *initialized {
            return Ok(());
        }

        self.mcp_client
            .initialize()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize MCP client: {}", e))?;
        *initialized = true;
        Ok(())
    }

    /// Access the database for resource queries
    pub fn db(&self) -> &ConversationDb {
        &self.db
    }

    /// List configured backends
    pub fn list_backends(&self) -> Vec<&BackendConfig> {
        self.backends.values().map(|s| &s.config).collect()
    }

    /// Create a new agent session
    pub async fn create_session(&self, request: AgentChatNewRequest) -> Result<AgentChatNewResponse> {
        let backend = self
            .backends
            .get(&request.backend)
            .ok_or_else(|| anyhow::anyhow!("Unknown backend: {}", request.backend))?;

        let conv = self.db.create_conversation(
            &request.backend,
            Some(&backend.config.default_model),
            request.system_prompt.as_deref(),
        )?;

        if let Some(prompt) = &request.system_prompt {
            self.db
                .append_message(&conv.id.0, llmchat::Role::System, Some(prompt))?;
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
        // Ensure MCP client is initialized before first use
        self.ensure_mcp_initialized().await?;

        let session = {
            let sessions = self.sessions.read().await;
            sessions
                .get(&request.session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", request.session_id))?
        };

        let backend_id = {
            let s = session.read().await;

            if s.status != SessionStatus::Idle {
                anyhow::bail!("Session is not idle, current status: {:?}", s.status);
            }

            s.backend_id.clone()
        };

        let backend = self
            .backends
            .get(&backend_id)
            .ok_or_else(|| anyhow::anyhow!("Backend not found: {}", backend_id))?;

        if let Some(temp) = request.temperature {
            let mut s = session.write().await;
            s.temperature = Some(temp);
        }

        let session_clone = session.clone();
        let provider = backend.provider.clone();
        let mcp_client = self.mcp_client.clone();
        let db = self.db.clone();
        let message = request.message.clone();
        let session_id = request.session_id.clone();

        let handle = tokio::spawn(async move {
            let result = run_agent_loop(session_clone.clone(), provider, mcp_client, db, &message)
                .await;

            match result {
                Ok(()) => {
                    tracing::info!("Agent loop completed successfully");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Agent loop failed");
                    let mut s = session_clone.write().await;
                    s.set_failed(&e.to_string());
                }
            }
        });

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
            sessions
                .get(&request.session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", request.session_id))?
        };

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

            if !chunks.is_empty()
                || status == SessionStatus::Finished
                || status == SessionStatus::Failed
                || status == SessionStatus::Cancelled
            {
                return Ok(AgentChatPollResponse {
                    session_id: request.session_id,
                    chunks,
                    next_index,
                    status,
                });
            }

            if start.elapsed() >= timeout {
                return Ok(AgentChatPollResponse {
                    session_id: request.session_id,
                    chunks: vec![],
                    next_index,
                    status,
                });
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Cancel a session
    pub async fn cancel(&self, request: AgentChatCancelRequest) -> Result<()> {
        {
            let mut handles = self.handles.write().await;
            if let Some(handle) = handles.remove(&request.session_id) {
                handle.abort();
            }
        }

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
            sessions
                .get(&request.session_id)
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
            sessions
                .get(&request.session_id)
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
            sessions
                .get(&request.session_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Session not found: {}", request.session_id))?
        };

        let (conv_id, backend_id) = {
            let s = session.read().await;
            (s.conversation_id.clone(), s.backend_id.clone())
        };

        let backend = self
            .backends
            .get(&backend_id)
            .ok_or_else(|| anyhow::anyhow!("Backend not found: {}", backend_id))?;

        let db_messages = self.db.get_messages(&conv_id)?;
        let mut chat_messages = Vec::new();

        for m in &db_messages {
            let tool_calls = if m.role == llmchat::Role::Assistant {
                let calls = self.db.get_tool_calls_for_message(&m.id.0)?;
                if calls.is_empty() {
                    None
                } else {
                    Some(
                        calls
                            .iter()
                            .map(|tc| crate::types::ChatToolCall {
                                id: tc.id.0.clone(),
                                call_type: "function".to_string(),
                                function: crate::types::ChatFunctionCall {
                                    name: tc.tool_name.clone(),
                                    arguments: serde_json::to_string(&tc.arguments)
                                        .unwrap_or_default(),
                                },
                            })
                            .collect(),
                    )
                }
            } else {
                None
            };

            let tool_call_id = if m.role == llmchat::Role::Tool {
                self.db.get_tool_call_id_for_result_message(&m.id.0)?
            } else {
                None
            };

            chat_messages.push(crate::types::ChatMessage {
                role: m.role.as_str().to_string(),
                content: m.content.clone(),
                tool_calls,
                tool_call_id,
            });
        }

        let messages = OpenAiProvider::convert_messages(&chat_messages)?;

        backend.provider.summarize(messages).await
    }

    /// List sessions
    pub async fn list_sessions(&self, request: AgentChatListRequest) -> Result<Vec<SessionInfo>> {
        let sessions = self.sessions.read().await;

        let mut infos: Vec<SessionInfo> = Vec::new();

        for (id, handle) in sessions.iter() {
            let s = handle.read().await;

            if request.active_only
                && (s.status == SessionStatus::Finished
                    || s.status == SessionStatus::Failed
                    || s.status == SessionStatus::Cancelled)
            {
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
