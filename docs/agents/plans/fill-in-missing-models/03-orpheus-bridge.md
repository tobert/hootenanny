# Task 03: orpheus_bridge - Fix Existing Stub

## Service Details

| Property | Value |
|----------|-------|
| Service Name | orpheus-bridge |
| Port | 2002 |
| Task | bridge |
| Model | bridge (43k steps, 1.8GB) |
| Output | MIDI (base64 encoded) |

## Current State

The `orpheus_bridge` tool exists but is a **stub** that immediately fails:

```rust
// From crates/hootenanny/src/api/tools/orpheus.rs:422-427
let handle = tokio::spawn(async move {
    let _ = job_store.mark_running(&job_id_clone);
    // Stub for bridge implementation
    let _ = job_store.mark_failed(&job_id_clone, "Bridge generation not implemented yet".to_string());
});
```

This task completes the implementation.

## API Contract

### Request (POST /predict)

```json
{
  "section_a": "base64...",    // required: first section MIDI
  "section_b": "base64...",    // optional: target section (for future use)
  "temperature": 1.0,
  "top_p": 0.95,
  "max_tokens": 1024,
  "client_job_id": "string"
}
```

### Response

```json
{
  "task": "bridge",
  "variations": [
    {
      "midi_base64": "base64...",
      "num_tokens": 256
    }
  ],
  "metadata": {...}
}
```

## Python Service Implementation (Reference)

From `~/src/halfremembered-music-models/services/orpheus-bridge/api.py`:

```python
def decode_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
    if "section_a" not in request:
        raise ValueError("bridge requires section_a")

    section_a_b64 = request["section_a"].strip()
    section_a = base64.b64decode(section_a_b64)

    section_b = None
    if "section_b" in request:
        section_b_b64 = request["section_b"].strip()
        section_b = base64.b64decode(section_b_b64)

    return {
        "temperature": request.get("temperature", 1.0),
        "top_p": request.get("top_p", 0.95),
        "max_tokens": request.get("max_tokens", 1024),
        "section_a": section_a,
        "section_b": section_b,
        "client_job_id": self.extract_client_job_id(request),
    }

def _do_predict(self, x: Dict[str, Any]) -> Dict[str, Any]:
    section_a_tokens = self.tokenizer.encode_midi(x["section_a"])

    # TODO: When API supports true bridging with section_b, update logic

    tokens = self._generate_tokens(
        seed_tokens=section_a_tokens,
        max_tokens=x["max_tokens"],
        temperature=x["temperature"],
        top_p=x["top_p"],
    )

    midi_bytes = self.tokenizer.decode_tokens(tokens)

    return {
        "task": "bridge",
        "variations": [{
            "midi_base64": base64.b64encode(midi_bytes).decode(),
            "num_tokens": len(tokens),
        }],
        "client_job_id": x.get("client_job_id"),
    }
```

## Implementation Steps

### 1. Schema Already Exists

The `OrpheusBridgeRequest` schema is already defined in `schema.rs`:

```rust
pub struct OrpheusBridgeRequest {
    pub section_a_hash: String,
    pub section_b_hash: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub variation_set_id: Option<String>,
    pub parent_id: Option<String>,
    pub tags: Vec<String>,
    pub creator: Option<String>,
}
```

### 2. Add HTTP Client Method (`local_models.rs`)

Add new method to call the bridge service at port 2002:

```rust
pub async fn run_orpheus_bridge(
    &self,
    section_a_hash: String,
    section_b_hash: Option<String>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    client_job_id: Option<String>,
) -> Result<OrpheusGenerateResult> {
    let section_a_bytes = self.resolve_cas(&section_a_hash)?;

    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    let mut request_body = serde_json::Map::new();
    request_body.insert("section_a".to_string(),
        serde_json::json!(BASE64.encode(&section_a_bytes)));

    if let Some(hash) = section_b_hash {
        let section_b_bytes = self.resolve_cas(&hash)?;
        request_body.insert("section_b".to_string(),
            serde_json::json!(BASE64.encode(&section_b_bytes)));
    }

    request_body.insert("temperature".to_string(),
        serde_json::json!(temperature.unwrap_or(1.0)));
    request_body.insert("top_p".to_string(),
        serde_json::json!(top_p.unwrap_or(0.95)));
    request_body.insert("max_tokens".to_string(),
        serde_json::json!(max_tokens.unwrap_or(1024)));

    if let Some(job_id) = client_job_id {
        request_body.insert("client_job_id".to_string(), serde_json::json!(job_id));
    }

    let builder = self.client.post("http://127.0.0.1:2002/predict")
        .json(&request_body);
    let builder = self.inject_trace_context(builder);

    let resp = builder.send().await
        .context("Failed to call bridge API")?;

    // Handle 429, parse response... (same pattern as run_orpheus_generate)
    // Extract variations array and store in CAS
    // Return OrpheusGenerateResult
}
```

### 3. Fix the Stub (`api/tools/orpheus.rs`)

Replace the stub implementation at line 408-438 with the actual implementation:

```rust
#[tracing::instrument(
    name = "mcp.tool.orpheus_bridge",
    skip(self, request),
    fields(
        model.name = ?request.model,
        model.section_a_hash = %request.section_a_hash,
        job.id = tracing::field::Empty,
    )
)]
pub async fn orpheus_bridge(
    &self,
    request: OrpheusBridgeRequest,
) -> Result<CallToolResult, McpError> {
    Self::validate_sampling_params(request.temperature, request.top_p)?;

    let job_id = self.job_store.create_job("orpheus_bridge".to_string());
    tracing::Span::current().record("job.id", job_id.as_str());

    let local_models = Arc::clone(&self.local_models);
    let artifact_store = Arc::clone(&self.artifact_store);
    let job_store = self.job_store.clone();
    let job_id_clone = job_id.clone();

    let handle = tokio::spawn(async move {
        let _ = job_store.mark_running(&job_id_clone);

        match local_models.run_orpheus_bridge(
            request.section_a_hash.clone(),
            request.section_b_hash.clone(),
            request.temperature,
            request.top_p,
            request.max_tokens,
            Some(job_id_clone.as_str().to_string()),
        ).await {
            Ok(result) => {
                // Create artifacts (follow same pattern as orpheus_generate)
                let artifacts_result = (|| -> anyhow::Result<Vec<Artifact>> {
                    let mut artifacts = Vec::new();
                    let store = artifact_store.write()
                        .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                    for (i, hash) in result.output_hashes.iter().enumerate() {
                        let tokens = result.num_tokens.get(i).copied().map(|t| t as u32);
                        let content_hash = ContentHash::new(hash);
                        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                        let creator = request.creator.clone()
                            .unwrap_or_else(|| "agent_orpheus".to_string());

                        let mut artifact = Artifact::new(
                            artifact_id,
                            content_hash,
                            &creator,
                            serde_json::json!({
                                "tokens": tokens,
                                "task": "bridge",
                                "section_a": request.section_a_hash,
                                "section_b": request.section_b_hash,
                            })
                        )
                        .with_tags(vec![
                            "type:midi",
                            "phase:generation",
                            "tool:orpheus_bridge"
                        ]);

                        // Link to parent (section_a)
                        artifact = artifact.with_parent(
                            ArtifactId::from_hash_prefix(&ContentHash::new(&request.section_a_hash))
                        );

                        artifact = artifact.with_tags(request.tags.clone());
                        store.put(artifact.clone())?;
                        artifacts.push(artifact);
                    }

                    store.flush()?;
                    Ok(artifacts)
                })();

                match artifacts_result {
                    Ok(artifacts) => {
                        let response = serde_json::json!({
                            "status": result.status,
                            "output_hashes": result.output_hashes,
                            "artifact_ids": artifacts.iter()
                                .map(|a| a.id.as_str()).collect::<Vec<_>>(),
                            "summary": result.summary,
                        });
                        let _ = job_store.mark_complete(&job_id_clone, response);
                    }
                    Err(e) => {
                        let _ = job_store.mark_failed(&job_id_clone,
                            format!("Failed to create artifacts: {}", e));
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
        "message": "Bridge generation started. Use job_poll() to retrieve results."
    });

    Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
}
```

## Testing

```bash
# Create two MIDI sections
result_a = orpheus_generate({max_tokens: 256})
# ... wait for completion, get hash_a

result_b = orpheus_generate({max_tokens: 256})
# ... wait for completion, get hash_b

# Generate bridge
orpheus_bridge({
  section_a_hash: hash_a,
  section_b_hash: hash_b,  // optional
  max_tokens: 512
})
```

## Use Cases

1. **Transition generation**: Create smooth transitions between song sections
2. **Arrangement**: Bridge verse to chorus, chorus to bridge, etc.
3. **Medley creation**: Connect different pieces together

## Notes

- The bridge model (43k steps) is trained specifically for transitions
- `section_b` is optional but reserved for future bidirectional bridging
- Links output to `section_a` as parent in artifact lineage
