//! Web endpoints for Hootenanny.
//!
//! Provides HTTP access to CAS and other resources.

use crate::cas::Cas;
use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
    body::Body,
};
use std::sync::Arc;
use tokio_util::io::ReaderStream;

pub fn router(cas: Cas) -> Router {
    Router::new()
        .route("/cas", post(upload_cas))
        .route("/cas/:hash", get(download_cas))
        .with_state(Arc::new(cas))
}

async fn upload_cas(
    State(cas): State<Arc<Cas>>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    match cas.write(&body) {
        Ok(hash) => (StatusCode::OK, hash).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn download_cas(
    State(cas): State<Arc<Cas>>,
    Path(hash): Path<String>,
) -> Response {
    match cas.get_path(&hash) {
        Ok(Some(path)) => {
            match tokio::fs::File::open(path).await {
                Ok(file) => {
                    let stream = ReaderStream::new(file);
                    let body = Body::from_stream(stream);
                    
                    // Determine mime type? For now generic.
                    // In a real implementation, we might store mime type in a separate DB or extended attributes.
                    // Or the client should know what it asked for.
                    // We'll rely on the client to handle it for now.
                    ([(header::CONTENT_TYPE, "application/octet-stream")], body).into_response()
                }
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}
