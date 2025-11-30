use std::sync::Arc;

use async_trait::async_trait;
use baton::{
    CallToolResult, ErrorData, Handler, Implementation, Resource, ResourceContents,
    ResourceTemplate, Tool, ToolSchema,
};
use serde_json::Value;

use crate::manager::AgentManager;
use crate::types::*;

fn schema_for<T: schemars::JsonSchema>() -> ToolSchema {
    let settings = schemars::generate::SchemaSettings::draft07().with(|s| {
        s.inline_subschemas = true;
    });
    let gen = settings.into_generator();
    let schema = gen.into_root_schema_for::<T>();
    let value = serde_json::to_value(&schema).unwrap_or_default();
    ToolSchema::from_value(value)
}

pub struct AgentChatHandler {
    manager: Arc<AgentManager>,
}

impl AgentChatHandler {
    pub fn new(manager: Arc<AgentManager>) -> Self {
        Self { manager }
    }

    async fn read_recent_conversations(&self) -> Result<baton::types::resource::ReadResourceResult, ErrorData> {
        let conversations = self.manager
            .db()
            .list_recent(10)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        let result: Vec<_> = conversations
            .iter()
            .map(|c| {
                serde_json::json!({
                    "id": c.id.0,
                    "backend": c.backend,
                    "model": c.model,
                    "created_at": c.created_at.to_rfc3339(),
                    "updated_at": c.updated_at.to_rfc3339(),
                })
            })
            .collect();

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(baton::types::resource::ReadResourceResult::single(
            ResourceContents::text_with_mime("conversations://recent", json, "application/json"),
        ))
    }

    async fn read_active_sessions(&self) -> Result<baton::types::resource::ReadResourceResult, ErrorData> {
        let request = AgentChatListRequest {
            limit: 50,
            active_only: true,
        };

        let sessions = self.manager
            .list_sessions(request)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        let json = serde_json::to_string_pretty(&sessions)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(baton::types::resource::ReadResourceResult::single(
            ResourceContents::text_with_mime("sessions://active", json, "application/json"),
        ))
    }

    async fn read_conversation(&self, id: &str) -> Result<baton::types::resource::ReadResourceResult, ErrorData> {
        let conversation = self.manager
            .db()
            .get_conversation(id)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?
            .ok_or_else(|| ErrorData::invalid_params(format!("Conversation not found: {}", id)))?;

        let messages = self.manager
            .db()
            .get_messages(id)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        let result = serde_json::json!({
            "conversation": {
                "id": conversation.id.0,
                "backend": conversation.backend,
                "model": conversation.model,
                "system_prompt": conversation.system_prompt,
                "created_at": conversation.created_at.to_rfc3339(),
                "updated_at": conversation.updated_at.to_rfc3339(),
            },
            "messages": messages.iter().map(|m| {
                serde_json::json!({
                    "id": m.id.0,
                    "role": m.role.as_str(),
                    "content": m.content,
                    "created_at": m.created_at.to_rfc3339(),
                    "token_count": m.token_count,
                })
            }).collect::<Vec<_>>(),
        });

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(baton::types::resource::ReadResourceResult::single(
            ResourceContents::text_with_mime(
                &format!("conversations://{}", id),
                json,
                "application/json",
            ),
        ))
    }

    async fn read_conversation_tools(&self, conv_id: &str) -> Result<baton::types::resource::ReadResourceResult, ErrorData> {
        let messages = self.manager
            .db()
            .get_messages(conv_id)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        let mut tool_calls_with_results = Vec::new();

        for msg in messages.iter().filter(|m| m.role == llmchat::Role::Assistant) {
            let calls = self.manager
                .db()
                .get_tool_calls_for_message(&msg.id.0)
                .map_err(|e| ErrorData::internal_error(e.to_string()))?;

            for call in calls {
                let result = self.manager
                    .db()
                    .get_tool_result(&call.id.0)
                    .map_err(|e| ErrorData::internal_error(e.to_string()))?;

                tool_calls_with_results.push(serde_json::json!({
                    "tool_call_id": call.id.0,
                    "tool_name": call.tool_name,
                    "arguments": call.arguments,
                    "created_at": call.created_at.to_rfc3339(),
                    "result": result.map(|r| serde_json::json!({
                        "output": r.output,
                        "is_error": r.is_error,
                        "duration_ms": r.duration_ms,
                    })),
                }));
            }
        }

        let result = serde_json::json!({
            "conversation_id": conv_id,
            "tool_calls": tool_calls_with_results,
        });

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(baton::types::resource::ReadResourceResult::single(
            ResourceContents::text_with_mime(
                &format!("conversations://{}/tools", conv_id),
                json,
                "application/json",
            ),
        ))
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
            Tool::new("agent_chat_backends", "List available LLM backends").read_only(),
        ]
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<CallToolResult, ErrorData> {
        match name {
            "agent_chat_new" => {
                let request: AgentChatNewRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.create_session(request).await {
                    Ok(response) => Ok(CallToolResult::text(
                        serde_json::to_string_pretty(&response).unwrap(),
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
                        serde_json::to_string_pretty(&response).unwrap(),
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
                        serde_json::to_string_pretty(&response).unwrap(),
                    )),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_history" => {
                let request: AgentChatHistoryRequest = serde_json::from_value(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string()))?;

                match self.manager.get_history(request).await {
                    Ok(messages) => Ok(CallToolResult::text(
                        serde_json::to_string_pretty(&messages).unwrap(),
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
                        serde_json::to_string_pretty(&sessions).unwrap(),
                    )),
                    Err(e) => Ok(CallToolResult::error(e.to_string())),
                }
            }

            "agent_chat_backends" => {
                let backends: Vec<_> = self.manager
                    .list_backends()
                    .iter()
                    .map(|b| {
                        serde_json::json!({
                            "id": b.id,
                            "display_name": b.display_name,
                            "default_model": b.default_model,
                            "supports_tools": b.supports_tools,
                        })
                    })
                    .collect();

                Ok(CallToolResult::text(
                    serde_json::to_string_pretty(&backends).unwrap(),
                ))
            }

            _ => Err(ErrorData::tool_not_found(name)),
        }
    }

    fn server_info(&self) -> Implementation {
        Implementation::new("llm-mcp-bridge", env!("CARGO_PKG_VERSION"))
    }

    fn resources(&self) -> Vec<Resource> {
        vec![
            Resource::new("conversations://recent", "recent-conversations")
                .with_description("10 most recently updated conversations")
                .with_mime_type("application/json"),
            Resource::new("sessions://active", "active-sessions")
                .with_description("Currently active agent sessions")
                .with_mime_type("application/json"),
        ]
    }

    fn resource_templates(&self) -> Vec<ResourceTemplate> {
        vec![
            ResourceTemplate::new("conversations://{id}", "conversation-detail")
                .with_description("Full conversation with all messages")
                .with_mime_type("application/json"),
            ResourceTemplate::new("conversations://{id}/tools", "conversation-tools")
                .with_description("Tool calls and results in this conversation")
                .with_mime_type("application/json"),
        ]
    }

    async fn read_resource(&self, uri: &str) -> Result<baton::types::resource::ReadResourceResult, ErrorData> {
        if uri == "conversations://recent" {
            return self.read_recent_conversations().await;
        }

        if uri == "sessions://active" {
            return self.read_active_sessions().await;
        }

        if let Some(path) = uri.strip_prefix("conversations://") {
            if let Some(id) = path.strip_suffix("/tools") {
                return self.read_conversation_tools(id).await;
            }

            return self.read_conversation(path).await;
        }

        Err(ErrorData::invalid_params(format!("Unknown resource: {}", uri)))
    }
}
