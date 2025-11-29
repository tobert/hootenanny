# Task 05: Implement OpenAI-compatible provider

## Goal

Wrap async-openai for chat completions with tool support.

## Files to Create

- `crates/llm-mcp-bridge/src/provider.rs`

## Implementation

```rust
use anyhow::{Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestToolMessageArgs, ChatCompletionTool, ChatCompletionToolType,
        CreateChatCompletionRequestArgs, FunctionObject,
    },
    Client,
};
use serde_json::Value;

use crate::config::BackendConfig;
use crate::mcp_client::OpenAiFunction;

/// Generation configuration
#[derive(Debug, Clone, Default)]
pub struct GenerationConfig {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
}

/// OpenAI-compatible LLM provider
pub struct OpenAiProvider {
    client: Client<OpenAIConfig>,
    model: String,
    summary_model: String,
    default_config: GenerationConfig,
}

impl OpenAiProvider {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        let openai_config = OpenAIConfig::new()
            .with_api_base(&config.base_url)
            .with_api_key(config.api_key.as_deref().unwrap_or("not-needed"));

        Ok(Self {
            client: Client::with_config(openai_config),
            model: config.default_model.clone(),
            summary_model: config.summary_model().to_string(),
            default_config: GenerationConfig {
                temperature: config.default_temperature,
                max_tokens: config.max_tokens,
                ..Default::default()
            },
        })
    }

    /// Non-streaming chat completion
    #[tracing::instrument(
        skip(self, messages, tools),
        fields(
            llm.model = %self.model,
            llm.messages_count = messages.len(),
        )
    )]
    pub async fn chat(
        &self,
        messages: Vec<ChatCompletionRequestMessage>,
        tools: Option<Vec<ChatCompletionTool>>,
        config: &GenerationConfig,
    ) -> Result<ChatCompletionResponse> {
        let mut request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(messages);

        // Apply configuration
        if let Some(temp) = config.temperature.or(self.default_config.temperature) {
            request = request.temperature(temp);
        }
        if let Some(max) = config.max_tokens.or(self.default_config.max_tokens) {
            request = request.max_tokens(max);
        }
        if let Some(top_p) = config.top_p.or(self.default_config.top_p) {
            request = request.top_p(top_p);
        }

        // Add tools if provided
        if let Some(tools) = tools {
            if !tools.is_empty() {
                request = request.tools(tools);
            }
        }

        let request = request.build().context("Failed to build chat request")?;

        let response = self.client
            .chat()
            .create(request)
            .await
            .context("Chat completion failed")?;

        Ok(ChatCompletionResponse::from(response))
    }

    /// Quick summary using the summary model
    #[tracing::instrument(skip(self, messages))]
    pub async fn summarize(&self, messages: Vec<ChatCompletionRequestMessage>) -> Result<String> {
        let summary_prompt = ChatCompletionRequestUserMessageArgs::default()
            .content("Please provide a brief summary of this conversation in 2-3 sentences.")
            .build()?
            .into();

        let mut all_messages = messages;
        all_messages.push(summary_prompt);

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.summary_model)
            .messages(all_messages)
            .max_tokens(200u32)
            .temperature(0.3)
            .build()?;

        let response = self.client
            .chat()
            .create(request)
            .await
            .context("Summary generation failed")?;

        response.choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| anyhow::anyhow!("No summary generated"))
    }

    /// Convert our message format to async-openai format
    pub fn convert_messages(
        messages: &[crate::types::ChatMessage],
    ) -> Result<Vec<ChatCompletionRequestMessage>> {
        messages
            .iter()
            .map(|msg| Self::convert_message(msg))
            .collect()
    }

    fn convert_message(msg: &crate::types::ChatMessage) -> Result<ChatCompletionRequestMessage> {
        match msg.role.as_str() {
            "system" => Ok(ChatCompletionRequestSystemMessageArgs::default()
                .content(msg.content.clone().unwrap_or_default())
                .build()?
                .into()),
            "user" => Ok(ChatCompletionRequestUserMessageArgs::default()
                .content(msg.content.clone().unwrap_or_default())
                .build()?
                .into()),
            "assistant" => {
                let mut builder = ChatCompletionRequestAssistantMessageArgs::default();
                if let Some(content) = &msg.content {
                    builder = builder.content(content.clone());
                }
                // Tool calls would need conversion here
                Ok(builder.build()?.into())
            }
            "tool" => Ok(ChatCompletionRequestToolMessageArgs::default()
                .tool_call_id(msg.tool_call_id.clone().unwrap_or_default())
                .content(msg.content.clone().unwrap_or_default())
                .build()?
                .into()),
            _ => anyhow::bail!("Unknown message role: {}", msg.role),
        }
    }

    /// Convert functions to ChatCompletionTool format
    pub fn convert_tools(functions: &[OpenAiFunction]) -> Vec<ChatCompletionTool> {
        functions
            .iter()
            .map(|f| ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name: f.name.clone(),
                    description: f.description.clone(),
                    parameters: Some(f.parameters.clone()),
                    strict: None,
                },
            })
            .collect()
    }
}

/// Simplified response from chat completion
#[derive(Debug, Clone)]
pub struct ChatCompletionResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCallResponse>,
    pub finish_reason: FinishReason,
}

#[derive(Debug, Clone)]
pub struct ToolCallResponse {
    pub id: String,
    pub function_name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
    Unknown,
}

impl From<async_openai::types::CreateChatCompletionResponse> for ChatCompletionResponse {
    fn from(response: async_openai::types::CreateChatCompletionResponse) -> Self {
        let choice = response.choices.first();

        let content = choice.and_then(|c| c.message.content.clone());

        let tool_calls = choice
            .and_then(|c| c.message.tool_calls.as_ref())
            .map(|tcs| {
                tcs.iter()
                    .map(|tc| ToolCallResponse {
                        id: tc.id.clone(),
                        function_name: tc.function.name.clone(),
                        arguments: tc.function.arguments.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let finish_reason = choice
            .and_then(|c| c.finish_reason.as_ref())
            .map(|fr| match fr {
                async_openai::types::FinishReason::Stop => FinishReason::Stop,
                async_openai::types::FinishReason::ToolCalls => FinishReason::ToolCalls,
                async_openai::types::FinishReason::Length => FinishReason::Length,
                async_openai::types::FinishReason::ContentFilter => FinishReason::ContentFilter,
                _ => FinishReason::Unknown,
            })
            .unwrap_or(FinishReason::Unknown);

        Self {
            content,
            tool_calls,
            finish_reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_tools() {
        let functions = vec![OpenAiFunction {
            name: "test".to_string(),
            description: Some("A test function".to_string()),
            parameters: serde_json::json!({"type": "object"}),
        }];

        let tools = OpenAiProvider::convert_tools(&functions);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].function.name, "test");
    }
}
```

## Reference

- async-openai ollama example: https://github.com/64bit/async-openai/tree/main/examples/ollama-chat

## Acceptance Criteria

- [ ] Can create provider from BackendConfig
- [ ] Non-streaming chat completion works
- [ ] Tool/function definitions converted correctly
- [ ] Message format conversion handles all roles
- [ ] Summary generation uses summary_model
- [ ] Tracing spans include model and message count
- [ ] Finish reason correctly mapped
