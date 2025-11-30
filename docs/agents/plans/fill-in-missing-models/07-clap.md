# Task 07: clap_analyze - Audio Analysis and Embeddings

## Service Details

| Property | Value |
|----------|-------|
| Service Name | clap |
| Port | 2007 |
| Model | laion/clap-htsat-unfused |
| VRAM Required | ~600MB |
| Sample Rate | 48kHz (resamples if needed) |
| Embedding Dim | 512 |

## Tasks

CLAP provides multiple analysis tasks in a single request:

1. **embeddings** - Extract audio embeddings (512-dim vector)
2. **zero_shot** - Compare audio to custom text labels
3. **similarity** - Compare two audio files
4. **genre** - Genre classification (preset labels)
5. **mood** - Mood detection (preset labels)

## API Contract

### Request (POST /predict)

```json
{
  "audio": "base64...",                    // required: primary audio (WAV)
  "tasks": ["embeddings", "genre", "mood"], // which analyses to run
  "audio_b": "base64...",                  // optional: second audio for similarity
  "text_candidates": ["rock", "jazz"],     // optional: labels for zero_shot
  "client_job_id": "string"
}
```

### Response

```json
{
  "tasks": ["embeddings", "genre", "mood"],
  "embeddings": [0.1, -0.2, ...],  // 512 floats
  "genre": {
    "top_prediction": {"label": "rock", "confidence": 0.85},
    "predictions": [
      {"label": "rock", "confidence": 0.85},
      {"label": "pop", "confidence": 0.10},
      ...
    ]
  },
  "mood": {
    "top_prediction": {"label": "energetic", "confidence": 0.72},
    "predictions": [...]
  },
  "similarity": {
    "score": 0.82,
    "distance": 0.18
  },
  "zero_shot": {
    "top_prediction": {...},
    "predictions": [...]
  },
  "metadata": {...}
}
```

## Python Service Implementation (Reference)

From `~/src/halfremembered-music-models/services/clap/api.py`:

```python
class CLAPAPI(ModelAPI, ls.LitAPI):
    def setup(self, device: str):
        super().setup(device)
        check_available_vram(1.0, device)

        self.audio_encoder = AudioEncoder()

        from transformers import ClapModel, ClapProcessor

        model_name = "laion/clap-htsat-unfused"
        self.processor = ClapProcessor.from_pretrained(model_name)
        self.model = ClapModel.from_pretrained(model_name).to(device)
        self.model.eval()

    def decode_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
        return {
            "audio": request.get("audio"),
            "tasks": request.get("tasks", ["embeddings"]),
            "audio_b": request.get("audio_b"),
            "text_candidates": request.get("text_candidates", []),
            "client_job_id": self.extract_client_job_id(request),
        }

    def _do_predict(self, x: Dict[str, Any]) -> Dict[str, Any]:
        tasks = x["tasks"]
        results = {"tasks": tasks}

        # Decode primary audio
        audio, sr = self.audio_encoder.decode_wav(x["audio"])
        audio_embeddings = self._get_embeddings(audio, sr)

        if "embeddings" in tasks:
            results["embeddings"] = audio_embeddings.tolist()

        if "zero_shot" in tasks and x["text_candidates"]:
            results["zero_shot"] = self._zero_shot_classification(
                audio_embeddings, x["text_candidates"]
            )

        if "similarity" in tasks and x.get("audio_b"):
            audio_b, sr_b = self.audio_encoder.decode_wav(x["audio_b"])
            embeddings_b = self._get_embeddings(audio_b, sr_b)
            similarity = torch.nn.functional.cosine_similarity(
                torch.tensor(audio_embeddings).unsqueeze(0),
                torch.tensor(embeddings_b).unsqueeze(0)
            ).item()
            results["similarity"] = {"score": similarity, "distance": 1.0 - similarity}

        if "genre" in tasks:
            genres = ["rock", "pop", "electronic", "jazz", "classical",
                      "hip hop", "country", "reggae", "metal", "folk", "blues"]
            results["genre"] = self._zero_shot_classification(audio_embeddings, genres)

        if "mood" in tasks:
            moods = ["happy", "sad", "angry", "peaceful", "exciting",
                     "scary", "tense", "melancholic", "upbeat", "calm"]
            results["mood"] = self._zero_shot_classification(audio_embeddings, moods)

        return results

    def _get_embeddings(self, audio: np.ndarray, sample_rate: int) -> np.ndarray:
        # CLAP expects 48kHz
        if sample_rate != 48000:
            audio = self.audio_encoder.resample(audio, sample_rate, 48000)
            sample_rate = 48000

        inputs = self.processor(audio=audio, sampling_rate=sample_rate, return_tensors="pt")
        inputs = {k: v.to(self.device) for k, v in inputs.items()}

        with torch.no_grad():
            embeddings = self.model.get_audio_features(**inputs)

        return embeddings[0].cpu().numpy()

    def _zero_shot_classification(self, audio_emb: np.ndarray, labels: List[str]) -> Dict:
        text_emb = self._get_text_embeddings(labels)

        audio_tensor = torch.from_numpy(audio_emb).unsqueeze(0).to(self.device)
        text_tensor = torch.from_numpy(text_emb).to(self.device)

        audio_tensor = torch.nn.functional.normalize(audio_tensor, dim=-1)
        text_tensor = torch.nn.functional.normalize(text_tensor, dim=-1)

        similarity = (audio_tensor @ text_tensor.T).squeeze(0)
        probs = torch.nn.functional.softmax(similarity * 100, dim=-1)
        scores = probs.cpu().numpy()

        results = [{"label": label, "confidence": float(score)}
                   for label, score in zip(labels, scores)]
        results.sort(key=lambda x: x["confidence"], reverse=True)

        return {"top_prediction": results[0], "predictions": results}
```

## Implementation Steps

### 1. Add Request Schema (`schema.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClapAnalyzeRequest {
    #[schemars(description = "CAS hash of audio file to analyze (required)")]
    pub audio_hash: String,

    #[schemars(description = "Tasks to run: 'embeddings', 'genre', 'mood', 'zero_shot', 'similarity'")]
    #[serde(default = "default_clap_tasks")]
    pub tasks: Vec<String>,

    #[schemars(description = "CAS hash of second audio for similarity comparison")]
    pub audio_b_hash: Option<String>,

    #[schemars(description = "Custom text labels for zero_shot classification")]
    #[serde(default)]
    pub text_candidates: Vec<String>,
}

fn default_clap_tasks() -> Vec<String> {
    vec!["embeddings".to_string()]
}
```

### 2. Add HTTP Client

```rust
pub struct ClapClient {
    cas: Cas,
    url: String,
    client: reqwest::Client,
}

impl ClapClient {
    pub fn new(cas: Cas, port: u16) -> Self {
        Self {
            cas,
            url: format!("http://127.0.0.1:{}", port),
            client: reqwest::Client::new(),
        }
    }

    pub async fn analyze(
        &self,
        audio_hash: String,
        tasks: Vec<String>,
        audio_b_hash: Option<String>,
        text_candidates: Vec<String>,
        client_job_id: Option<String>,
    ) -> Result<ClapResult> {
        // Read audio from CAS
        let audio_bytes = self.cas.read(&audio_hash)?
            .ok_or_else(|| anyhow::anyhow!("Audio not found in CAS"))?;

        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

        let mut request_body = serde_json::json!({
            "audio": BASE64.encode(&audio_bytes),
            "tasks": tasks,
            "text_candidates": text_candidates,
            "client_job_id": client_job_id,
        });

        // Add second audio if similarity requested
        if let Some(hash_b) = audio_b_hash {
            let audio_b_bytes = self.cas.read(&hash_b)?
                .ok_or_else(|| anyhow::anyhow!("Audio B not found in CAS"))?;
            request_body["audio_b"] = serde_json::json!(BASE64.encode(&audio_b_bytes));
        }

        let resp = self.client.post(format!("{}/predict", self.url))
            .json(&request_body)
            .send()
            .await?;

        let resp_json: serde_json::Value = resp.json().await?;

        Ok(ClapResult {
            embeddings: resp_json.get("embeddings")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect()),
            genre: resp_json.get("genre").cloned(),
            mood: resp_json.get("mood").cloned(),
            similarity: resp_json.get("similarity").cloned(),
            zero_shot: resp_json.get("zero_shot").cloned(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ClapResult {
    pub embeddings: Option<Vec<f32>>,
    pub genre: Option<serde_json::Value>,
    pub mood: Option<serde_json::Value>,
    pub similarity: Option<serde_json::Value>,
    pub zero_shot: Option<serde_json::Value>,
}
```

### 3. Create Tool Implementation (`api/tools/clap.rs`)

CLAP analysis is **synchronous** (fast inference):

```rust
use crate::api::schema::ClapAnalyzeRequest;
use crate::api::service::EventDualityServer;
use baton::{ErrorData as McpError, CallToolResult, Content};

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.clap_analyze",
        skip(self, request),
        fields(
            audio.hash = %request.audio_hash,
            tasks = ?request.tasks,
        )
    )]
    pub async fn clap_analyze(
        &self,
        request: ClapAnalyzeRequest,
    ) -> Result<CallToolResult, McpError> {
        // Validate tasks
        let valid_tasks = ["embeddings", "genre", "mood", "zero_shot", "similarity"];
        for task in &request.tasks {
            if !valid_tasks.contains(&task.as_str()) {
                return Err(McpError::invalid_params(
                    format!("Invalid task '{}'. Valid: {:?}", task, valid_tasks)
                ));
            }
        }

        // Check required params for specific tasks
        if request.tasks.contains(&"similarity".to_string()) && request.audio_b_hash.is_none() {
            return Err(McpError::invalid_params(
                "similarity task requires audio_b_hash"
            ));
        }
        if request.tasks.contains(&"zero_shot".to_string()) && request.text_candidates.is_empty() {
            return Err(McpError::invalid_params(
                "zero_shot task requires text_candidates"
            ));
        }

        match self.clap_client.analyze(
            request.audio_hash.clone(),
            request.tasks.clone(),
            request.audio_b_hash.clone(),
            request.text_candidates.clone(),
            None,
        ).await {
            Ok(result) => {
                let mut response = serde_json::json!({
                    "tasks": request.tasks,
                    "input_hash": request.audio_hash,
                });

                if let Some(embeddings) = result.embeddings {
                    response["embeddings"] = serde_json::json!(embeddings);
                    response["embedding_dim"] = serde_json::json!(embeddings.len());
                }
                if let Some(genre) = result.genre {
                    response["genre"] = genre;
                }
                if let Some(mood) = result.mood {
                    response["mood"] = mood;
                }
                if let Some(similarity) = result.similarity {
                    response["similarity"] = similarity;
                }
                if let Some(zero_shot) = result.zero_shot {
                    response["zero_shot"] = zero_shot;
                }

                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response).unwrap_or_default()
                )]))
            }
            Err(e) => {
                Ok(CallToolResult::error(format!("CLAP analysis failed: {}", e)))
            }
        }
    }
}
```

### 4. Register Tool (`handler.rs`)

```rust
Tool::new("clap_analyze", "Analyze audio: extract embeddings, classify genre/mood, compare similarity")
    .with_input_schema(schema_for::<ClapAnalyzeRequest>())
    .read_only(),
```

## Testing

```bash
# Basic embedding extraction
clap_analyze({audio_hash: "<hash>", tasks: ["embeddings"]})

# Genre and mood classification
clap_analyze({audio_hash: "<hash>", tasks: ["genre", "mood"]})

# Custom zero-shot classification
clap_analyze({
  audio_hash: "<hash>",
  tasks: ["zero_shot"],
  text_candidates: ["orchestral", "solo piano", "synthesizer", "acoustic guitar"]
})

# Similarity comparison
clap_analyze({
  audio_hash: "<hash_a>",
  audio_b_hash: "<hash_b>",
  tasks: ["similarity"]
})
```

## Use Cases

1. **Audio tagging**: Automatic genre/mood classification
2. **Similarity search**: Find similar audio in collection
3. **Quality assessment**: Compare generated audio to references
4. **Embedding retrieval**: Semantic audio search
5. **Custom classification**: Zero-shot with domain-specific labels

## Preset Labels

### Genre Labels
rock, pop, electronic, jazz, classical, hip hop, country, reggae, metal, folk, blues

### Mood Labels
happy, sad, angry, peaceful, exciting, scary, tense, melancholic, upbeat, calm

## Notes

- Fast inference (~100ms), synchronous tool
- Audio resampled to 48kHz internally
- Embedding dimension: 512 (fixed)
- Uses CLIP-like contrastive learning
- Zero-shot works with any English text labels
- Confidence scores are softmax-normalized (sum to 1.0)
