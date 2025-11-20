//! Local Model MCP Tools
//!
//! Tools for interacting with local Orpheus (music) and DeepSeek (code) models.
//! Handles CAS integration automatically.

use crate::cas::Cas;
use crate::domain::CasReference;
use anyhow::{Context, Result};
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
    pub output_hash: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrpheusClassifyResult {
    pub is_human: bool,
    pub confidence: f32,
    pub probabilities: std::collections::HashMap<String, f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeepSeekQueryResult {
    pub text: String,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub role: String,
    pub content: String,
}

// --- Tool Interface ---

// #[rmcp::macros::rpc] // Commenting out until I confirm the path
pub trait LocalModelTools {
    /// Generate or transform music using Orpheus.
    async fn orpheus_generate(
        &self,
        model: String,
        task: String,
        input_hash: Option<String>,
        params: OrpheusGenerateParams,
    ) -> anyhow::Result<OrpheusGenerateResult>;

    /// Analyze MIDI content using Orpheus.
    async fn orpheus_classify(
        &self,
        model: Option<String>,
        input_hash: String,
    ) -> anyhow::Result<OrpheusClassifyResult>;

    /// Query the local DeepSeek Coder model.
    async fn deepseek_query(
        &self,
        model: Option<String>,
        messages: Vec<Message>,
        stream: Option<bool>,
    ) -> anyhow::Result<DeepSeekQueryResult>;

    /// Store a file in CAS manually.
    async fn cas_store(
        &self,
        content_base64: String,
        mime_type: String,
    ) -> anyhow::Result<String>;

    /// Inspect a file in CAS (metadata only).
    async fn cas_inspect(
        &self,
        hash: String,
    ) -> anyhow::Result<CasReference>;
}

// --- Implementation ---

pub struct LocalModels {
    cas: Cas,
    orpheus_url: String,
    deepseek_url: String,
    client: reqwest::Client,
}

impl LocalModels {
    pub fn new(cas: Cas, orpheus_port: u16, deepseek_port: u16) -> Self {
        Self {
            cas,
            orpheus_url: format!("http://127.0.0.1:{}", orpheus_port),
            deepseek_url: format!("http://127.0.0.1:{}", deepseek_port),
            client: reqwest::Client::new(),
        }
    }

    // Helper to resolve CAS hash to bytes
    fn resolve_cas(&self, hash: &str) -> Result<Vec<u8>> {
        self.cas.read(hash)?.context("CAS object not found")
    }

    // Helper to store bytes to CAS
    fn store_cas(&self, data: &[u8], mime_type: &str) -> Result<String> {
        self.cas.write(data, mime_type)
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
            let flags = if span_context.is_sampled() { "01" } else { "00" };

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
    pub async fn store_cas_content(
        &self,
        content: &[u8],
        mime_type: &str,
    ) -> Result<String> {
        self.cas.write(content, mime_type)
            .context("Failed to store content in CAS")
    }

    pub async fn inspect_cas_content(
        &self,
        hash: &str,
    ) -> Result<CasReference> {
        self.cas.inspect(hash)?
            .ok_or_else(|| anyhow::anyhow!("CAS object with hash {} not found", hash))
    }

    pub async fn run_orpheus_generate(
        &self,
        model: String,
        task: String,
        input_hash: Option<String>,
        params: OrpheusGenerateParams,
    ) -> Result<OrpheusGenerateResult> {
        let mut request_body = serde_json::Map::new();
        request_body.insert("model".to_string(), serde_json::json!(model));
        request_body.insert("task".to_string(), serde_json::json!(task));

        if let Some(temp) = params.temperature {
            request_body.insert("temperature".to_string(), serde_json::json!(temp));
        }
        if let Some(top_p) = params.top_p {
            request_body.insert("top_p".to_string(), serde_json::json!(top_p));
        }
        if let Some(max) = params.max_tokens {
            request_body.insert("max_tokens".to_string(), serde_json::json!(max));
        }
        if let Some(num_var) = params.num_variations {
            request_body.insert("num_variations".to_string(), serde_json::json!(num_var));
        }

        if let Some(hash) = input_hash {
            let midi_bytes = self.resolve_cas(&hash)?;
            // Convert raw bytes to base64 for API
            use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
            let b64_midi = BASE64.encode(midi_bytes);
            request_body.insert("midi_input".to_string(), serde_json::json!(b64_midi));
        }

        let builder = self.client.post(format!("{}/predict", self.orpheus_url))
            .json(&request_body);
        let builder = self.inject_trace_context(builder);
        let resp = builder.send()
            .await
            .context("Failed to call Orpheus API")?;

        let status = resp.status();
        if !status.is_success() {
            // Capture error response body for better debugging
            let error_body = resp.text().await.unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("Orpheus API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp.json().await
            .context("Failed to parse Orpheus response as JSON")?;
        
        // Extract MIDI output
        if let Some(midi_b64) = resp_json.get("midi_base64").and_then(|v| v.as_str()) {
             use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
             let midi_bytes = BASE64.decode(midi_b64).context("Failed to decode API output")?;
             
             let hash = self.store_cas(&midi_bytes, "audio/midi")?;
             
             let token_count = resp_json.get("num_tokens").and_then(|v| v.as_u64()).unwrap_or(0);

             Ok(OrpheusGenerateResult {
                 status: "success".to_string(),
                 output_hash: hash,
                 summary: format!("Generated {} tokens", token_count),
             })
        } else {
            anyhow::bail!("No MIDI output in response");
        }
    }

     pub async fn run_orpheus_classify(
        &self,
        model: Option<String>,
        input_hash: String,
    ) -> Result<OrpheusClassifyResult> {
        let midi_bytes = self.resolve_cas(&input_hash)?;
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        let b64_midi = BASE64.encode(midi_bytes);

        let request_body = serde_json::json!({
            "model": model.unwrap_or_else(|| "classifier".to_string()),
            "task": "classify",
            "midi_input": b64_midi
        });

        let builder = self.client.post(format!("{}/predict", self.orpheus_url))
            .json(&request_body);
        let builder = self.inject_trace_context(builder);
        let resp = builder.send()
            .await
            .context("Failed to call Orpheus API")?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("Orpheus API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp.json().await
            .context("Failed to parse Orpheus response as JSON")?;

        if let Some(classification) = resp_json.get("classification") {
             let is_human = classification.get("is_human").and_then(|v| v.as_bool()).unwrap_or(false);
             let confidence = classification.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

             let probabilities = classification.get("probabilities")
                .ok_or_else(|| anyhow::anyhow!("Missing 'probabilities' field in classification"))?;
             let probs: std::collections::HashMap<String, f32> = serde_json::from_value(probabilities.clone())
                .context("Failed to parse probabilities map")?;

             Ok(OrpheusClassifyResult {
                 is_human,
                 confidence,
                 probabilities: probs,
             })
        } else {
            anyhow::bail!("Invalid classification response");
        }
    }

    pub async fn run_deepseek_query(
        &self,
        model: Option<String>,
        messages: Vec<Message>,
        stream: Option<bool>,
    ) -> Result<DeepSeekQueryResult> {
         let request_body = serde_json::json!({
            "messages": messages,
            "model": model.unwrap_or_else(|| "deepseek-coder-v2-lite".to_string()),
            "stream": stream.unwrap_or(false)
        });

        // Note: For PoC we only handle non-streaming /predict endpoint even if stream is requested
        // as implementing SSE client here is complex.
        let builder = self.client.post(format!("{}/predict", self.deepseek_url))
            .json(&request_body);
        let builder = self.inject_trace_context(builder);
        let resp = builder.send()
            .await
            .context("Failed to call DeepSeek API")?;

        let status = resp.status();
        if !status.is_success() {
            let error_body = resp.text().await.unwrap_or_else(|_| "<failed to read error body>".to_string());
            anyhow::bail!("DeepSeek API error {}: {}", status, error_body);
        }

        let resp_json: serde_json::Value = resp.json().await
            .context("Failed to parse DeepSeek response as JSON")?;

        let text = resp_json.get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' field in DeepSeek response"))?
            .to_string();

        Ok(DeepSeekQueryResult {
            text,
            finish_reason: Some("stop".to_string()),
        })
    }
}
