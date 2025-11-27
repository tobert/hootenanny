//! Web endpoints for Hootenanny.
//!
//! Provides HTTP access to CAS and other resources.

use crate::cas::Cas;
use axum::{
    extract::{Path, State},
    http::{StatusCode, header, HeaderMap},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
    body::Body,
};
use std::sync::Arc;
use tokio_util::io::ReaderStream;

pub mod mcp;
pub mod state;

pub fn router(cas: Cas) -> Router {
    Router::new()
        .route("/cas", post(upload_cas))
        .route("/cas/{hash}", get(download_cas))
        .with_state(Arc::new(cas))
}

async fn upload_cas(
    State(cas): State<Arc<Cas>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Extract MIME type from Content-Type header, default to application/octet-stream
    let mime_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    match cas.write(&body, mime_type) {
        Ok(hash) => (StatusCode::OK, hash).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn download_cas(
    State(cas): State<Arc<Cas>>,
    Path(hash): Path<String>,
) -> Response {
    // Get metadata first to retrieve MIME type
    let mime_type = match cas.inspect(&hash) {
        Ok(Some(cas_ref)) => cas_ref.mime_type,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    match cas.get_path(&hash) {
        Ok(Some(path)) => {
            match tokio::fs::File::open(path).await {
                Ok(file) => {
                    let stream = ReaderStream::new(file);
                    let body = Body::from_stream(stream);

                    // Use stored MIME type from CAS metadata
                    ([(header::CONTENT_TYPE, mime_type.as_str())], body).into_response()
                }
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            }
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}
