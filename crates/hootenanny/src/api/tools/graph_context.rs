//! Graph context tools for sub-agent asset discovery
//!
//! Provides bounded context about artifacts for sub-agent conversations.

use crate::api::schema::{GraphContextRequest, AddAnnotationRequest};
use crate::api::service::EventDualityServer;
use baton::{ErrorData as McpError, CallToolResult, Content};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use trustfall::{execute_query, FieldValue};

impl EventDualityServer {
    /// Generate a bounded context about artifacts for sub-agents
    ///
    /// Returns a JSON object with:
    /// - Summary counts by type
    /// - List of matching artifacts with optional metadata and annotations
    /// - Suitable for injecting into agent prompts
    ///
    /// Now uses Trustfall for consistent graph query execution.
    #[tracing::instrument(name = "mcp.tool.graph_context", skip(self, request))]
    pub async fn graph_context(
        &self,
        request: GraphContextRequest,
    ) -> Result<CallToolResult, McpError> {
        let limit = request.limit.unwrap_or(20);

        // Build Trustfall query from request parameters
        let query = build_context_query(&request);

        // Execute query through Trustfall
        let schema = self.graph_adapter.schema();
        let adapter_arc: Arc<_> = Arc::clone(&self.graph_adapter);
        let results_iter = execute_query(
            schema,
            adapter_arc,
            &query,
            BTreeMap::<Arc<str>, FieldValue>::new(), // No variables for now, filters are in query string
        )
        .map_err(|e| McpError::internal_error(format!("Query execution failed: {}", e)))?;

        // Collect and aggregate results
        let rows: Vec<_> = results_iter.take(limit * 10).collect(); // Extra rows for annotations
        let artifacts = aggregate_artifact_rows(rows, &request);

        // Take only up to limit artifacts (after aggregation)
        let artifacts: Vec<_> = artifacts.into_iter().take(limit).collect();

        // Build summary counts by metadata type
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        for artifact in &artifacts {
            let meta_type = artifact["type"].as_str().unwrap_or("unknown").to_string();
            *type_counts.entry(meta_type).or_insert(0) += 1;
        }

        // Build final context
        let context = serde_json::json!({
            "summary": {
                "total_matching": artifacts.len(),
                "by_type": type_counts,
                "filter": {
                    "tag": request.tag,
                    "vibe_search": request.vibe_search,
                    "creator": request.creator,
                },
            },
            "artifacts": artifacts,
        });

        let json = serde_json::to_string_pretty(&context)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize context: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Add an annotation to an artifact
    #[tracing::instrument(name = "mcp.tool.add_annotation", skip(self, request))]
    pub async fn add_annotation(
        &self,
        request: AddAnnotationRequest,
    ) -> Result<CallToolResult, McpError> {
        // Verify artifact exists
        let artifact_source = self.graph_adapter.artifact_source();
        let artifact = artifact_source.get_artifact(&request.artifact_id)
            .map_err(|e| McpError::internal_error(format!("Failed to get artifact: {}", e)))?
            .ok_or_else(|| McpError::invalid_params(format!("Artifact not found: {}", request.artifact_id)))?;

        // Generate annotation ID
        let annotation_id = format!("ann_{}", &uuid::Uuid::new_v4().to_string().replace("-", "")[..12]);

        // Add annotation to database
        let source = request.source.unwrap_or_else(|| "agent".to_string());
        self.audio_graph_db.add_annotation(
            &annotation_id,
            "artifact",
            &request.artifact_id,
            &source,
            &request.message,
            request.vibe.as_deref(),
        ).map_err(|e| McpError::internal_error(format!("Failed to add annotation: {}", e)))?;

        let result = serde_json::json!({
            "annotation_id": annotation_id,
            "artifact_id": artifact.id,
            "message": request.message,
            "vibe": request.vibe,
            "source": source,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize result: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

// ============================================================================
// Helper Functions for Trustfall Query Building and Aggregation
// ============================================================================

/// Build a Trustfall query string from GraphContextRequest parameters
fn build_context_query(request: &GraphContextRequest) -> String {
    let mut query = String::from("{\n  Artifact");

    // Add filter parameters
    let mut params = Vec::new();
    if let Some(tag) = &request.tag {
        params.push(format!("tag: \"{}\"", tag));
    }
    if let Some(creator) = &request.creator {
        params.push(format!("creator: \"{}\"", creator));
    }
    if let Some(vibe) = &request.vibe_search {
        params.push(format!("annotation_contains: \"{}\"", vibe));
    }

    if !params.is_empty() {
        query.push_str("(");
        query.push_str(&params.join(", "));
        query.push_str(")");
    }

    query.push_str(" {\n");
    query.push_str("    id @output\n");
    query.push_str("    content_hash @output\n");
    query.push_str("    created_at @output\n");
    query.push_str("    creator @output\n");
    query.push_str("    metadata_type @output\n");
    query.push_str("    tags { tag @output }\n");

    if request.include_annotations {
        query.push_str("    annotations {\n");
        query.push_str("      message @output\n");
        query.push_str("      vibe @output\n");
        query.push_str("      source @output\n");
        query.push_str("    }\n");
    }

    query.push_str("  }\n}");
    query
}

/// Aggregate Trustfall rows into artifact objects
///
/// Trustfall returns one row per nested item (e.g., one row per tag, one row per annotation).
/// We need to group these rows by artifact ID and rebuild the artifact structure.
fn aggregate_artifact_rows(
    rows: Vec<BTreeMap<Arc<str>, FieldValue>>,
    request: &GraphContextRequest,
) -> Vec<serde_json::Value> {
    let mut artifacts_map: HashMap<String, serde_json::Value> = HashMap::new();

    for row in rows {
        // Extract artifact ID
        let artifact_id = match row.get("id" as &str) {
            Some(FieldValue::String(s)) => s.to_string(),
            _ => continue, // Skip rows without ID
        };

        // Get or create artifact entry
        let artifact = artifacts_map.entry(artifact_id.clone()).or_insert_with(|| {
            // Create artifact from first row
            let mut artifact = serde_json::json!({
                "id": artifact_id,
                "type": field_value_to_string(row.get("metadata_type" as &str)),
                "creator": field_value_to_string(row.get("creator" as &str)),
                "created_at": field_value_to_string(row.get("created_at" as &str)),
                "tags": Vec::<String>::new(),
            });

            // Initialize annotations array if needed
            if request.include_annotations {
                artifact["annotations"] = serde_json::Value::Array(Vec::new());
            }

            artifact
        });

        // Add tag if present
        if let Some(FieldValue::String(tag)) = row.get("tag" as &str) {
            if let Some(tags_array) = artifact["tags"].as_array_mut() {
                let tag_str = tag.to_string();
                if !tags_array.iter().any(|t| t.as_str() == Some(&tag_str)) {
                    tags_array.push(serde_json::Value::String(tag_str));
                }
            }
        }

        // Add annotation if present
        if request.include_annotations {
            if let Some(FieldValue::String(message)) = row.get("message" as &str) {
                if let Some(annotations) = artifact["annotations"].as_array_mut() {
                    let ann = serde_json::json!({
                        "message": message.to_string(),
                        "vibe": field_value_to_string(row.get("vibe" as &str)),
                        "source": field_value_to_string(row.get("source" as &str)),
                    });
                    // Avoid duplicates
                    if !annotations.iter().any(|a| a["message"] == ann["message"]) {
                        annotations.push(ann);
                    }
                }
            }
        }
    }

    // Convert to vec and sort by ID for consistency
    let mut artifacts: Vec<_> = artifacts_map.into_values().collect();
    artifacts.sort_by(|a, b| {
        let a_id = a["id"].as_str().unwrap_or("");
        let b_id = b["id"].as_str().unwrap_or("");
        a_id.cmp(b_id)
    });

    artifacts
}

/// Helper to convert FieldValue to string, handling None gracefully
fn field_value_to_string(field: Option<&FieldValue>) -> String {
    match field {
        Some(FieldValue::String(s)) => s.to_string(),
        Some(FieldValue::Null) | None => String::new(),
        Some(other) => format!("{:?}", other),
    }
}
