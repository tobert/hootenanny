# Task 05: yue_generate - Text-to-Song with Lyrics

## Service Details

| Property | Value |
|----------|-------|
| Service Name | yue |
| Port | 2008 |
| Models | YuE-s1-7B + YuE-s2-1B |
| VRAM Required | ~15-24GB |
| Output | MP3/WAV audio |
| Generation Time | **Minutes** (long-running) |

## API Contract

### Request (POST /predict)

```json
{
  "lyrics": "[verse]\nHello world...\n[chorus]\nLa la la...",
  "genre": "Pop",
  "max_new_tokens": 3000,
  "run_n_segments": 2,
  "seed": 42,
  "client_job_id": "string"
}
```

### Response

```json
{
  "status": "success",
  "audio_base64": "base64...",  // MP3 or WAV
  "format": "mp3",
  "lyrics": "...",
  "genre": "Pop",
  "metadata": {...}
}
```

## Python Service Implementation (Reference)

From `~/src/halfremembered-music-models/services/yue/api.py`:

```python
class YuEAPI(ModelAPI, ls.LitAPI):
    def setup(self, device: str):
        super().setup(device)
        # YuE needs ~15-24GB depending on context length
        check_available_vram(16.0, device)

        # Verify repo exists
        if not os.path.exists(INFERENCE_DIR):
            raise RuntimeError(f"YuE repo not found at {INFERENCE_DIR}")

    def decode_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
        return {
            "lyrics": request.get("lyrics", ""),
            "genre": request.get("genre", "Pop"),
            "max_new_tokens": int(request.get("max_new_tokens", 3000)),
            "run_n_segments": int(request.get("run_n_segments", 2)),
            "seed": int(request.get("seed", 42)),
            "client_job_id": self.extract_client_job_id(request),
        }

    def _do_predict(self, x: Dict[str, Any]) -> Dict[str, Any]:
        lyrics = x["lyrics"]
        genre = x["genre"]

        if not lyrics:
            return {"error": "lyrics are required", ...}

        with tempfile.TemporaryDirectory() as temp_dir:
            lyrics_path = os.path.join(temp_dir, "lyrics.txt")
            genre_path = os.path.join(temp_dir, "genre.txt")
            output_dir = os.path.join(temp_dir, "output")

            os.makedirs(output_dir, exist_ok=True)

            with open(lyrics_path, "w") as f:
                f.write(lyrics)
            with open(genre_path, "w") as f:
                f.write(genre)

            venv_python = os.path.join(REPO_DIR, ".venv", "bin", "python")

            cmd = [
                venv_python, "infer.py",
                "--stage1_model", "m-a-p/YuE-s1-7B-anneal-en-cot",
                "--stage2_model", "m-a-p/YuE-s2-1B-general",
                "--genre_txt", genre_path,
                "--lyrics_txt", lyrics_path,
                "--run_n_segments", str(x["run_n_segments"]),
                "--stage2_batch_size", "4",
                "--output_dir", output_dir,
                "--max_new_tokens", str(x["max_new_tokens"]),
                "--cuda_idx", "0",
                "--seed", str(x["seed"])
            ]

            process = subprocess.run(cmd, cwd=INFERENCE_DIR, ...)

            # Find output file in output_dir/vocoder/mix/
            # Return base64 encoded audio
```

## Implementation Steps

### 1. Add Request Schema (`schema.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct YueGenerateRequest {
    #[schemars(description = "Lyrics with structure markers like [verse], [chorus], [bridge]")]
    pub lyrics: String,

    #[schemars(description = "Genre (e.g., 'Pop', 'Rock', 'Jazz', 'Electronic'). Default: 'Pop'")]
    pub genre: Option<String>,

    #[schemars(description = "Max tokens for stage 1 generation (default: 3000)")]
    pub max_new_tokens: Option<u32>,

    #[schemars(description = "Number of song segments to generate (default: 2)")]
    pub run_n_segments: Option<u32>,

    #[schemars(description = "Random seed for reproducibility (default: 42)")]
    pub seed: Option<u64>,

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

### 2. Add HTTP Client

Create `YueClient`:

```rust
pub struct YueClient {
    cas: Cas,
    url: String,
    client: reqwest::Client,
}

impl YueClient {
    pub fn new(cas: Cas, port: u16) -> Self {
        Self {
            cas,
            url: format!("http://127.0.0.1:{}", port),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(600))  // 10 min timeout!
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    pub async fn generate(
        &self,
        lyrics: String,
        genre: Option<String>,
        max_new_tokens: Option<u32>,
        run_n_segments: Option<u32>,
        seed: Option<u64>,
        client_job_id: Option<String>,
    ) -> Result<YueResult> {
        let request_body = serde_json::json!({
            "lyrics": lyrics,
            "genre": genre.unwrap_or_else(|| "Pop".to_string()),
            "max_new_tokens": max_new_tokens.unwrap_or(3000),
            "run_n_segments": run_n_segments.unwrap_or(2),
            "seed": seed.unwrap_or(42),
            "client_job_id": client_job_id,
        });

        let resp = self.client.post(format!("{}/predict", self.url))
            .json(&request_body)
            .send()
            .await?;

        // This can take MINUTES - handle timeout gracefully
        let resp_json: serde_json::Value = resp.json().await?;

        if let Some(error) = resp_json.get("error") {
            anyhow::bail!("YuE error: {}", error);
        }

        let audio_b64 = resp_json["audio_base64"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing audio_base64"))?;
        let format = resp_json["format"].as_str().unwrap_or("mp3");

        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        let audio_bytes = BASE64.decode(audio_b64)?;

        let mime_type = if format == "mp3" { "audio/mpeg" } else { "audio/wav" };
        let hash = self.cas.write(&audio_bytes, mime_type)?;

        Ok(YueResult {
            hash,
            format: format.to_string(),
            lyrics,
            genre: genre.unwrap_or_else(|| "Pop".to_string()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct YueResult {
    pub hash: String,
    pub format: String,
    pub lyrics: String,
    pub genre: String,
}
```

### 3. Create Tool Implementation (`api/tools/yue.rs`)

```rust
use crate::api::schema::YueGenerateRequest;
use crate::api::service::EventDualityServer;
use crate::artifact_store::{Artifact, ArtifactStore};
use crate::types::{ArtifactId, ContentHash};
use baton::{ErrorData as McpError, CallToolResult, Content};
use std::sync::Arc;

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.yue_generate",
        skip(self, request),
        fields(
            genre = ?request.genre,
            lyrics_len = request.lyrics.len(),
            job.id = tracing::field::Empty,
        )
    )]
    pub async fn yue_generate(
        &self,
        request: YueGenerateRequest,
    ) -> Result<CallToolResult, McpError> {
        if request.lyrics.trim().is_empty() {
            return Err(McpError::invalid_params("lyrics are required"));
        }

        let job_id = self.job_store.create_job("yue_generate".to_string());
        tracing::Span::current().record("job.id", job_id.as_str());

        let yue_client = Arc::clone(&self.yue_client);
        let artifact_store = Arc::clone(&self.artifact_store);
        let job_store = self.job_store.clone();
        let job_id_clone = job_id.clone();

        let handle = tokio::spawn(async move {
            let _ = job_store.mark_running(&job_id_clone);

            // This is a LONG operation - can take several minutes
            match yue_client.generate(
                request.lyrics.clone(),
                request.genre.clone(),
                request.max_new_tokens,
                request.run_n_segments,
                request.seed,
                Some(job_id_clone.as_str().to_string()),
            ).await {
                Ok(result) => {
                    let artifacts_result = (|| -> anyhow::Result<Artifact> {
                        let store = artifact_store.write()
                            .map_err(|_| anyhow::anyhow!("Lock poisoned"))?;

                        let content_hash = ContentHash::new(&result.hash);
                        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);
                        let creator = request.creator.clone()
                            .unwrap_or_else(|| "agent_yue".to_string());

                        let artifact = Artifact::new(
                            artifact_id,
                            content_hash,
                            &creator,
                            serde_json::json!({
                                "lyrics": result.lyrics,
                                "genre": result.genre,
                                "format": result.format,
                            })
                        )
                        .with_tags(vec![
                            "type:audio",
                            &format!("format:{}", result.format),
                            "phase:generation",
                            "tool:yue_generate",
                            "has:vocals"
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
                                "format": result.format,
                                "genre": result.genre,
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
            "message": "YuE song generation started. This may take several minutes. Use job_poll() to check progress."
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }
}
```

### 4. Register Tool (`handler.rs`)

```rust
Tool::new("yue_generate", "Generate a complete song with vocals from lyrics using YuE")
    .with_input_schema(schema_for::<YueGenerateRequest>()),
```

## Testing

```bash
yue_generate({
  lyrics: "[verse]\nHello world, I'm singing to you\nThis is a test of YuE\n\n[chorus]\nLa la la, generated song\nLa la la, won't take long",
  genre: "Pop"
})

# Then poll - this takes MINUTES
job_poll({job_ids: ["job_xxx"], timeout_ms: 60000})
```

## Lyrics Format

YuE expects structured lyrics with markers:

```
[verse]
First verse lyrics here
More lines of the verse

[chorus]
Catchy chorus lyrics
Repeat this part

[bridge]
Optional bridge section

[outro]
Ending lyrics
```

## Artifact Tags

- `type:audio`
- `format:mp3` or `format:wav`
- `phase:generation`
- `tool:yue_generate`
- `has:vocals`

## Notes

- **Very long generation time** - minutes, not seconds
- Requires ~16GB+ VRAM
- Uses subprocess to call YuE inference script
- Output includes synthesized vocals
- Two-stage model: 7B for structure, 1B for audio
- Set client timeout to 10+ minutes
