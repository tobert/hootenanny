# Task 04: musicgen_generate - Text-to-Music Generation

## Service Details

| Property | Value |
|----------|-------|
| Service Name | musicgen |
| Port | 2006 |
| Model | facebook/musicgen-small |
| Sample Rate | 32kHz (fixed) |
| Max Duration | 30 seconds |
| Output | WAV audio (base64 encoded) |

## API Contract

### Request (POST /predict)

```json
{
  "prompt": "ambient electronic with soft pads",  // text description
  "duration": 10.0,           // seconds (0.5-30.0)
  "temperature": 1.0,         // 0.01-2.0
  "top_k": 250,               // 0-1000
  "top_p": 0.9,               // 0.0-1.0
  "guidance_scale": 3.0,      // 1.0-15.0 (CFG strength)
  "do_sample": true,          // sampling vs greedy
  "client_job_id": "string"
}
```

### Response

```json
{
  "audio_base64": "base64...",   // WAV file
  "sample_rate": 32000,
  "duration": 10.5,              // actual duration
  "num_samples": 336000,
  "channels": 1,
  "prompt": "ambient electronic with soft pads",
  "metadata": {...}
}
```

## Python Service Implementation (Reference)

From `~/src/halfremembered-music-models/services/musicgen/api.py`:

```python
class MusicGenAPI(ModelAPI, ls.LitAPI):
    SAMPLE_RATE = 32000  # Fixed at 32kHz
    TOKENS_PER_SECOND = 50  # Frame rate
    MAX_DURATION = 30.0  # seconds

    def setup(self, device: str):
        super().setup(device)
        check_available_vram(2.0, device)

        from transformers import AutoProcessor, MusicgenForConditionalGeneration

        self.processor = AutoProcessor.from_pretrained("facebook/musicgen-small")
        self.model = MusicgenForConditionalGeneration.from_pretrained(
            "facebook/musicgen-small"
        )
        self.model.to(device)
        self.audio_encoder = AudioEncoder()

    def decode_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
        duration = float(request.get("duration", 10.0))
        duration = max(0.5, min(duration, self.MAX_DURATION))

        temperature = float(request.get("temperature", 1.0))
        temperature = max(0.01, min(temperature, 2.0))

        top_k = int(request.get("top_k", 250))
        top_k = max(0, min(top_k, 1000))

        top_p = float(request.get("top_p", 0.9))
        top_p = max(0.0, min(top_p, 1.0))

        guidance_scale = float(request.get("guidance_scale", 3.0))
        guidance_scale = max(1.0, min(guidance_scale, 15.0))

        return {
            "prompt": request.get("prompt", ""),
            "duration": duration,
            "temperature": temperature,
            "top_k": top_k,
            "top_p": top_p,
            "guidance_scale": guidance_scale,
            "do_sample": bool(request.get("do_sample", True)),
            "client_job_id": self.extract_client_job_id(request),
        }

    def _do_predict(self, x: Dict[str, Any]) -> Dict[str, Any]:
        prompt = x["prompt"]
        duration = x["duration"]
        max_new_tokens = int(duration * self.TOKENS_PER_SECOND)

        inputs = self.processor(
            text=[prompt] if prompt else None,
            padding=True,
            return_tensors="pt",
        )
        inputs = inputs.to(self.device)

        with torch.no_grad():
            audio_values = self.model.generate(
                **inputs,
                max_new_tokens=max_new_tokens,
                do_sample=x["do_sample"],
                temperature=x["temperature"],
                top_k=x["top_k"],
                top_p=x["top_p"],
                guidance_scale=x["guidance_scale"],
            )

        audio = audio_values[0].cpu().numpy()
        # Handle channel dimension...
        audio = np.asarray(audio).flatten()

        audio_b64 = self.audio_encoder.encode_wav(audio, self.SAMPLE_RATE)

        return {
            "audio_base64": audio_b64,
            "sample_rate": self.SAMPLE_RATE,
            "duration": len(audio) / self.SAMPLE_RATE,
            "num_samples": len(audio),
            "channels": 1,
            "prompt": prompt,
            "client_job_id": x.get("client_job_id"),
        }
```

## Implementation Steps

### 1. Add Request Schema (`schema.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MusicgenGenerateRequest {
    #[schemars(description = "Text prompt describing the music to generate")]
    pub prompt: Option<String>,

    #[schemars(description = "Duration in seconds (0.5-30.0, default: 10.0)")]
    pub duration: Option<f32>,

    #[schemars(description = "Sampling temperature (0.01-2.0, default: 1.0)")]
    pub temperature: Option<f32>,

    #[schemars(description = "Top-k filtering (0-1000, default: 250)")]
    pub top_k: Option<u32>,

    #[schemars(description = "Nucleus sampling (0.0-1.0, default: 0.9)")]
    pub top_p: Option<f32>,

    #[schemars(description = "Classifier-free guidance scale (1.0-15.0, default: 3.0). Higher = stronger prompt adherence")]
    pub guidance_scale: Option<f32>,

    #[schemars(description = "Enable sampling vs greedy decoding (default: true)")]
    pub do_sample: Option<bool>,

    // Standard artifact fields
    #[schemars(description = "Optional variation set ID")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID")]
    pub parent_id: Option<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}
```

### 2. Add HTTP Client (`local_models.rs` or new file)

Create a dedicated MusicGen client since it has different parameters:

```rust
pub struct MusicGenClient {
    cas: Cas,
    url: String,
    client: reqwest::Client,
}

impl MusicGenClient {
    pub fn new(cas: Cas, port: u16) -> Self {
        Self {
            cas,
            url: format!("http://127.0.0.1:{}", port),
            client: reqwest::Client::new(),
        }
    }

    pub async fn generate(
        &self,
        prompt: Option<String>,
        duration: Option<f32>,
        temperature: Option<f32>,
        top_k: Option<u32>,
        top_p: Option<f32>,
        guidance_scale: Option<f32>,
        do_sample: Option<bool>,
        client_job_id: Option<String>,
    ) -> Result<MusicGenResult> {
        let request_body = serde_json::json!({
            "prompt": prompt.unwrap_or_default(),
            "duration": duration.unwrap_or(10.0),
            "temperature": temperature.unwrap_or(1.0),
            "top_k": top_k.unwrap_or(250),
            "top_p": top_p.unwrap_or(0.9),
            "guidance_scale": guidance_scale.unwrap_or(3.0),
            "do_sample": do_sample.unwrap_or(true),
            "client_job_id": client_job_id,
        });

        let resp = self.client.post(format!("{}/predict", self.url))
            .json(&request_body)
            .send()
            .await?;

        // Handle 429, parse response...
        let resp_json: serde_json::Value = resp.json().await?;

        // Decode audio and store in CAS
        let audio_b64 = resp_json["audio_base64"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing audio_base64"))?;

        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        let audio_bytes = BASE64.decode(audio_b64)?;

        let hash = self.cas.write(&audio_bytes, "audio/wav")?;

        Ok(MusicGenResult {
            hash,
            sample_rate: resp_json["sample_rate"].as_u64().unwrap_or(32000) as u32,
            duration: resp_json["duration"].as_f64().unwrap_or(0.0) as f32,
            num_samples: resp_json["num_samples"].as_u64().unwrap_or(0),
            channels: 1,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MusicGenResult {
    pub hash: String,
    pub sample_rate: u32,
    pub duration: f32,
    pub num_samples: u64,
    pub channels: u32,
}
```

### 3. Create Tool Implementation (`api/tools/musicgen.rs`)

```rust
use crate::api::schema::MusicgenGenerateRequest;
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use baton::{ErrorData as McpError, CallToolResult, Content};
use std::sync::Arc;

impl EventDualityServer {
    fn validate_musicgen_params(request: &MusicgenGenerateRequest) -> Result<(), McpError> {
        if let Some(duration) = request.duration {
            if !(0.5..=30.0).contains(&duration) {
                return Err(McpError::invalid_params(
                    format!("duration must be 0.5-30.0, got {}", duration)
                ));
            }
        }
        if let Some(temp) = request.temperature {
            if !(0.01..=2.0).contains(&temp) {
                return Err(McpError::invalid_params(
                    format!("temperature must be 0.01-2.0, got {}", temp)
                ));
            }
        }
        if let Some(gs) = request.guidance_scale {
            if !(1.0..=15.0).contains(&gs) {
                return Err(McpError::invalid_params(
                    format!("guidance_scale must be 1.0-15.0, got {}", gs)
                ));
            }
        }
        Ok(())
    }

    #[tracing::instrument(
        name = "mcp.tool.musicgen_generate",
        skip(self, request),
        fields(
            prompt = ?request.prompt,
            duration = ?request.duration,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn musicgen_generate(
        &self,
        request: MusicgenGenerateRequest,
    ) -> Result<CallToolResult, McpError> {
        Self::validate_musicgen_params(&request)?;

        let job_id = self.job_store.create_job("musicgen_generate".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let musicgen_client = Arc::clone(&self.musicgen_client);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            match musicgen_client.generate(
                request.prompt.clone(),
                request.duration,
                request.temperature,
                request.top_k,
                request.top_p,
                request.guidance_scale,
                request.do_sample,
                Some(job_id_clone.as_str().to_string()),
            ).await {
                Ok(result) => {
                    // Create artifact
                    let artifacts_result = (|| -> anyhow::Result<Artifact> {
                        let store = artifact_store.write()
                            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                        let content_hash = ContentHash::new(&result.hash);
                        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                        let creator = request.creator.clone()
                            .unwrap_or_else(|| "agent_musicgen".to_string());

                        let artifact = Artifact::new(
                            artifact_id,
                            content_hash,
                            &creator,
                            serde_json::json!({
                                "prompt": request.prompt,
                                "duration": result.duration,
                                "sample_rate": result.sample_rate,
                                "guidance_scale": request.guidance_scale,
                            })
                        )
                        .with_tags(vec![
                            "type:audio",
                            "format:wav",
                            "phase:generation",
                            "tool:musicgen_generate"
                        ])
                        .with_tags(request.tags.clone());

                        store.put(artifact.clone())?;
                        store.flush()?;
                        Ok(artifact)
                    })();

                    match artifacts_result {
                        Ok(artifact) => {
                            let response = serde_json::json!({
                                "status": "success",
                                "output_hash": result.hash,
                                "artifact_id": artifact.id.as_str(),
                                "duration": result.duration,
                                "sample_rate": result.sample_rate,
                            });
                            let _ = job_store.mark_complete(&job_id_clone, response);
                        }
                        Err(e) => {
                            let _ = job_store.mark_failed(&job_id_clone,
                                format!("Failed to create artifact: {}", e));
                        }
                    }
                }
                Err(e) => {
                    let _ = job_store.mark_failed(&job_id_clone, e.to_string());
                }
            }
        });

        self.job_store.store_handle(&job_id, handle);

        let response = serde_json::json!({
            "job_id": job_id.as_str(),
            "status": "pending",
            "message": "MusicGen generation started. Use job_poll() to retrieve results."
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }
}
```

### 4. Register Tool (`handler.rs`)

```rust
Tool::new("musicgen_generate", "Generate music from text description using MusicGen")
    .with_input_schema(schema_for::<MusicgenGenerateRequest>()),
```

```rust
"musicgen_generate" => {
    let request: MusicgenGenerateRequest = serde_json::from_value(args)
        .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
    self.server.musicgen_generate(request).await
}
```

### 5. Wire Up Client in `service.rs`

Add `musicgen_client: Arc<MusicGenClient>` to `EventDualityServer` and initialize with port 2006.

## Testing

```bash
# Simple generation
musicgen_generate({prompt: "calm ambient piano"})

# With parameters
musicgen_generate({
  prompt: "energetic techno beat with synthesizers",
  duration: 15.0,
  guidance_scale: 5.0,
  temperature: 0.8
})

# Unconditional generation
musicgen_generate({duration: 5.0})
```

## Artifact Tags

- `type:audio`
- `format:wav`
- `phase:generation`
- `tool:musicgen_generate`

## Notes

- Output is **audio** (WAV), not MIDI - different from Orpheus tools
- `guidance_scale` is unique to MusicGen (classifier-free guidance)
- Higher `guidance_scale` = stronger adherence to prompt
- 32kHz mono output
- Duration capped at 30 seconds
