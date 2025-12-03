//! Graph context tools for sub-agent asset discovery
//!
//! Provides bounded context about artifacts for sub-agent conversations.
//! Uses Trustfall queries through the graph_adapter for artifact queries.

use crate::api::schema::{AddAnnotationRequest, GraphContextRequest};
use crate::api::service::EventDualityServer;
use audio_graph_mcp::sources::AnnotationData;
use baton::{CallToolResult, Content, ErrorData as McpError};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use trustfall::{execute_query, FieldValue};

impl EventDualityServer {
    /// Generate a bounded context about artifacts for sub-agents
    ///
    /// Uses Trustfall queries to fetch artifacts based on filters.
    /// Returns a JSON object with:
    /// - Summary counts by type
    /// - List of matching artifacts with optional metadata
    /// - Suitable for injecting into agent prompts
    #[tracing::instrument(name = "mcp.tool.graph_context", skip(self, request))]
    pub async fn graph_context(
        &self,
        request: GraphContextRequest,
    ) -> Result<CallToolResult, McpError> {
        let limit = request.limit.unwrap_or(20);

        // Build Trustfall query based on filters
        let (query, variables) = build_artifact_query(&request);

        // Execute via Trustfall
        let schema = self.graph_adapter.schema();
        let adapter_arc: Arc<_> = Arc::clone(&self.graph_adapter);

        let results_iter = execute_query(schema, adapter_arc, &query, variables)
            .map_err(|e| McpError::internal_error(format!("Query failed: {}", e)))?;

        // Collect results
        let results: Vec<_> = results_iter.take(limit).collect();

        // Build summary counts by type
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        for result in &results {
            if let Some(FieldValue::List(tags)) = result.get("tags" as &str) {
                let type_tag = tags
                    .iter()
                    .filter_map(|t| {
                        if let FieldValue::String(s) = t {
                            if s.starts_with("type:") {
                                return Some(s.strip_prefix("type:").unwrap_or(s).to_string());
                            }
                        }
                        None
                    })
                    .next()
                    .unwrap_or_else(|| "unknown".to_string());
                *type_counts.entry(type_tag).or_insert(0) += 1;
            }
        }

        // Convert results to JSON representation
        let artifacts: Vec<serde_json::Value> = results
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();

                // Required fields
                if let Some(FieldValue::String(id)) = row.get("id" as &str) {
                    obj.insert("id".to_string(), serde_json::Value::String(id.to_string()));
                }
                if let Some(FieldValue::String(hash)) = row.get("content_hash" as &str) {
                    obj.insert(
                        "content_hash".to_string(),
                        serde_json::Value::String(hash.to_string()),
                    );
                }
                if let Some(FieldValue::String(created)) = row.get("created_at" as &str) {
                    obj.insert(
                        "created_at".to_string(),
                        serde_json::Value::String(created.to_string()),
                    );
                }
                if let Some(FieldValue::String(creator)) = row.get("creator" as &str) {
                    obj.insert(
                        "creator".to_string(),
                        serde_json::Value::String(creator.to_string()),
                    );
                }

                // Tags
                if let Some(FieldValue::List(tags)) = row.get("tags" as &str) {
                    let tag_arr: Vec<serde_json::Value> = tags
                        .iter()
                        .filter_map(|t| {
                            if let FieldValue::String(s) = t {
                                Some(serde_json::Value::String(s.to_string()))
                            } else {
                                None
                            }
                        })
                        .collect();
                    obj.insert("tags".to_string(), serde_json::Value::Array(tag_arr));

                    // Extract type from tags
                    for t in tags.iter() {
                        if let FieldValue::String(s) = t {
                            if s.starts_with("type:") {
                                obj.insert(
                                    "type".to_string(),
                                    serde_json::Value::String(
                                        s.strip_prefix("type:").unwrap_or(s).to_string(),
                                    ),
                                );
                                break;
                            }
                        }
                    }
                }

                // Optional fields
                if let Some(FieldValue::String(v)) = row.get("variation_set_id" as &str) {
                    obj.insert(
                        "variation_set_id".to_string(),
                        serde_json::Value::String(v.to_string()),
                    );
                }
                if let Some(FieldValue::Int64(idx)) = row.get("variation_index" as &str) {
                    obj.insert(
                        "variation_index".to_string(),
                        serde_json::Value::Number((*idx).into()),
                    );
                }

                // Include metadata if requested
                if request.include_metadata {
                    if let Some(FieldValue::String(m)) = row.get("metadata" as &str) {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(m) {
                            obj.insert("metadata".to_string(), parsed);
                        }
                    }
                }

                serde_json::Value::Object(obj)
            })
            .collect();

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
    ///
    /// Uses the proper annotation storage system via ArtifactSource trait.
    #[tracing::instrument(name = "mcp.tool.add_annotation", skip(self, request))]
    pub async fn add_annotation(
        &self,
        request: AddAnnotationRequest,
    ) -> Result<CallToolResult, McpError> {
        use audio_graph_mcp::sources::ArtifactSource;

        // Verify artifact exists
        let artifact_store = self.artifact_store.read().map_err(|e| {
            McpError::internal_error(format!("Failed to read artifact store: {}", e))
        })?;

        let artifact_exists = ArtifactSource::get(&*artifact_store, &request.artifact_id)
            .map_err(|e| McpError::internal_error(format!("Failed to get artifact: {}", e)))?
            .is_some();

        if !artifact_exists {
            return Err(McpError::invalid_params(format!(
                "Artifact not found: {}",
                request.artifact_id
            )));
        }

        // Create annotation
        let source = request.source.unwrap_or_else(|| "agent".to_string());
        let annotation = AnnotationData::new(
            request.artifact_id.clone(),
            request.message.clone(),
            request.vibe.clone(),
            source.clone(),
        );

        let annotation_id = annotation.id.clone();

        // Store the annotation
        ArtifactSource::add_annotation(&*artifact_store, annotation).map_err(|e| {
            McpError::internal_error(format!("Failed to store annotation: {}", e))
        })?;

        let result = serde_json::json!({
            "annotation_id": annotation_id,
            "artifact_id": request.artifact_id,
            "message": request.message,
            "vibe": request.vibe,
            "source": source,
        });

        let json = serde_json::to_string(&result)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize result: {}", e)))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

/// Build a Trustfall query for artifacts based on request filters
fn build_artifact_query(
    request: &GraphContextRequest,
) -> (String, BTreeMap<Arc<str>, FieldValue>) {
    let mut variables: BTreeMap<Arc<str>, FieldValue> = BTreeMap::new();

    // Build query parameters
    let query_params = if let Some(ref tag) = request.tag {
        variables.insert("tag".into(), FieldValue::String(tag.clone().into()));
        "tag: $tag"
    } else if let Some(ref creator) = request.creator {
        variables.insert("creator".into(), FieldValue::String(creator.clone().into()));
        "creator: $creator"
    } else {
        ""
    };

    // Build query - note: vibe_search would require annotation traversal
    // For now we filter by tag or creator at the entry point
    let query = format!(
        r#"
        query {{
            Artifact({}) {{
                id @output
                content_hash @output
                created_at @output
                creator @output
                tags @output
                variation_set_id @output
                variation_index @output
                metadata @output
            }}
        }}
        "#,
        query_params
    );

    (query, variables)
}
