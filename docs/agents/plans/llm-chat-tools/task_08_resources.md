# Task 08: Add MCP resources for conversation introspection

## Goal

Let LLMs inspect conversation state via MCP resources.

## Files to Modify

- `crates/llm-mcp-bridge/src/handler.rs`

## Resources to Add

| URI | Description |
|-----|-------------|
| `conversations://recent` | Last 10 conversations |
| `conversations://{id}` | Full conversation with messages |
| `conversations://{id}/tools` | Tool calls and results for a conversation |
| `sessions://active` | Currently active agent sessions |

## Implementation

Add to `AgentChatHandler`:

```rust
use baton::{Resource, ResourceTemplate, ResourceContents};
use baton::types::resource::ReadResourceResult;

#[async_trait]
impl Handler for AgentChatHandler {
    // ... existing tools() and call_tool() ...

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

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, ErrorData> {
        // Parse URI
        if uri == "conversations://recent" {
            return self.read_recent_conversations().await;
        }

        if uri == "sessions://active" {
            return self.read_active_sessions().await;
        }

        if uri.starts_with("conversations://") {
            let path = &uri["conversations://".len()..];

            if path.ends_with("/tools") {
                let id = &path[..path.len() - 6];
                return self.read_conversation_tools(id).await;
            }

            return self.read_conversation(path).await;
        }

        Err(ErrorData::invalid_params(format!("Unknown resource: {}", uri)))
    }
}

impl AgentChatHandler {
    async fn read_recent_conversations(&self) -> Result<ReadResourceResult, ErrorData> {
        let conversations = self.manager.db
            .list_recent(10)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        let result: Vec<_> = conversations.iter().map(|c| {
            serde_json::json!({
                "id": c.id.0,
                "backend": c.backend,
                "model": c.model,
                "created_at": c.created_at.to_rfc3339(),
                "updated_at": c.updated_at.to_rfc3339(),
            })
        }).collect();

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
            "conversations://recent",
            json,
            "application/json",
        )))
    }

    async fn read_active_sessions(&self) -> Result<ReadResourceResult, ErrorData> {
        let request = AgentChatListRequest {
            limit: 50,
            active_only: true,
        };

        let sessions = self.manager.list_sessions(request).await
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        let json = serde_json::to_string_pretty(&sessions)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
            "sessions://active",
            json,
            "application/json",
        )))
    }

    async fn read_conversation(&self, id: &str) -> Result<ReadResourceResult, ErrorData> {
        let conversation = self.manager.db
            .get_conversation(id)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?
            .ok_or_else(|| ErrorData::invalid_params(format!("Conversation not found: {}", id)))?;

        let messages = self.manager.db
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

        Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
            &format!("conversations://{}", id),
            json,
            "application/json",
        )))
    }

    async fn read_conversation_tools(&self, conv_id: &str) -> Result<ReadResourceResult, ErrorData> {
        // Get all messages for conversation
        let messages = self.manager.db
            .get_messages(conv_id)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        // Get tool calls for each assistant message
        let mut tool_calls_with_results = Vec::new();

        for msg in messages.iter().filter(|m| m.role == llmchat::Role::Assistant) {
            let calls = self.manager.db
                .get_tool_calls_for_message(&msg.id.0)
                .map_err(|e| ErrorData::internal_error(e.to_string()))?;

            for call in calls {
                // Get result if exists
                let result_query = format!(
                    "SELECT output, is_error, duration_ms FROM tool_results WHERE tool_call_id = ?1"
                );
                // Note: This would need direct DB access, simplified here

                tool_calls_with_results.push(serde_json::json!({
                    "tool_call_id": call.id.0,
                    "tool_name": call.tool_name,
                    "arguments": call.arguments,
                    "created_at": call.created_at.to_rfc3339(),
                    // "result": would need separate query
                }));
            }
        }

        let result = serde_json::json!({
            "conversation_id": conv_id,
            "tool_calls": tool_calls_with_results,
        });

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string()))?;

        Ok(ReadResourceResult::single(ResourceContents::text_with_mime(
            &format!("conversations://{}/tools", conv_id),
            json,
            "application/json",
        )))
    }
}
```

## Reference Files

- `crates/hootenanny/src/api/handler.rs:284-332` - Existing resource implementation patterns

## Acceptance Criteria

- [ ] `conversations://recent` returns last 10 conversations
- [ ] `conversations://{id}` returns full conversation with messages
- [ ] `conversations://{id}/tools` returns tool calls and results
- [ ] `sessions://active` returns active agent sessions
- [ ] Resources return valid JSON
- [ ] Error handling for missing resources
