//! Local Model MCP Tools
//!
//! Tools for interacting with local Orpheus (music) and DeepSeek (code) models.
//! Handles CAS integration automatically.

use anyhow::{Context, Result};
use cas::{CasReference, ContentHash, ContentStore, FileStore};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// --- Data Structures ---

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrpheusGenerateParams {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub num_variations: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrpheusGenerateResult {
    pub status: String,
    pub output_hashes: Vec<String>,
    pub num_tokens: Vec<u64>,
    pub summary: String,
}

// --- Tool Interface ---

// #[rmcp::macros::rpc] // Commenting out until I confirm the path
#[allow(dead_code, async_fn_in_trait)]
pub trait LocalModelTools {
    /// Generate or transform music using Orpheus.
    async fn orpheus_generate(
        &self,
        model: String,
        task: String,
        input_hash: Option<String>,
        params: OrpheusGenerateParams,
    ) -> anyhow::Result<OrpheusGenerateResult>;

    /// Store a file in CAS manually.
    async fn cas_store(&self, content_base64: String, mime_type: String) -> anyhow::Result<String>;

    /// Inspect a file in CAS (metadata only).
    async fn cas_inspect(&self, hash: String) -> anyhow::Result<CasReference>;
}

// --- Implementation ---

pub struct LocalModels {
    cas: FileStore,
    orpheus_url: String,
    client: reqwest::Client,
}

impl LocalModels {
    pub fn new(cas: FileStore, orpheus_port: u16) -> Self {
        Self {
            cas,
            orpheus_url: format!("http://127.0.0.1:{}", orpheus_port),
            client: reqwest::Client::new(),
        }
    }

    /// Get the base path of the CAS directory
    pub fn cas_base_path(&self) -> std::path::PathBuf {
        self.cas.config().base_path.clone()
    }

    // Helper to resolve CAS hash to bytes
    fn resolve_cas(&self, hash: &str) -> Result<Vec<u8>> {
        let content_hash: ContentHash = hash.parse().context("Invalid hash format")?;
        self.cas
            .retrieve(&content_hash)?
            .context("CAS object not found")
    }

    // Helper to store bytes to CAS
    fn store_cas(&self, data: &[u8], mime_type: &str) -> Result<String> {
        let hash = self.cas.store(data, mime_type)?;
        Ok(hash.into_inner())
    }

    // Helper to inject traceparent header for distributed tracing
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
}

// We need to manually implement the trait logic for now as we are defining the interface.
// In a real server, this would be wired up to the rpc macro.
// Here is the logic that would go inside the trait implementation.

impl LocalModels {
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
        self.resolve_cas(hash)
    }

    pub async fn run_orpheus_generate(
        &self,
        model: String,
        task: String,
        input_hash: Option<String>,
        params: OrpheusGenerateParams,
        client_job_id: Option<String>,
    ) -> Result<OrpheusGenerateResult> {
        let mut request_body = serde_json::Map::new();
        request_body.insert("model".to_string(), serde_json::json!(model));
        request_body.insert("task".to_string(), serde_json::json!(task));

        // Always send these values, using defaults when None to ensure Python receives them
        request_body.insert(
            "temperature".to_string(),
            serde_json::json!(params.temperature.unwrap_or(1.0)),
        );
        request_body.insert(
            "top_p".to_string(),
            serde_json::json!(params.top_p.unwrap_or(0.95)),
        );
        request_body.insert(
            "max_tokens".to_string(),
            serde_json::json!(params.max_tokens.unwrap_or(1024)),
        );
        request_body.insert(
            "num_variations".to_string(),
            serde_json::json!(params.num_variations.unwrap_or(1)),
        );

        if let Some(job_id) = client_job_id {
            request_body.insert("client_job_id".to_string(), serde_json::json!(job_id));
        }

        if let Some(hash) = input_hash {
            let midi_bytes = self.resolve_cas(&hash)?;
            // Convert raw bytes to base64 for API
            use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
            let b64_midi = BASE64.encode(midi_bytes);
            request_body.insert("midi_input".to_string(), serde_json::json!(b64_midi));
        }

        let builder = self
            .client
            .post(format!("{}/predict", self.orpheus_url))
            .json(&request_body);
        let builder = self.inject_trace_context(builder);
        let resp = builder.send().await.context("Failed to call Orpheus API")?;

        let status = resp.status();

        // Handle HTTP 429 - GPU busy, retry
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);

            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());

            tracing::warn!(
                retry_after = retry_after,
                error_body = ?error_body,
                "GPU busy, retrying after {}s",
                retry_after
            );

            anyhow::bail!("GPU busy, retry after {}s", retry_after);
        }

        if !status.is_success() {
            // Capture error response body for better debugging
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("Orpheus API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse Orpheus response as JSON")?;

        // Extract variations array from new API format
        if let Some(variations) = resp_json.get("variations").and_then(|v| v.as_array()) {
            use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

            let mut output_hashes = Vec::new();
            let mut num_tokens_list = Vec::new();

            for variation in variations {
                if let Some(midi_b64) = variation.get("midi_base64").and_then(|v| v.as_str()) {
                    let midi_bytes = BASE64
                        .decode(midi_b64)
                        .context("Failed to decode variation MIDI")?;

                    let hash = self.store_cas(&midi_bytes, "audio/midi")?;
                    output_hashes.push(hash);

                    let tokens = variation
                        .get("num_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    num_tokens_list.push(tokens);
                } else {
                    anyhow::bail!("Variation missing midi_base64 field");
                }
            }

            let total_tokens: u64 = num_tokens_list.iter().sum();
            let summary = if output_hashes.len() == 1 {
                format!("Generated {} tokens", total_tokens)
            } else {
                format!(
                    "Generated {} variations ({} tokens total)",
                    output_hashes.len(),
                    total_tokens
                )
            };

            Ok(OrpheusGenerateResult {
                status: "success".to_string(),
                output_hashes,
                num_tokens: num_tokens_list,
                summary,
            })
        } else {
            anyhow::bail!("No variations array in response (expected new API format)");
        }
    }

    /// Call the Orpheus bridge service (port 2002) to create transitions between MIDI sections.
    pub async fn run_orpheus_bridge(
        &self,
        section_a_hash: String,
        section_b_hash: Option<String>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
        client_job_id: Option<String>,
    ) -> Result<OrpheusGenerateResult> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        let section_a_bytes = self.resolve_cas(&section_a_hash)?;

        let mut request_body = serde_json::Map::new();
        request_body.insert(
            "section_a".to_string(),
            serde_json::json!(BASE64.encode(&section_a_bytes)),
        );

        if let Some(ref hash) = section_b_hash {
            let section_b_bytes = self.resolve_cas(hash)?;
            request_body.insert(
                "section_b".to_string(),
                serde_json::json!(BASE64.encode(&section_b_bytes)),
            );
        }

        request_body.insert(
            "temperature".to_string(),
            serde_json::json!(temperature.unwrap_or(1.0)),
        );
        request_body.insert(
            "top_p".to_string(),
            serde_json::json!(top_p.unwrap_or(0.95)),
        );
        request_body.insert(
            "max_tokens".to_string(),
            serde_json::json!(max_tokens.unwrap_or(1024)),
        );

        if let Some(job_id) = client_job_id {
            request_body.insert("client_job_id".to_string(), serde_json::json!(job_id));
        }

        let builder = self
            .client
            .post("http://127.0.0.1:2002/predict")
            .json(&request_body);
        let builder = self.inject_trace_context(builder);

        let resp = match builder.send().await {
            Ok(r) => r,
            Err(e) if e.is_connect() => {
                anyhow::bail!(
                    "Bridge service unavailable at port 2002 - is it running? Error: {}",
                    e
                )
            }
            Err(e) if e.is_timeout() => {
                anyhow::bail!("Bridge service timeout")
            }
            Err(e) => anyhow::bail!("HTTP error calling bridge service: {}", e),
        };

        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);

            anyhow::bail!("GPU busy, retry after {}s", retry_after);
        }

        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("Bridge API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse bridge response as JSON")?;

        if let Some(variations) = resp_json.get("variations").and_then(|v| v.as_array()) {
            let mut output_hashes = Vec::new();
            let mut num_tokens_list = Vec::new();

            for variation in variations {
                if let Some(midi_b64) = variation.get("midi_base64").and_then(|v| v.as_str()) {
                    let midi_bytes = BASE64
                        .decode(midi_b64)
                        .context("Failed to decode bridge MIDI")?;

                    let hash = self.store_cas(&midi_bytes, "audio/midi")?;
                    output_hashes.push(hash);

                    let tokens = variation
                        .get("num_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    num_tokens_list.push(tokens);
                } else {
                    anyhow::bail!("Variation missing midi_base64 field");
                }
            }

            let total_tokens: u64 = num_tokens_list.iter().sum();
            let summary = format!("Generated bridge ({} tokens)", total_tokens);

            Ok(OrpheusGenerateResult {
                status: "success".to_string(),
                output_hashes,
                num_tokens: num_tokens_list,
                summary,
            })
        } else {
            anyhow::bail!("No variations array in bridge response");
        }
    }

    /// Call the Orpheus classifier service (port 2001) to classify MIDI
    pub async fn run_orpheus_classifier(&self, midi_hash: String) -> Result<serde_json::Value> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        let midi_bytes = self.resolve_cas(&midi_hash)?;

        let mut request_body = serde_json::Map::new();
        request_body.insert(
            "midi_input".to_string(),
            serde_json::json!(BASE64.encode(&midi_bytes)),
        );

        let builder = self
            .client
            .post("http://127.0.0.1:2001/predict")
            .json(&request_body);
        let builder = self.inject_trace_context(builder);

        let resp = match builder.send().await {
            Ok(r) => r,
            Err(e) if e.is_connect() => {
                anyhow::bail!(
                    "Classifier service unavailable at port 2001 - is it running? Error: {}",
                    e
                )
            }
            Err(e) if e.is_timeout() => {
                anyhow::bail!("Classifier service timeout")
            }
            Err(e) => anyhow::bail!("HTTP error calling classifier service: {}", e),
        };

        let status = resp.status();

        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("Classifier API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse classifier response as JSON")?;

        Ok(resp_json)
    }

    /// Call the Orpheus loops service (port 2003) to generate drum loops
    pub async fn run_orpheus_loops(
        &self,
        seed_hash: Option<String>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        client_job_id: Option<String>,
    ) -> Result<OrpheusGenerateResult> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

        let mut request_body = serde_json::Map::new();

        if let Some(ref hash) = seed_hash {
            let seed_bytes = self.resolve_cas(hash)?;
            request_body.insert(
                "seed".to_string(),
                serde_json::json!(BASE64.encode(&seed_bytes)),
            );
        }

        request_body.insert(
            "temperature".to_string(),
            serde_json::json!(temperature.unwrap_or(1.0)),
        );
        request_body.insert(
            "top_p".to_string(),
            serde_json::json!(top_p.unwrap_or(0.95)),
        );
        request_body.insert(
            "max_tokens".to_string(),
            serde_json::json!(max_tokens.unwrap_or(1024)),
        );
        request_body.insert(
            "num_variations".to_string(),
            serde_json::json!(num_variations.unwrap_or(1) as i64),
        );

        if let Some(job_id) = client_job_id {
            request_body.insert("client_job_id".to_string(), serde_json::json!(job_id));
        }

        let builder = self
            .client
            .post("http://127.0.0.1:2003/predict")
            .json(&request_body);
        let builder = self.inject_trace_context(builder);

        let resp = match builder.send().await {
            Ok(r) => r,
            Err(e) if e.is_connect() => {
                anyhow::bail!(
                    "Loops service unavailable at port 2003 - is it running? Error: {}",
                    e
                )
            }
            Err(e) if e.is_timeout() => {
                anyhow::bail!("Loops service timeout")
            }
            Err(e) => anyhow::bail!("HTTP error calling loops service: {}", e),
        };

        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);

            anyhow::bail!("GPU busy, retry after {}s", retry_after);
        }

        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("Loops API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse loops response as JSON")?;

        if let Some(variations) = resp_json.get("variations").and_then(|v| v.as_array()) {
            let mut output_hashes = Vec::new();
            let mut num_tokens_list = Vec::new();

            for variation in variations {
                if let Some(midi_b64) = variation.get("midi_base64").and_then(|v| v.as_str()) {
                    let midi_bytes = BASE64
                        .decode(midi_b64)
                        .context("Failed to decode loops MIDI")?;

                    let hash = self.store_cas(&midi_bytes, "audio/midi")?;
                    output_hashes.push(hash);

                    let tokens = variation
                        .get("num_tokens")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as u64;
                    num_tokens_list.push(tokens);
                }
            }

            let summary = format!(
                "Generated {} drum loop variations ({} tokens total)",
                output_hashes.len(),
                num_tokens_list.iter().sum::<u64>()
            );

            Ok(OrpheusGenerateResult {
                status: "success".to_string(),
                output_hashes,
                num_tokens: num_tokens_list,
                summary,
            })
        } else {
            anyhow::bail!("No variations array in loops response");
        }
    }

    /// Call MusicGen service (port 2006) to generate music from text prompt
    #[allow(clippy::too_many_arguments)]
    pub async fn run_musicgen_generate(
        &self,
        prompt: String,
        duration: f32,
        temperature: f32,
        top_k: u32,
        top_p: f32,
        guidance_scale: f32,
        do_sample: bool,
        client_job_id: Option<String>,
    ) -> Result<serde_json::Value> {
        let url = "http://127.0.0.1:2006/predict";

        let body = serde_json::json!({
            "prompt": prompt,
            "duration": duration,
            "temperature": temperature,
            "top_k": top_k,
            "top_p": top_p,
            "guidance_scale": guidance_scale,
            "do_sample": do_sample,
            "client_job_id": client_job_id,
        });

        let builder = self
            .client
            .post(url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(120));

        let builder = self.inject_trace_context(builder);
        let resp = builder
            .send()
            .await
            .context("Failed to call MusicGen API")?;

        let status = resp.status();

        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "Service busy".to_string());
            anyhow::bail!("MusicGen service busy: {}", error_body);
        }

        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("MusicGen API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse MusicGen response as JSON")?;

        Ok(resp_json)
    }

    /// Call CLAP service (port 2007) to analyze audio
    pub async fn run_clap_analyze(
        &self,
        audio_base64: String,
        tasks: Vec<String>,
        audio_b_base64: Option<String>,
        text_candidates: Option<Vec<String>>,
        client_job_id: Option<String>,
    ) -> Result<serde_json::Value> {
        let url = "http://127.0.0.1:2007/predict";

        let body = serde_json::json!({
            "audio": audio_base64,
            "tasks": tasks,
            "audio_b": audio_b_base64,
            "text_candidates": text_candidates,
            "client_job_id": client_job_id,
        });

        let builder = self
            .client
            .post(url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(60));

        let builder = self.inject_trace_context(builder);
        let resp = builder.send().await.context("Failed to call CLAP API")?;

        let status = resp.status();

        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "Service busy".to_string());
            anyhow::bail!("CLAP service busy: {}", error_body);
        }

        if !status.is_success() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("CLAP API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse CLAP response as JSON")?;

        Ok(resp_json)
    }

    /// Call YuE service (port 2008) to generate song from lyrics
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
            .timeout(std::time::Duration::from_secs(600)); // YuE is very slow!

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
