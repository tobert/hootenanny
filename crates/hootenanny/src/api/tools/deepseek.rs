use crate::api::schema::DeepSeekQueryRequest;
use crate::api::service::EventDualityServer;
use crate::mcp_tools::local_models::Message;
use baton::{CallToolResult, Content, ErrorData as McpError};

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.deepseek_query",
        skip(self, request),
        fields(
            model.name = ?request.model,
            messages.count = request.messages.len(),
        )
    )]
    pub async fn deepseek_query(
        &self,
        request: DeepSeekQueryRequest,
    ) -> Result<CallToolResult, McpError> {
        let messages: Vec<Message> = request
            .messages
            .into_iter()
            .map(|m| Message {
                role: m.role,
                content: m.content,
            })
            .collect();

        match self
            .local_models
            .run_deepseek_query(request.model, messages, Some(false))
            .await
        {
            Ok(result) => {
                let response = serde_json::json!({
                    "text": result.text,
                    "finish_reason": result.finish_reason,
                });

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default(),
                )]))
            }
            Err(e) => {
                tracing::error!(error = %e, "DeepSeek query failed");
                Ok(CallToolResult::error(format!("DeepSeek query failed: {}", e)))
            }
        }
    }
}
