# Task 02: orpheus_classify - Human vs AI Music Detection

## Service Details

| Property | Value |
|----------|-------|
| Service Name | orpheus-classifier |
| Port | 2001 |
| Task | classify |
| Model | classifier (23k steps, 398MB) |
| Output | Classification result (JSON) |

## API Contract

### Request (POST /predict)

```json
{
  "midi_input": "base64...",   // required: MIDI to classify
  "client_job_id": "string"    // optional: for tracing
}
```

### Response

```json
{
  "task": "classify",
  "classification": {
    "is_human": true,
    "confidence": 0.87,
    "probabilities": {
      "human": 0.87,
      "ai": 0.13
    }
  },
  "metadata": {
    "client_job_id": "...",
    "trace_id": "...",
    "span_id": "..."
  }
}
```

## Python Service Implementation (Reference)

From `~/src/halfremembered-music-models/services/orpheus-classifier/api.py`:

```python
def decode_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
    if "midi_input" not in request:
        raise ValueError("classify requires midi_input")

    midi_b64 = request["midi_input"].strip()
    midi_input = base64.b64decode(midi_b64)

    return {
        "midi_input": midi_input,
        "client_job_id": self.extract_client_job_id(request),
    }

def _do_predict(self, x: Dict[str, Any]) -> Dict[str, Any]:
    tokens = self.tokenizer.encode_midi(x["midi_input"])

    # Truncate to classifier's max length
    max_len = 1024
    if len(tokens) > max_len:
        tokens = tokens[:max_len]

    input_tokens = torch.LongTensor([tokens]).to(self.device)

    self.model.eval()
    with torch.no_grad():
        logits = self.model(input_tokens)
        prob = torch.sigmoid(logits).item()

    is_human = prob > 0.5
    confidence = prob if is_human else 1 - prob

    return {
        "task": "classify",
        "classification": {
            "is_human": is_human,
            "confidence": confidence,
            "probabilities": {
                "human": prob,
                "ai": 1 - prob
            }
        },
        "client_job_id": x.get("client_job_id"),
    }
```

## Implementation Steps

### 1. Add Request Schema (`schema.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusClassifyRequest {
    #[schemars(description = "CAS hash of MIDI to classify (required)")]
    pub midi_hash: String,
}
```

Note: This is a **synchronous** tool (fast inference, ~100ms). No job system needed.

### 2. Add HTTP Client Method (`local_models.rs`)

Add new method:

```rust
pub async fn run_orpheus_classify(
    &self,
    midi_hash: String,
) -> Result<OrpheusClassifyResult> {
    let midi_bytes = self.resolve_cas(&midi_hash)?;

    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    let b64_midi = BASE64.encode(&midi_bytes);

    let mut request_body = serde_json::Map::new();
    request_body.insert("midi_input".to_string(), serde_json::json!(b64_midi));

    let builder = self.client.post("http://127.0.0.1:2001/predict")
        .json(&request_body);
    let builder = self.inject_trace_context(builder);

    let resp = builder.send().await
        .context("Failed to call classifier API")?;

    // Handle 429, errors, parse response...
    let resp_json: serde_json::Value = resp.json().await?;

    let classification = resp_json.get("classification")
        .ok_or_else(|| anyhow::anyhow!("Missing classification in response"))?;

    Ok(OrpheusClassifyResult {
        is_human: classification["is_human"].as_bool().unwrap_or(false),
        confidence: classification["confidence"].as_f64().unwrap_or(0.0) as f32,
        human_probability: classification["probabilities"]["human"].as_f64().unwrap_or(0.0) as f32,
        ai_probability: classification["probabilities"]["ai"].as_f64().unwrap_or(0.0) as f32,
    })
}
```

Add result type:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrpheusClassifyResult {
    pub is_human: bool,
    pub confidence: f32,
    pub human_probability: f32,
    pub ai_probability: f32,
}
```

### 3. Create Tool Implementation (`api/tools/classify.rs`)

This tool is **synchronous** - no job system needed since classification is fast:

```rust
use crate::api::schema::OrpheusClassifyRequest;
use crate::api::service::EventDualityServer;
use baton::{ErrorData as McpError, CallToolResult, Content};

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.orpheus_classify",
        skip(self, request),
        fields(midi.hash = %request.midi_hash)
    )]
    pub async fn orpheus_classify(
        &self,
        request: OrpheusClassifyRequest,
    ) -> Result<CallToolResult, McpError> {
        match self.local_models.run_orpheus_classify(request.midi_hash.clone()).await {
            Ok(result) => {
                let response = serde_json::json!({
                    "is_human": result.is_human,
                    "confidence": result.confidence,
                    "verdict": if result.is_human { "human" } else { "AI" },
                    "probabilities": {
                        "human": result.human_probability,
                        "ai": result.ai_probability,
                    },
                    "input_hash": request.midi_hash,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default()
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(format!("Classification failed: {}", e)))
            }
        }
    }
}
```

### 4. Register Tool (`handler.rs`)

Add to `tools()`:
```rust
Tool::new("orpheus_classify", "Classify MIDI as human-composed or AI-generated")
    .with_input_schema(schema_for::<OrpheusClassifyRequest>())
    .read_only(),  // This is a read-only analysis tool
```

Add to `call_tool()`:
```rust
"orpheus_classify" => {
    let request: OrpheusClassifyRequest = serde_json::from_value(args)
        .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
    self.server.orpheus_classify(request).await
}
```

## Testing

```bash
# Test via MCP
orpheus_classify({midi_hash: "<cas_hash>"})

# Expected output:
{
  "is_human": false,
  "confidence": 0.92,
  "verdict": "AI",
  "probabilities": {
    "human": 0.08,
    "ai": 0.92
  }
}
```

## Use Cases

1. **Quality assessment**: Check if generated MIDI "sounds human"
2. **Iteration guidance**: Low human probability might suggest regenerating
3. **Experimentation**: Test different temperature/top_p settings
4. **Dataset validation**: Verify training data quality

## Notes

- Fast inference (~100ms), no need for async job pattern
- Max input length: 1024 tokens (longer MIDI is truncated)
- Confidence interpretation:
  - >0.9: High confidence
  - 0.7-0.9: Moderate confidence
  - <0.7: Low confidence, ambiguous
