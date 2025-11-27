use crate::api::service::EventDualityServer;
use crate::api::schema::{CasStoreRequest, CasInspectRequest, UploadFileRequest};
use rmcp::{ErrorData as McpError, model::{CallToolResult, Content}};
use base64::{Engine as _, engine::general_purpose};
use tracing;

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.cas_store",
        skip(self, request),
        fields(
            cas.mime_type = %request.mime_type,
            cas.content_size = request.content_base64.len(),
            cas.hash = tracing::field::Empty,
        )
    )]
    pub async fn cas_store(
        &self,
        request: CasStoreRequest,
    ) -> Result<CallToolResult, McpError> {
        let decoded_content = general_purpose::STANDARD.decode(&request.content_base64)
            .map_err(|e| McpError::parse_error(format!("Failed to base64 decode content: {}", e), None))?;

        let hash = self.local_models.store_cas_content(&decoded_content, &request.mime_type)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to store content in CAS: {}", e), None))?;

        tracing::Span::current().record("cas.hash", &hash);

        Ok(CallToolResult::success(vec![Content::text(hash)]))
    }

    #[tracing::instrument(
        name = "mcp.tool.cas_inspect",
        skip(self, request),
        fields(
            cas.hash = %request.hash,
            cas.mime_type = tracing::field::Empty,
            cas.size_bytes = tracing::field::Empty,
        )
    )]
    pub async fn cas_inspect(
        &self,
        request: CasInspectRequest,
    ) -> Result<CallToolResult, McpError> {
        let cas_ref = self.local_models.inspect_cas_content(&request.hash)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to inspect CAS: {}", e), None))?;

        let span = tracing::Span::current();
        span.record("cas.mime_type", &*cas_ref.mime_type);
        span.record("cas.size_bytes", cas_ref.size_bytes);

        let result = serde_json::json!({
            "hash": cas_ref.hash,
            "mime_type": cas_ref.mime_type,
            "size": cas_ref.size_bytes,
            "local_path": cas_ref.local_path,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize CAS reference: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tracing::instrument(
        name = "mcp.tool.upload_file",
        skip(self, request),
        fields(
            file.path = %request.file_path,
            file.mime_type = %request.mime_type,
            file.size = tracing::field::Empty,
            cas.hash = tracing::field::Empty,
        )
    )]
    pub async fn upload_file(
        &self,
        request: UploadFileRequest,
    ) -> Result<CallToolResult, McpError> {
        // Read file from disk
        let file_bytes = tokio::fs::read(&request.file_path)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to read file: {}", e), None))?;

        let span = tracing::Span::current();
        span.record("file.size", file_bytes.len());

        // Store in CAS
        let hash = self.local_models.store_cas_content(&file_bytes, &request.mime_type)
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to store file in CAS: {}", e), None))?;

        span.record("cas.hash", &*hash);

        let result = serde_json::json!({
            "hash": hash,
            "size_bytes": file_bytes.len(),
            "mime_type": request.mime_type,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize response: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}
