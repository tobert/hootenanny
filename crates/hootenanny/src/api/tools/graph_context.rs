//! Graph context tools for sub-agent asset discovery
//!
//! Provides bounded context about artifacts for sub-agent conversations.
//! Uses Trustfall queries through the graph_adapter for artifact queries.

use crate::api::responses::{GraphContextResponse, ContextSummary, AddAnnotationResponse};
use crate::api::schema::{AddAnnotationRequest, GraphContextRequest};
use crate::api::service::EventDualityServer;
use audio_graph_mcp::sources::AnnotationData;
use hooteproto::{ToolOutput, ToolResult, ToolError};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use trustfall::{execute_query, FieldValue};

impl EventDualityServer {
    #[tracing::instrument(name = "mcp.tool.graph_context", skip(self, request))]
    pub async fn graph_context(
        &self,
        request: GraphContextRequest,
    ) -> ToolResult {
        let limit = request.limit.unwrap_or(20);

        let (query, variables) = build_artifact_query(&request);

        let schema = self.graph_adapter.schema();
        let adapter_arc: Arc<_> = Arc::clone(&self.graph_adapter);

        let results_iter = execute_query(schema, adapter_arc, &query, variables)
            .map_err(|e| ToolError::internal(format!("Query failed: {}", e)))?;

        let results: Vec<_> = results_iter.take(limit).collect();

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

        let artifacts: Vec<serde_json::Value> = results
            .into_iter()
            .map(|row| {
                let mut obj = serde_json::Map::new();

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

        let total = artifacts.len();
        let response = GraphContextResponse {
            artifacts,
            summary: ContextSummary {
                total,
                by_type: type_counts,
            },
        };

        let json = serde_json::to_string_pretty(&response)
            .map_err(|e| ToolError::internal(format!("Failed to serialize: {}", e)))?;

        Ok(ToolOutput::new(json, &response))
    }

    #[tracing::instrument(name = "mcp.tool.add_annotation", skip(self, request))]
    pub async fn add_annotation(
        &self,
        request: AddAnnotationRequest,
    ) -> ToolResult {
        use audio_graph_mcp::sources::ArtifactSource;

        let artifact_store = self.artifact_store.read().map_err(|e| {
            ToolError::internal(format!("Failed to read artifact store: {}", e))
        })?;

        let artifact_exists = ArtifactSource::get(&*artifact_store, &request.artifact_id)
            .map_err(|e| ToolError::internal(format!("Failed to get artifact: {}", e)))?
            .is_some();

        if !artifact_exists {
            return Err(ToolError::invalid_params(format!(
                "Artifact not found: {}",
                request.artifact_id
            )));
        }

        let source = request.source.unwrap_or_else(|| "agent".to_string());
        let annotation = AnnotationData::new(
            request.artifact_id.clone(),
            request.message.clone(),
            request.vibe.clone(),
            source.clone(),
        );

        let annotation_id = annotation.id.clone();

        ArtifactSource::add_annotation(&*artifact_store, annotation).map_err(|e| {
            ToolError::internal(format!("Failed to store annotation: {}", e))
        })?;

        let response = AddAnnotationResponse {
            artifact_id: request.artifact_id.clone(),
            annotation_id: annotation_id.clone(),
            success: true,
        };

        let json = serde_json::to_string_pretty(&response)
            .map_err(|e| ToolError::internal(format!("Failed to serialize: {}", e)))?;

        Ok(ToolOutput::new(json, &response))
    }
}

fn build_artifact_query(
    request: &GraphContextRequest,
) -> (String, BTreeMap<Arc<str>, FieldValue>) {
    let mut variables: BTreeMap<Arc<str>, FieldValue> = BTreeMap::new();
    let mut params = Vec::new();

    if let Some(ref tag) = request.tag {
        variables.insert("tag".into(), FieldValue::String(tag.clone().into()));
        params.push("tag: $tag");
    }
    if let Some(ref creator) = request.creator {
        variables.insert("creator".into(), FieldValue::String(creator.clone().into()));
        params.push("creator: $creator");
    }
    if let Some(minutes) = request.within_minutes {
        variables.insert("within_minutes".into(), FieldValue::Int64(minutes));
        params.push("within_minutes: $within_minutes");
    }

    let entry_point = if params.is_empty() {
        // Default: uses adapter's DEFAULT_RECENT_WINDOW (10 minutes)
        "Artifact".to_string()
    } else {
        format!("Artifact({})", params.join(", "))
    };

    let query = format!(
        r#"
        query {{
            {} {{
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
        entry_point
    );

    (query, variables)
}
