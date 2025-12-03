//! Sampling Tools
//!
//! Tools for server-initiated LLM sampling requests.

use crate::api::responses::SampleLlmResponse;
use crate::api::schema::SampleLlmRequest;
use crate::api::service::EventDualityServer;
use baton::{CallToolResult, Content, ErrorData as McpError, ToolContext};

impl EventDualityServer {
    /// Request LLM inference from the connected client.
    ///
    /// This is a meta-tool - the server calls back to the client's LLM to get inference.
    /// Requires the client to advertise sampling capability during initialization.
    #[tracing::instrument(
        name = "mcp.tool.sample_llm",
        skip(self, request, context),
        fields(
            prompt_length = request.prompt.len(),
            max_tokens = request.max_tokens,
        )
    )]
    pub async fn sample_llm(
        &self,
        request: SampleLlmRequest,
        context: ToolContext,
    ) -> Result<CallToolResult, McpError> {
        // Check if client supports sampling
        let sampler = context
            .sampler
            .ok_or_else(|| McpError::invalid_request("Client does not support sampling"))?;

        // Build sampling request
        let sampling_request = baton::types::sampling::SamplingRequest {
            messages: vec![baton::types::sampling::SamplingMessage::user(&request.prompt)],
            max_tokens: request.max_tokens.or(Some(500)),
            temperature: request.temperature,
            system_prompt: request.system_prompt,
            ..Default::default()
        };

        // Send sampling request
        let sampling_response = sampler
            .sample(sampling_request)
            .await
            .map_err(|e| McpError::internal_error(format!("Sampling failed: {}", e)))?;

        // Extract text from response
        let response_text = if let Some(text) = sampling_response.content.as_text() {
            text.to_string()
        } else {
            String::new()
        };

        // Build response
        let response = SampleLlmResponse {
            response: response_text.clone(),
            model: sampling_response.model.clone(),
            stop_reason: sampling_response
                .stop_reason
                .map(|r| format!("{:?}", r)),
        };

        let human_text = format!(
            "LLM Response ({}): {}",
            sampling_response.model,
            if response_text.len() > 100 {
                format!("{}...", &response_text[..100])
            } else {
                response_text
            }
        );

        Ok(CallToolResult::success(vec![Content::text(human_text)])
            .with_structured(serde_json::to_value(&response).unwrap()))
    }
}
