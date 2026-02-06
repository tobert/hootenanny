//! CAS helpers and HTTP model clients
//!
//! Provides CAS integration and HTTP clients for model services
//! that haven't been migrated to ZMQ yet (currently just YuE).

use anyhow::{Context, Result};
use cas::{CasReference, ContentHash, ContentStore, FileStore};

pub struct LocalModels {
    cas: FileStore,
    client: reqwest::Client,
}

impl LocalModels {
    pub fn new(cas: FileStore) -> Self {
        Self {
            cas,
            client: reqwest::Client::new(),
        }
    }

    pub fn cas_base_path(&self) -> std::path::PathBuf {
        self.cas.config().base_path.clone()
    }

    pub async fn store_cas_content(&self, content: &[u8], mime_type: &str) -> Result<String> {
        let hash = self
            .cas
            .store(content, mime_type)
            .context("Failed to store content in CAS")?;
        Ok(hash.into_inner())
    }

    pub async fn inspect_cas_content(&self, hash: &str) -> Result<CasReference> {
        let content_hash: ContentHash = hash.parse().context("Invalid hash format")?;
        self.cas
            .inspect(&content_hash)?
            .ok_or_else(|| anyhow::anyhow!("CAS object with hash {} not found", hash))
    }

    #[allow(dead_code)]
    pub async fn read_cas_content(&self, hash: &str) -> Result<Vec<u8>> {
        let content_hash: ContentHash = hash.parse().context("Invalid hash format")?;
        self.cas
            .retrieve(&content_hash)?
            .context("CAS object not found")
    }

    fn inject_trace_context(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        let span = tracing::Span::current();
        let context = span.context();
        let ctx_span = context.span();
        let span_context = ctx_span.span_context();

        if span_context.is_valid() {
            let trace_id = span_context.trace_id();
            let span_id = span_context.span_id();
            let flags = if span_context.is_sampled() {
                "01"
            } else {
                "00"
            };

            let traceparent = format!("00-{}-{}-{}", trace_id, span_id, flags);
            builder.header("traceparent", traceparent)
        } else {
            builder
        }
    }

    /// Call YuE service (HTTP) to generate song from lyrics.
    /// YuE stays on HTTP: slow generation makes ZMQ latency irrelevant.
    pub async fn run_yue_generate(
        &self,
        lyrics: String,
        genre: String,
        max_new_tokens: u32,
        run_n_segments: u32,
        seed: u64,
        client_job_id: Option<String>,
    ) -> Result<serde_json::Value> {
        let url = "http://127.0.0.1:2008/predict";

        let body = serde_json::json!({
            "lyrics": lyrics,
            "genre": genre,
            "max_new_tokens": max_new_tokens,
            "run_n_segments": run_n_segments,
            "seed": seed,
            "client_job_id": client_job_id,
        });

        let builder = self
            .client
            .post(url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(600));

        let builder = self.inject_trace_context(builder);
        let resp = builder.send().await.context("Failed to call YuE API")?;

        let status = resp.status();

        if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "Validation error".to_string());
            anyhow::bail!("YuE validation error: {}", error_body);
        }

        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "Service busy".to_string());
            anyhow::bail!("YuE service busy: {}", error_body);
        }

        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("YuE API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse YuE response as JSON")?;

        Ok(resp_json)
    }
}
