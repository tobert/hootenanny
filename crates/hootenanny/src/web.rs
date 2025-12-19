//! Web endpoints for Hootenanny.
//!
//! Provides HTTP access to artifacts. Content is served through artifact IDs,
//! with CAS as an internal implementation detail.
//!
//! Note: MCP handlers have migrated to the baton crate.

use crate::artifact_store::{ArtifactStore, FileStore};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use cas::{ContentStore, FileStore as CasFileStore};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tokio_util::io::ReaderStream;

/// Shared state for web handlers
#[derive(Clone)]
pub struct WebState {
    pub artifact_store: Arc<RwLock<FileStore>>,
    pub cas: Arc<CasFileStore>,
}

pub fn router(state: WebState) -> Router {
    Router::new()
        .route("/artifact/{id}", get(download_artifact))
        .route("/artifact/{id}/meta", get(artifact_meta))
        .route("/artifacts", get(list_artifacts))
        .with_state(state)
}

/// Download artifact content
///
/// Resolves artifact ID to CAS content and streams it with the correct MIME type.
/// Records access in the artifact for tracking.
#[tracing::instrument(
    name = "http.artifact.content",
    skip(state),
    fields(
        artifact.id = %id,
        artifact.content_hash = tracing::field::Empty,
        artifact.creator = tracing::field::Empty,
        artifact.access_count = tracing::field::Empty,
    )
)]
async fn download_artifact(State(state): State<WebState>, Path(id): Path<String>) -> Response {
    // Get artifact and update access
    let (content_hash, mime_type, path, access_count, artifact_id_str) = {
        let store = match state.artifact_store.write() {
            Ok(s) => s,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        let mut artifact = match store.get(&id) {
            Ok(Some(a)) => a,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        };

        // Record access
        artifact.record_access();
        let access_count = artifact.access_count;
        let content_hash = artifact.content_hash.clone();
        let creator = artifact.creator.clone();
        let artifact_id_str = artifact.id.as_str().to_string();

        // Persist updated artifact
        if let Err(e) = store.put(artifact) {
            tracing::warn!("Failed to persist access update: {}", e);
        }
        if let Err(e) = store.flush() {
            tracing::warn!("Failed to flush artifact store: {}", e);
        }

        // Record in span
        let span = tracing::Span::current();
        span.record("artifact.content_hash", content_hash.as_str());
        span.record("artifact.creator", &creator);
        span.record("artifact.access_count", access_count);

        // Get CAS info
        let cas_hash: cas::ContentHash = match content_hash.as_str().parse() {
            Ok(h) => h,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let cas_ref = match state.cas.inspect(&cas_hash) {
            Ok(Some(r)) => r,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };

        let path = match state.cas.path(&cas_hash) {
            Some(p) => p,
            None => return StatusCode::NOT_FOUND.into_response(),
        };

        (
            content_hash,
            cas_ref.mime_type,
            path,
            access_count,
            artifact_id_str,
        )
    };

    // Stream content
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header("X-Artifact-Id", artifact_id_str)
        .header("X-Content-Hash", content_hash.as_str())
        .header("X-Access-Count", access_count.to_string())
        .body(body)
        .map_err(|e| {
            tracing::error!("Failed to build response: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .unwrap_or_else(|status| status.into_response())
}

/// Artifact metadata response
#[derive(Serialize)]
struct ArtifactMetaResponse {
    id: String,
    content_hash: String,
    content_url: String,
    mime_type: Option<String>,
    size_bytes: Option<u64>,
    creator: String,
    created_at: String,
    tags: Vec<String>,
    variation_set_id: Option<String>,
    variation_index: Option<u32>,
    parent_id: Option<String>,
    access_count: u64,
    last_accessed: Option<String>,
    metadata: serde_json::Value,
}

/// Get artifact metadata as JSON
#[tracing::instrument(name = "http.artifact.meta", skip(state))]
async fn artifact_meta(State(state): State<WebState>, Path(id): Path<String>) -> impl IntoResponse {
    let store = match state.artifact_store.read() {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "lock poisoned"})),
            )
        }
    };

    match store.get(&id) {
        Ok(Some(artifact)) => {
            // Get CAS metadata for MIME type and size
            let (mime_type, size_bytes) = {
                let cas_hash: Result<cas::ContentHash, _> = artifact.content_hash.as_str().parse();
                match cas_hash.and_then(|h| {
                    state
                        .cas
                        .inspect(&h)
                        .map_err(|_| cas::HashError::InvalidLength(0))
                }) {
                    Ok(Some(r)) => (Some(r.mime_type), Some(r.size_bytes)),
                    _ => (None, None),
                }
            };

            let response = ArtifactMetaResponse {
                id: artifact.id.as_str().to_string(),
                content_hash: artifact.content_hash.as_str().to_string(),
                content_url: format!("/artifact/{}", artifact.id.as_str()),
                mime_type,
                size_bytes,
                creator: artifact.creator.clone(),
                created_at: artifact.created_at.to_rfc3339(),
                tags: artifact.tags.clone(),
                variation_set_id: artifact
                    .variation_set_id
                    .as_ref()
                    .map(|s| s.as_str().to_string()),
                variation_index: artifact.variation_index,
                parent_id: artifact.parent_id.as_ref().map(|s| s.as_str().to_string()),
                access_count: artifact.access_count,
                last_accessed: artifact.last_accessed.map(|t| t.to_rfc3339()),
                metadata: artifact.metadata.clone(),
            };

            (
                StatusCode::OK,
                Json(serde_json::to_value(response).unwrap_or_else(|e| {
                    tracing::error!("Failed to serialize artifact metadata: {}", e);
                    serde_json::json!({"error": "serialization failed"})
                })),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "not found"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// Query parameters for listing artifacts
#[derive(Debug, Deserialize)]
struct ListQuery {
    tag: Option<String>,
    creator: Option<String>,
    limit: Option<usize>,
}

/// Artifact summary for list response
#[derive(Serialize)]
struct ArtifactSummary {
    id: String,
    content_hash: String,
    content_url: String,
    creator: String,
    created_at: String,
    tags: Vec<String>,
    access_count: u64,
}

/// List artifacts with optional filtering
#[tracing::instrument(name = "http.artifacts.list", skip(state))]
async fn list_artifacts(
    State(state): State<WebState>,
    Query(query): Query<ListQuery>,
) -> impl IntoResponse {
    let store = match state.artifact_store.read() {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "lock poisoned"})),
            )
        }
    };

    let all = match store.all() {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        }
    };

    let filtered: Vec<ArtifactSummary> = all
        .into_iter()
        .filter(|a| query.tag.as_ref().is_none_or(|t| a.has_tag(t)))
        .filter(|a| query.creator.as_ref().is_none_or(|c| &a.creator == c))
        .take(query.limit.unwrap_or(100))
        .map(|a| ArtifactSummary {
            id: a.id.as_str().to_string(),
            content_hash: a.content_hash.as_str().to_string(),
            content_url: format!("/artifact/{}", a.id.as_str()),
            creator: a.creator,
            created_at: a.created_at.to_rfc3339(),
            tags: a.tags,
            access_count: a.access_count,
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::to_value(filtered).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize artifact list: {}", e);
            serde_json::json!({"error": "serialization failed"})
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact_store::Artifact;
    use crate::types::{ArtifactId, ContentHash};
    use axum::body::to_bytes;
    use axum::http::Request;
    use tempfile::TempDir;
    use tower::ServiceExt;

    async fn setup_test_state() -> (WebState, TempDir) {
        let temp_dir = TempDir::new().unwrap();

        // Create CAS
        let cas_path = temp_dir.path().join("cas");
        let cas = CasFileStore::at_path(&cas_path).unwrap();

        // Store some content
        let content = b"Hello, artifact world!";
        let hash = cas.store(content, "text/plain").unwrap();

        // Create artifact store
        let artifact_path = temp_dir.path().join("artifacts.json");
        let artifact_store = FileStore::new(&artifact_path).unwrap();

        // Create an artifact pointing to the content
        let artifact = Artifact::new(
            ArtifactId::new("test_artifact"),
            ContentHash::new(hash.as_str()),
            "test_creator",
            serde_json::json!({"test": true}),
        )
        .with_tags(vec!["type:text", "test:yes"]);

        artifact_store.put(artifact).unwrap();
        artifact_store.flush().unwrap();

        let state = WebState {
            artifact_store: Arc::new(RwLock::new(FileStore::new(&artifact_path).unwrap())),
            cas: Arc::new(cas),
        };

        (state, temp_dir)
    }

    #[tokio::test]
    async fn test_download_artifact() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifact/test_artifact")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain"
        );
        assert_eq!(
            response.headers().get("x-artifact-id").unwrap(),
            "test_artifact"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"Hello, artifact world!");
    }

    #[tokio::test]
    async fn test_artifact_meta() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifact/test_artifact/meta")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["id"], "test_artifact");
        assert_eq!(json["creator"], "test_creator");
        assert_eq!(json["mime_type"], "text/plain");
        assert!(json["content_url"]
            .as_str()
            .unwrap()
            .contains("test_artifact"));
    }

    #[tokio::test]
    async fn test_list_artifacts() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifacts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["id"], "test_artifact");
    }

    #[tokio::test]
    async fn test_list_artifacts_with_filter() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        // Filter by tag that exists
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/artifacts?tag=type:text")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 1);

        // Filter by tag that doesn't exist
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifacts?tag=type:audio")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.len(), 0);
    }

    #[tokio::test]
    async fn test_artifact_not_found() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/artifact/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_access_count_increments() {
        let (state, _temp_dir) = setup_test_state().await;
        let app = router(state.clone());

        // First access
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/artifact/test_artifact")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check access count
        let store = state.artifact_store.read().unwrap();
        let artifact = store.get("test_artifact").unwrap().unwrap();
        assert_eq!(artifact.access_count, 1);
        drop(store);

        // Second access
        let _ = app
            .oneshot(
                Request::builder()
                    .uri("/artifact/test_artifact")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let store = state.artifact_store.read().unwrap();
        let artifact = store.get("test_artifact").unwrap().unwrap();
        assert_eq!(artifact.access_count, 2);
    }
}
