# Task 06: anticipatory_* - Stanford Anticipatory Music Transformer

## Service Details

| Property | Value |
|----------|-------|
| Service Name | anticipatory |
| Port | 2011 |
| Models | stanford-crfm/music-{small,medium,large}-800k |
| Hidden Dim | 768 |
| Max Sequence | 1024 tokens |
| Output | MIDI or embeddings |

## Tasks

The anticipatory service provides **three tasks**:

1. **generate** - Generate music from scratch
2. **continue** - Continue from existing MIDI
3. **embed** - Extract hidden state embeddings

## API Contract

### Generate Request

```json
{
  "task": "generate",
  "length_seconds": 20.0,
  "top_p": 0.95,
  "num_variations": 1,
  "model_size": "small",
  "client_job_id": "string"
}
```

### Continue Request

```json
{
  "task": "continue",
  "midi_input": "base64...",
  "prime_seconds": 5.0,
  "length_seconds": 20.0,
  "top_p": 0.95,
  "num_variations": 1,
  "model_size": "small"
}
```

### Embed Request

```json
{
  "task": "embed",
  "midi_input": "base64...",
  "embed_layer": -3,
  "model_size": "small"
}
```

### Response (generate/continue)

```json
{
  "task": "generate",
  "variations": [
    {
      "midi_base64": "base64...",
      "num_events": 1234,
      "duration_seconds": 20.0
    }
  ],
  "metadata": {...}
}
```

### Response (embed)

```json
{
  "task": "embed",
  "embedding": [0.1, -0.2, ...],  // 768 floats
  "embedding_dim": 768,
  "layer": -3,
  "num_tokens": 512,
  "metadata": {...}
}
```

## Python Service Implementation (Reference)

From `~/src/halfremembered-music-models/services/anticipatory/api.py`:

```python
MODEL_CONFIGS = {
    "small": "stanford-crfm/music-small-800k",
    "medium": "stanford-crfm/music-medium-800k",
    "large": "stanford-crfm/music-large-800k",
}

HIDDEN_DIM = 768
MAX_SEQ_LEN = 1024
DEFAULT_TOP_P = 0.95
DEFAULT_LENGTH = 20.0
EMBED_LAYER = -3  # Layer 10 of 12

class AnticipatoryAPI(ModelAPI, ls.LitAPI):
    def _generate(self, model, x: Dict[str, Any]) -> Dict[str, Any]:
        from anticipation.sample import generate

        results = []
        for i in range(x["num_variations"]):
            events = generate(
                model,
                start_time=0,
                end_time=x["length_seconds"],
                top_p=x["top_p"]
            )

            midi_bytes = self._events_to_bytes(events)
            results.append({
                "midi_base64": base64.b64encode(midi_bytes).decode(),
                "num_events": len(events),
                "duration_seconds": x["length_seconds"],
            })

        return {"task": "generate", "variations": results, ...}

    def _continue(self, model, x: Dict[str, Any]) -> Dict[str, Any]:
        from anticipation.sample import generate
        from anticipation import ops

        source_events = self._bytes_to_events(x["midi_input"])
        prime_events = ops.clip(source_events, 0, x["prime_seconds"])
        total_length = x["prime_seconds"] + x["length_seconds"]

        results = []
        for i in range(x["num_variations"]):
            events = generate(
                model,
                start_time=0,
                end_time=total_length,
                inputs=prime_events,
                top_p=x["top_p"]
            )
            # ...

    def _embed(self, model, x: Dict[str, Any]) -> Dict[str, Any]:
        tokens = list(self._bytes_to_events(x["midi_input"]))
        tokens = tokens[:MAX_SEQ_LEN]

        input_ids = torch.LongTensor([tokens]).to(self.device)
        with torch.no_grad():
            outputs = model(input_ids, output_hidden_states=True)

        hidden = outputs.hidden_states[x["embed_layer"]]
        embedding = hidden.mean(dim=1).squeeze(0).cpu().tolist()

        return {
            "task": "embed",
            "embedding": embedding,
            "embedding_dim": len(embedding),
            "layer": x["embed_layer"],
            "num_tokens": len(tokens),
            ...
        }
```

## Implementation Steps

### 1. Add Request Schemas (`schema.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AnticipatoryGenerateRequest {
    #[schemars(description = "Duration in seconds to generate (1.0-120.0, default: 20.0)")]
    pub length_seconds: Option<f32>,

    #[schemars(description = "Nucleus sampling (0.1-1.0, default: 0.95)")]
    pub top_p: Option<f32>,

    #[schemars(description = "Number of variations (1-5, default: 1)")]
    pub num_variations: Option<u32>,

    #[schemars(description = "Model size: 'small', 'medium', or 'large' (default: 'small')")]
    pub model_size: Option<String>,

    // Standard artifact fields...
    pub variation_set_id: Option<String>,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AnticipatoryContinueRequest {
    #[schemars(description = "CAS hash of MIDI to continue (required)")]
    pub midi_hash: String,

    #[schemars(description = "Seconds of input to use as prime (1.0-60.0, default: 5.0)")]
    pub prime_seconds: Option<f32>,

    #[schemars(description = "Seconds of new music to generate (1.0-120.0, default: 20.0)")]
    pub length_seconds: Option<f32>,

    #[schemars(description = "Nucleus sampling (0.1-1.0, default: 0.95)")]
    pub top_p: Option<f32>,

    #[schemars(description = "Number of variations (1-5, default: 1)")]
    pub num_variations: Option<u32>,

    #[schemars(description = "Model size: 'small', 'medium', or 'large' (default: 'small')")]
    pub model_size: Option<String>,

    // Standard artifact fields...
    pub variation_set_id: Option<String>,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AnticipatoryEmbedRequest {
    #[schemars(description = "CAS hash of MIDI to embed (required)")]
    pub midi_hash: String,

    #[schemars(description = "Hidden layer to extract (-12 to -1, default: -3 = layer 10)")]
    pub embed_layer: Option<i32>,

    #[schemars(description = "Model size: 'small', 'medium', or 'large' (default: 'small')")]
    pub model_size: Option<String>,
}
```

### 2. Add HTTP Client

```rust
pub struct AnticipatoryClient {
    cas: Cas,
    url: String,
    client: reqwest::Client,
}

impl AnticipatoryClient {
    pub fn new(cas: Cas, port: u16) -> Self {
        Self {
            cas,
            url: format!("http://127.0.0.1:{}", port),
            client: reqwest::Client::new(),
        }
    }

    pub async fn generate(
        &self,
        length_seconds: f32,
        top_p: f32,
        num_variations: u32,
        model_size: String,
        client_job_id: Option<String>,
    ) -> Result<Vec<AnticipatoryMidiResult>> {
        let request_body = serde_json::json!({
            "task": "generate",
            "length_seconds": length_seconds,
            "top_p": top_p,
            "num_variations": num_variations,
            "model_size": model_size,
            "client_job_id": client_job_id,
        });

        let resp = self.client.post(format!("{}/predict", self.url))
            .json(&request_body)
            .send()
            .await?;

        let resp_json: serde_json::Value = resp.json().await?;

        // Parse variations, store in CAS, return results
        self.parse_midi_variations(&resp_json).await
    }

    pub async fn continue_midi(
        &self,
        midi_hash: String,
        prime_seconds: f32,
        length_seconds: f32,
        top_p: f32,
        num_variations: u32,
        model_size: String,
        client_job_id: Option<String>,
    ) -> Result<Vec<AnticipatoryMidiResult>> {
        let midi_bytes = self.cas.read(&midi_hash)?
            .ok_or_else(|| anyhow::anyhow!("MIDI not found in CAS"))?;

        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

        let request_body = serde_json::json!({
            "task": "continue",
            "midi_input": BASE64.encode(&midi_bytes),
            "prime_seconds": prime_seconds,
            "length_seconds": length_seconds,
            "top_p": top_p,
            "num_variations": num_variations,
            "model_size": model_size,
            "client_job_id": client_job_id,
        });

        let resp = self.client.post(format!("{}/predict", self.url))
            .json(&request_body)
            .send()
            .await?;

        let resp_json: serde_json::Value = resp.json().await?;
        self.parse_midi_variations(&resp_json).await
    }

    pub async fn embed(
        &self,
        midi_hash: String,
        embed_layer: i32,
        model_size: String,
    ) -> Result<AnticipatoryEmbedResult> {
        let midi_bytes = self.cas.read(&midi_hash)?
            .ok_or_else(|| anyhow::anyhow!("MIDI not found in CAS"))?;

        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

        let request_body = serde_json::json!({
            "task": "embed",
            "midi_input": BASE64.encode(&midi_bytes),
            "embed_layer": embed_layer,
            "model_size": model_size,
        });

        let resp = self.client.post(format!("{}/predict", self.url))
            .json(&request_body)
            .send()
            .await?;

        let resp_json: serde_json::Value = resp.json().await?;

        let embedding: Vec<f32> = resp_json["embedding"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing embedding"))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        Ok(AnticipatoryEmbedResult {
            embedding,
            embedding_dim: resp_json["embedding_dim"].as_u64().unwrap_or(768) as usize,
            layer: embed_layer,
            num_tokens: resp_json["num_tokens"].as_u64().unwrap_or(0) as usize,
            truncated: resp_json["truncated"].as_bool().unwrap_or(false),
        })
    }

    async fn parse_midi_variations(&self, resp: &serde_json::Value) -> Result<Vec<AnticipatoryMidiResult>> {
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

        let variations = resp["variations"].as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing variations"))?;

        let mut results = Vec::new();
        for v in variations {
            let midi_b64 = v["midi_base64"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing midi_base64"))?;
            let midi_bytes = BASE64.decode(midi_b64)?;
            let hash = self.cas.write(&midi_bytes, "audio/midi")?;

            results.push(AnticipatoryMidiResult {
                hash,
                num_events: v["num_events"].as_u64().unwrap_or(0) as usize,
                duration_seconds: v["duration_seconds"].as_f64().unwrap_or(0.0) as f32,
            });
        }

        Ok(results)
    }
}

#[derive(Debug, Clone)]
pub struct AnticipatoryMidiResult {
    pub hash: String,
    pub num_events: usize,
    pub duration_seconds: f32,
}

#[derive(Debug, Clone)]
pub struct AnticipatoryEmbedResult {
    pub embedding: Vec<f32>,
    pub embedding_dim: usize,
    pub layer: i32,
    pub num_tokens: usize,
    pub truncated: bool,
}
```

### 3. Create Tool Implementations (`api/tools/anticipatory.rs`)

Three tools: `anticipatory_generate`, `anticipatory_continue`, `anticipatory_embed`

```rust
impl EventDualityServer {
    #[tracing::instrument(name = "mcp.tool.anticipatory_generate", skip(self, request))]
    pub async fn anticipatory_generate(
        &self,
        request: AnticipatoryGenerateRequest,
    ) -> Result<CallToolResult, McpError> {
        // Validate, create job, spawn background task...
        // Similar pattern to orpheus_generate
    }

    #[tracing::instrument(name = "mcp.tool.anticipatory_continue", skip(self, request))]
    pub async fn anticipatory_continue(
        &self,
        request: AnticipatoryContinueRequest,
    ) -> Result<CallToolResult, McpError> {
        // Validate, create job, spawn background task...
    }

    // Embed is synchronous (fast)
    #[tracing::instrument(name = "mcp.tool.anticipatory_embed", skip(self, request))]
    pub async fn anticipatory_embed(
        &self,
        request: AnticipatoryEmbedRequest,
    ) -> Result<CallToolResult, McpError> {
        match self.anticipatory_client.embed(
            request.midi_hash.clone(),
            request.embed_layer.unwrap_or(-3),
            request.model_size.unwrap_or_else(|| "small".to_string()),
        ).await {
            Ok(result) => {
                let response = serde_json::json!({
                    "embedding": result.embedding,
                    "embedding_dim": result.embedding_dim,
                    "layer": result.layer,
                    "num_tokens": result.num_tokens,
                    "truncated": result.truncated,
                    "input_hash": request.midi_hash,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default()
                )]))
            }
            Err(e) => Ok(CallToolResult::error(format!("Embedding failed: {}", e)))
        }
    }
}
```

### 4. Register Tools (`handler.rs`)

```rust
Tool::new("anticipatory_generate", "Generate polyphonic MIDI with Stanford's Anticipatory Music Transformer")
    .with_input_schema(schema_for::<AnticipatoryGenerateRequest>()),
Tool::new("anticipatory_continue", "Continue existing MIDI with Anticipatory Music Transformer")
    .with_input_schema(schema_for::<AnticipatoryContinueRequest>()),
Tool::new("anticipatory_embed", "Extract hidden state embeddings from MIDI")
    .with_input_schema(schema_for::<AnticipatoryEmbedRequest>())
    .read_only(),
```

## Testing

```bash
# Generate
anticipatory_generate({length_seconds: 15.0, top_p: 0.95})

# Continue
anticipatory_continue({
  midi_hash: "<hash>",
  prime_seconds: 5.0,
  length_seconds: 20.0
})

# Embed (for similarity/clustering)
anticipatory_embed({midi_hash: "<hash>"})
```

## Use Cases

1. **Polyphonic generation**: Better at multi-voice textures than Orpheus
2. **Continuation**: Extend existing pieces naturally
3. **Embeddings**: Semantic similarity, clustering, retrieval
4. **Research**: Access to intermediate hidden states

## Notes

- Different from Orpheus - uses `anticipation` library format
- Three model sizes available (small/medium/large)
- Embed tool is synchronous (fast inference)
- Max input: 1024 tokens (longer is truncated)
- Recommended `top_p`: 0.95-0.98
