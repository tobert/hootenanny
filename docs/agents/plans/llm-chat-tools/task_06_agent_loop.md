# Task 06: Implement agent session and tool loop

## Goal

The core agent loop that handles multi-turn conversations with tool calls.

## Files to Create

- `crates/llm-mcp-bridge/src/session.rs`
- `crates/llm-mcp-bridge/src/loop.rs`

## Session State (session.rs)

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

use crate::types::{SessionId, SessionStatus, OutputChunk, ChatMessage};

/// An agent session - a managed conversation with an LLM
pub struct AgentSession {
    pub id: SessionId,
    pub backend_id: String,
    pub conversation_id: String,  // llmchat ConversationId
    pub status: SessionStatus,
    pub enable_tools: bool,
    pub max_tool_iterations: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    /// Output chunks for polling
    output_chunks: Vec<OutputChunk>,

    /// Current generation config
    pub temperature: Option<f32>,
}

impl AgentSession {
    pub fn new(
        backend_id: &str,
        conversation_id: &str,
        enable_tools: bool,
        max_tool_iterations: u32,
    ) -> Self {
        Self {
            id: SessionId::new(),
            backend_id: backend_id.to_string(),
            conversation_id: conversation_id.to_string(),
            status: SessionStatus::Idle,
            enable_tools,
            max_tool_iterations,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            output_chunks: Vec::new(),
            temperature: None,
        }
    }

    /// Add an output chunk
    pub fn push_chunk(&mut self, chunk: OutputChunk) {
        self.output_chunks.push(chunk);
        self.updated_at = Utc::now();
    }

    /// Get chunks since index
    pub fn chunks_since(&self, index: usize) -> &[OutputChunk] {
        if index < self.output_chunks.len() {
            &self.output_chunks[index..]
        } else {
            &[]
        }
    }

    /// Total chunk count
    pub fn chunk_count(&self) -> usize {
        self.output_chunks.len()
    }

    /// Mark session as generating
    pub fn set_generating(&mut self) {
        self.status = SessionStatus::Generating;
        self.updated_at = Utc::now();
    }

    /// Mark session as executing tools
    pub fn set_executing_tools(&mut self) {
        self.status = SessionStatus::ExecutingTools;
        self.updated_at = Utc::now();
    }

    /// Mark session as finished
    pub fn set_finished(&mut self) {
        self.status = SessionStatus::Finished;
        self.updated_at = Utc::now();
    }

    /// Mark session as failed
    pub fn set_failed(&mut self, error: &str) {
        self.status = SessionStatus::Failed;
        self.push_chunk(OutputChunk::Error {
            message: error.to_string(),
            index: self.output_chunks.len(),
        });
        self.updated_at = Utc::now();
    }

    /// Mark session as cancelled
    pub fn set_cancelled(&mut self) {
        self.status = SessionStatus::Cancelled;
        self.updated_at = Utc::now();
    }
}

/// Thread-safe session handle
pub type SessionHandle = Arc<RwLock<AgentSession>>;
```

## Agent Loop (loop.rs)

```rust
use std::sync::Arc;
use anyhow::Result;
use tracing;

use llmchat::{ConversationDb, Role, NewToolCall, ToolCallId};
use crate::mcp_client::McpToolClient;
use crate::provider::{OpenAiProvider, GenerationConfig, FinishReason};
use crate::session::{AgentSession, SessionHandle};
use crate::types::OutputChunk;

/// Run the agent loop for a session
#[tracing::instrument(
    skip(session, provider, mcp_client, db),
    fields(
        session.id = %session.read().await.id.0,
        session.backend = %session.read().await.backend_id,
    )
)]
pub async fn run_agent_loop(
    session: SessionHandle,
    provider: Arc<OpenAiProvider>,
    mcp_client: Arc<McpToolClient>,
    db: Arc<ConversationDb>,
    user_message: &str,
) -> Result<()> {
    let (conv_id, enable_tools, max_iterations, temperature) = {
        let s = session.read().await;
        (
            s.conversation_id.clone(),
            s.enable_tools,
            s.max_tool_iterations,
            s.temperature,
        )
    };

    // Append user message to database
    db.append_message(&conv_id, Role::User, Some(user_message))?;

    // Get tool definitions if enabled
    let tools = if enable_tools {
        let tool_infos = mcp_client.list_tools().await?;
        let functions = mcp_client.to_openai_functions(&tool_infos);
        Some(OpenAiProvider::convert_tools(&functions))
    } else {
        None
    };

    // Build messages from conversation history
    let db_messages = db.get_messages(&conv_id)?;
    let mut messages = OpenAiProvider::convert_messages(
        &db_messages.iter().map(|m| crate::types::ChatMessage {
            role: m.role.as_str().to_string(),
            content: m.content.clone(),
            tool_calls: None,
            tool_call_id: None,
        }).collect::<Vec<_>>()
    )?;

    let config = GenerationConfig {
        temperature,
        ..Default::default()
    };

    // Agent loop
    for iteration in 0..max_iterations {
        tracing::info!(iteration, "Starting agent loop iteration");

        // Update status
        {
            let mut s = session.write().await;
            s.set_generating();
        }

        // Call LLM
        let response = provider.chat(messages.clone(), tools.clone(), &config).await?;

        // Check finish reason
        match response.finish_reason {
            FinishReason::Stop => {
                // Done - record assistant response
                let content = response.content.as_deref().unwrap_or("");
                db.append_message(&conv_id, Role::Assistant, Some(content))?;

                let index = {
                    let s = session.read().await;
                    s.chunk_count()
                };

                {
                    let mut s = session.write().await;
                    s.push_chunk(OutputChunk::TurnComplete {
                        finish_reason: "stop".to_string(),
                        content: response.content.clone(),
                        index,
                    });
                    s.set_finished();
                }

                return Ok(());
            }

            FinishReason::ToolCalls => {
                if response.tool_calls.is_empty() {
                    anyhow::bail!("ToolCalls finish reason but no tool calls");
                }

                // Record assistant message (may have partial content)
                let assistant_msg = db.append_message(
                    &conv_id,
                    Role::Assistant,
                    response.content.as_deref(),
                )?;

                {
                    let mut s = session.write().await;
                    s.set_executing_tools();
                }

                // Execute each tool call
                for tc in &response.tool_calls {
                    let index = {
                        let s = session.read().await;
                        s.chunk_count()
                    };

                    // Notify tool call started
                    {
                        let mut s = session.write().await;
                        s.push_chunk(OutputChunk::ToolCallStarted {
                            tool_call_id: tc.id.clone(),
                            name: tc.function_name.clone(),
                            index,
                        });
                    }

                    // Record tool call in database
                    let call_record = db.record_tool_call(
                        &assistant_msg.id.0,
                        NewToolCall {
                            id: ToolCallId(tc.id.clone()),
                            tool_name: tc.function_name.clone(),
                            arguments: serde_json::from_str(&tc.arguments)?,
                        },
                    )?;

                    // Execute tool
                    let start = std::time::Instant::now();
                    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
                    let result = mcp_client.call_tool(&tc.function_name, args).await;
                    let duration_ms = start.elapsed().as_millis() as i64;

                    let (output, is_error) = match result {
                        Ok(v) => (serde_json::to_string(&v)?, false),
                        Err(e) => (e.to_string(), true),
                    };

                    // Record tool result
                    let result_msg = db.append_message(&conv_id, Role::Tool, Some(&output))?;
                    db.record_tool_result(
                        &call_record.id.0,
                        &result_msg.id.0,
                        &output,
                        is_error,
                        Some(duration_ms),
                    )?;

                    let index = {
                        let s = session.read().await;
                        s.chunk_count()
                    };

                    // Notify tool call completed
                    {
                        let mut s = session.write().await;
                        s.push_chunk(OutputChunk::ToolCallCompleted {
                            tool_call_id: tc.id.clone(),
                            result: output.clone(),
                            is_error,
                            index,
                        });
                    }

                    // Add tool result to messages for next iteration
                    messages.push(
                        async_openai::types::ChatCompletionRequestToolMessageArgs::default()
                            .tool_call_id(&tc.id)
                            .content(output)
                            .build()?
                            .into(),
                    );
                }

                // Add assistant message with tool calls to message history
                // (This is complex with async-openai types, simplified here)
            }

            FinishReason::Length => {
                let index = {
                    let s = session.read().await;
                    s.chunk_count()
                };

                {
                    let mut s = session.write().await;
                    s.push_chunk(OutputChunk::TurnComplete {
                        finish_reason: "length".to_string(),
                        content: response.content.clone(),
                        index,
                    });
                    s.set_finished();
                }

                return Ok(());
            }

            _ => {
                anyhow::bail!("Unexpected finish reason: {:?}", response.finish_reason);
            }
        }
    }

    // Max iterations exceeded
    {
        let mut s = session.write().await;
        s.set_failed(&format!("Max tool iterations ({}) exceeded", max_iterations));
    }

    anyhow::bail!("Max tool iterations exceeded")
}
```

## Reference Files

- `crates/hootenanny/src/api/tools/orpheus.rs` - Background task pattern with handle storage
- `crates/hootenanny/src/job_system.rs` - JobStore for session management inspiration

## Acceptance Criteria

- [ ] Session state machine handles all transitions
- [ ] Agent loop terminates on Stop finish reason
- [ ] Tool calls are executed and results fed back
- [ ] Max iterations limit enforced
- [ ] Output chunks generated for polling
- [ ] Database records all messages and tool calls
- [ ] Proper tracing spans for observability
- [ ] Error handling doesn't panic
