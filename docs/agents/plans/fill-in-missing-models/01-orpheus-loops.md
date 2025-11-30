# Task 01: orpheus_loops - Drum/Percussion Loop Generation

## Service Details

| Property | Value |
|----------|-------|
| Service Name | orpheus-loops |
| Port | 2003 |
| Task | loops |
| Model | loops (3.4k steps, 1.8GB) |
| EOS Token | 18818 (different from base!) |
| Output | MIDI (base64 encoded) |

## API Contract

### Request (POST /predict)

```json
{
  "temperature": 1.0,        // 0.0-2.0, sampling randomness
  "top_p": 0.95,             // 0.0-1.0, nucleus sampling
  "max_tokens": 1024,        // max tokens to generate
  "num_variations": 1,       // number of variations
  "seed_midi": "base64...",  // optional: seed MIDI for variation
  "client_job_id": "string"  // optional: for tracing
}
```

### Response

```json
{
  "task": "loops",
  "variations": [
    {
      "midi_base64": "base64...",
      "num_tokens": 512
    }
  ],
  "metadata": {
    "client_job_id": "...",
    "trace_id": "...",
    "span_id": "..."
  }
}
```

## Python Service Implementation (Reference)

From `~/src/halfremembered-music-models/services/orpheus-loops/api.py`:

```python
def decode_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
    # Decode MIDI seed if present
    seed_midi = None
    if "seed_midi" in request:
        seed_b64 = request["seed_midi"].strip()
        seed_midi = base64.b64decode(seed_b64)

    return {
        "temperature": request.get("temperature", 1.0),
        "top_p": request.get("top_p", 0.95),
        "max_tokens": request.get("max_tokens", 1024),
        "num_variations": request.get("num_variations", 1),
        "seed_midi": seed_midi,
        "client_job_id": self.extract_client_job_id(request),
    }

def _do_predict(self, x: Dict[str, Any]) -> Dict[str, Any]:
    seed_tokens = []
    if x["seed_midi"]:
        seed_tokens = self.tokenizer.encode_midi(x["seed_midi"])

    num_variations = x["num_variations"]
    results = []

    for i in range(num_variations):
        tokens = self._generate_tokens(
            seed_tokens=seed_tokens,
            max_tokens=x["max_tokens"],
            temperature=x["temperature"],
            top_p=x["top_p"],
        )

        midi_bytes = self.tokenizer.decode_tokens(tokens)
        results.append({
            "midi_base64": base64.b64encode(midi_bytes).decode(),
            "num_tokens": len(tokens),
        })

    return {
        "task": "loops",
        "variations": results,
        "client_job_id": x.get("client_job_id"),
    }
```

## Implementation Steps

### 1. Add Request Schema (`schema.rs`)

Add to `crates/hootenanny/src/api/schema.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusLoopsRequest {
    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024)")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    #[schemars(description = "CAS hash of seed MIDI for seeded generation (optional)")]
    pub seed_hash: Option<String>,

    // Standard artifact fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}
```

### 2. Add HTTP Client Method (`local_models.rs`)

This tool requires a **new HTTP client** since it calls a different port (2003) than the base orpheus (2000).

Option A: Add a new field `orpheus_loops_url` to `LocalModels`:
```rust
pub struct LocalModels {
    cas: Cas,
    orpheus_url: String,
    orpheus_loops_url: String,  // NEW
    client: reqwest::Client,
}
```

Option B: Create a generic `run_orpheus_task` method that takes port as param.

**Recommended**: Option A is cleaner for now. Add `--orpheus-loops-port` CLI arg to main.rs.

### 3. Create Tool Implementation (`api/tools/loops.rs`)

Create new file `crates/hootenanny/src/api/tools/loops.rs`:

```rust
use crate::api::schema::OrpheusLoopsRequest;
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash, VariationSetId};
use baton::{ErrorData as McpError, CallToolResult, Content};
use std::sync::Arc;

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.orpheus_loops",
        skip(self, request),
        fields(
            model.temperature = request.temperature,
            model.num_variations = request.num_variations,
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn orpheus_loops(
        &self,
        request: OrpheusLoopsRequest,
    ) -> Result<CallToolResult, McpError> {
        // Validate parameters
        Self::validate_sampling_params(request.temperature, request.top_p)?;

        // Create job
        let job_id = self.job_store.create_job("orpheus_loops".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        // Clone for background task
        let local_models = Arc::clone(&self.local_models);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            // Call the loops service at port 2003
            match local_models.run_orpheus_loops(
                request.seed_hash,
                request.temperature,
                request.top_p,
                request.max_tokens,
                request.num_variations,
                Some(job_id_clone.as_str().to_string()),
            ).await {
                Ok(result) => {
                    // Create artifacts... (follow orpheus_generate pattern)
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
            "message": "Loop generation started. Use job_poll() to retrieve results."
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }
}
```

### 4. Register Tool (`handler.rs`)

Add to `tools()`:
```rust
Tool::new("orpheus_loops", "Generate drum/percussion loops with Orpheus")
    .with_input_schema(schema_for::<OrpheusLoopsRequest>()),
```

Add to `call_tool()`:
```rust
"orpheus_loops" => {
    let request: OrpheusLoopsRequest = serde_json::from_value(args)
        .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
    self.server.orpheus_loops(request).await
}
```

### 5. Update Module Exports

Add to `api/tools/mod.rs`:
```rust
mod loops;
```

## Testing

```bash
# Test via MCP
# 1. Generate fresh loops
orpheus_loops({})

# 2. Generate with seed
orpheus_loops({seed_hash: "<cas_hash>", num_variations: 3})

# 3. Check HTTP directly
curl -X POST http://127.0.0.1:2003/predict \
  -H "Content-Type: application/json" \
  -d '{"temperature": 1.0, "max_tokens": 512}'
```

## Artifact Tags

Generated artifacts should have:
- `type:midi`
- `phase:generation`
- `tool:orpheus_loops`
- `style:drums` or `style:percussion`

## Notes

- Uses different EOS token (18818) than base Orpheus (18817)
- Optimized for rhythmic/percussion patterns
- Can generate multi-instrumental drum patterns
