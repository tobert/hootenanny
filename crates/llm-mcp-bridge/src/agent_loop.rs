use std::sync::Arc;

use anyhow::Result;

use llmchat::{ConversationDb, NewToolCall, Role, ToolCallId};

use crate::mcp_client::{McpToolClient, to_openai_functions};
use crate::provider::{FinishReason, GenerationConfig, OpenAiProvider};
use crate::session::SessionHandle;
use crate::types::OutputChunk;

/// Run the agent loop for a session
#[tracing::instrument(
    skip(session, provider, mcp_client, db, user_message),
    fields(
        session.backend = tracing::field::Empty,
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
        let mut s = session.write().await;
        tracing::Span::current().record("session.backend", &s.backend_id);
        s.clear_chunks(); // Clear stale chunks from previous turn
        (
            s.conversation_id.clone(),
            s.enable_tools,
            s.max_tool_iterations,
            s.temperature,
        )
    };

    db.append_message(&conv_id, Role::User, Some(user_message))?;

    let tools = if enable_tools {
        let tool_infos = mcp_client.list_tools().await?;
        let functions = to_openai_functions(&tool_infos);
        Some(OpenAiProvider::convert_tools(&functions))
    } else {
        None
    };

    let config = GenerationConfig {
        temperature,
        ..Default::default()
    };

    for iteration in 0..max_iterations {
        tracing::info!(iteration, "Starting agent loop iteration");

        {
            let mut s = session.write().await;
            s.set_generating();
        }

        let db_messages = db.get_messages(&conv_id)?;
        let mut chat_messages = Vec::new();

        for m in &db_messages {
            let tool_calls = if m.role == llmchat::Role::Assistant {
                let calls = db.get_tool_calls_for_message(&m.id.0)?;
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
                db.get_tool_call_id_for_result_message(&m.id.0)?
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

        let response = provider.chat(messages, tools.clone(), &config).await?;

        match response.finish_reason {
            FinishReason::Stop => {
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
                    s.set_idle(); // Ready for next turn
                }

                return Ok(());
            }

            FinishReason::ToolCalls => {
                if response.tool_calls.is_empty() {
                    anyhow::bail!("ToolCalls finish reason but no tool calls");
                }

                let assistant_msg =
                    db.append_message(&conv_id, Role::Assistant, response.content.as_deref())?;

                {
                    let mut s = session.write().await;
                    s.set_executing_tools();
                }

                for tc in &response.tool_calls {
                    let index = {
                        let s = session.read().await;
                        s.chunk_count()
                    };

                    {
                        let mut s = session.write().await;
                        s.push_chunk(OutputChunk::ToolCallStarted {
                            tool_call_id: tc.id.clone(),
                            name: tc.function_name.clone(),
                            index,
                        });
                    }

                    let call_record = db.record_tool_call(
                        &assistant_msg.id.0,
                        NewToolCall {
                            id: ToolCallId(tc.id.clone()),
                            tool_name: tc.function_name.clone(),
                            arguments: serde_json::from_str(&tc.arguments)?,
                        },
                    )?;

                    let start = std::time::Instant::now();
                    let args: serde_json::Value = serde_json::from_str(&tc.arguments)?;
                    let result = mcp_client.call_tool(&tc.function_name, args).await;
                    let duration_ms = start.elapsed().as_millis() as i64;

                    let (output, is_error) = match result {
                        Ok(v) => (serde_json::to_string(&v)?, false),
                        Err(e) => (e.to_string(), true),
                    };

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

                    {
                        let mut s = session.write().await;
                        s.push_chunk(OutputChunk::ToolCallCompleted {
                            tool_call_id: tc.id.clone(),
                            result: output.clone(),
                            is_error,
                            index,
                        });
                    }
                }
            }

            FinishReason::Length => {
                let content = response.content.as_deref().unwrap_or("");
                db.append_message(&conv_id, Role::Assistant, Some(content))?;

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
                    s.set_idle(); // Ready for next turn
                }

                return Ok(());
            }

            _ => {
                anyhow::bail!("Unexpected finish reason: {:?}", response.finish_reason);
            }
        }
    }

    {
        let mut s = session.write().await;
        s.set_failed(&format!("Max tool iterations ({}) exceeded", max_iterations));
    }

    anyhow::bail!("Max tool iterations exceeded")
}
