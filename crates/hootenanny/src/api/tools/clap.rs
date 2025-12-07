use crate::api::schema::ClapAnalyzeRequest;
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash};
use baton::{ErrorData as McpError, CallToolResult, Content};
use tracing;

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.clap_analyze",
        skip(self, request),
        fields(
            audio_hash = %request.audio_hash,
            tasks = ?request.tasks,
        )
    )]
    pub async fn clap_analyze(
        &self,
        request: ClapAnalyzeRequest,
    ) -> Result<CallToolResult, McpError> {
        // Fetch audio from CAS
        let cas_ref = self.local_models.inspect_cas_content(&request.audio_hash).await
            .map_err(|e| McpError::internal_error(format!("Failed to inspect CAS: {}", e)))?;

        let audio_path = cas_ref.local_path
            .ok_or_else(|| McpError::internal_error("Audio not found in local CAS"))?;

        let audio_bytes = tokio::fs::read(&audio_path).await
            .map_err(|e| McpError::internal_error(format!("Failed to read audio: {}", e)))?;

        // Base64 encode audio for API
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        let audio_b64 = BASE64.encode(&audio_bytes);

        // Optional: second audio for similarity
        let audio_b_b64 = if let Some(hash_b) = &request.audio_b_hash {
            let cas_ref_b = self.local_models.inspect_cas_content(hash_b).await
                .map_err(|e| McpError::internal_error(format!("Failed to inspect audio_b CAS: {}", e)))?;

            let audio_b_path = cas_ref_b.local_path
                .ok_or_else(|| McpError::internal_error("Audio B not found in local CAS"))?;

            let audio_b_bytes = tokio::fs::read(&audio_b_path).await
                .map_err(|e| McpError::internal_error(format!("Failed to read audio B: {}", e)))?;

            Some(BASE64.encode(&audio_b_bytes))
        } else {
            None
        };

        // Call CLAP service
        let text_candidates = if request.text_candidates.is_empty() {
            None
        } else {
            Some(request.text_candidates.clone())
        };

        let analysis_result = self.local_models.run_clap_analyze(
            audio_b64,
            request.tasks.clone(),
            audio_b_b64,
            text_candidates,
            None, // client_job_id
        ).await
        .map_err(|e| McpError::internal_error(format!("CLAP analysis failed: {}", e)))?;

        // Store analysis as JSON artifact
        let analysis_json = serde_json::to_vec(&analysis_result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize analysis: {}", e)))?;

        let analysis_hash = self.local_models.store_cas_content(&analysis_json, "application/json").await
            .map_err(|e| McpError::internal_error(format!("Failed to store analysis in CAS: {}", e)))?;

        // Create artifact
        let content_hash = ContentHash::new(&analysis_hash);
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

        let mut artifact = Artifact::new(
            artifact_id.clone(),
            content_hash,
            request.creator.unwrap_or_else(|| "unknown".to_string()),
            serde_json::json!({
                "type": "clap_analysis",
                "source_audio_hash": request.audio_hash,
                "tasks": request.tasks,
                "results": analysis_result,
            })
        ).with_tags(vec!["type:analysis", "source:clap", "tool:clap_analyze"]);

        // Set parent if specified
        if let Some(parent_id) = request.parent_id {
            artifact = artifact.with_parent(ArtifactId::new(parent_id));
        }

        // Persist artifact
        {
            let store = self.artifact_store.write()
                .map_err(|e| McpError::internal_error(format!("Failed to acquire artifact store lock: {}", e)))?;
            store.put(artifact)
                .map_err(|e| McpError::internal_error(format!("Failed to store artifact: {}", e)))?;
            store.flush()
                .map_err(|e| McpError::internal_error(format!("Failed to flush artifact store: {}", e)))?;
        }

        // Build response summary
        let mut summary_parts = vec![];
        if analysis_result.get("embeddings").is_some() {
            summary_parts.push("embeddings".to_string());
        }
        if let Some(genre) = analysis_result.get("genre") {
            if let Some(top_label) = genre.get("top_label").and_then(|v| v.as_str()) {
                summary_parts.push(format!("genre: {}", top_label));
            }
        }
        if let Some(mood) = analysis_result.get("mood") {
            if let Some(top_label) = mood.get("top_label").and_then(|v| v.as_str()) {
                summary_parts.push(format!("mood: {}", top_label));
            }
        }
        if let Some(similarity) = analysis_result.get("similarity") {
            if let Some(score) = similarity.get("score").and_then(|v| v.as_f64()) {
                summary_parts.push(format!("similarity: {:.2}", score));
            }
        }

        let summary = if summary_parts.is_empty() {
            "Analysis complete".to_string()
        } else {
            format!("Analysis: {}", summary_parts.join(", "))
        };

        let response_body = serde_json::json!({
            "artifact_id": artifact_id.as_str(),
            "content_hash": analysis_hash,
            "tasks": request.tasks,
            "summary": summary,
        });

        Ok(CallToolResult::success(vec![Content::text(summary)])
            .with_structured(response_body))
    }
}
