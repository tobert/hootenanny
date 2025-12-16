# 02: Content Resolver

**File:** `crates/chaosgarden/src/nodes.rs`, new `crates/chaosgarden/src/content.rs`
**Focus:** Load WAV audio from artifact_id via CAS
**Dependencies:** None
**Unblocks:** 03-playback-mixing

---

## Task

Implement content resolution: given an `artifact_id`, fetch the WAV file from CAS and decode it to samples that `PlaybackEngine` can use.

**Why this first?** Regions reference content by artifact_id. Without resolution, regions have no audio data.

**Deliverables:**
1. `ContentResolver` trait for fetching audio by artifact_id
2. `CasContentResolver` implementation using CAS HTTP API
3. Decoded audio cached in memory for playback

**Definition of Done:**
```bash
cargo check -p chaosgarden
cargo test -p chaosgarden
```

## Out of Scope

- PipeWire output wiring — that's task 01
- Mixing multiple regions — that's task 03
- Streaming decode (load full file for v1)

---

## Existing Code

### DecodedAudio (already exists)

```rust
// crates/chaosgarden/src/nodes/audio_file.rs
pub struct DecodedAudio {
    pub samples: Vec<f32>,  // Interleaved (L, R, L, R, ...)
    pub sample_rate: u32,
    pub channels: u8,       // 1 = mono, 2 = stereo
}

impl DecodedAudio {
    pub fn frames(&self) -> usize;        // samples per channel
    pub fn duration_seconds(&self) -> f64;
}

pub fn decode_wav(data: &[u8]) -> Result<DecodedAudio, DecodeError>;
```

### ContentResolver trait (create new)

```rust
// crates/chaosgarden/src/content.rs (new file)
pub trait ContentResolver: Send + Sync {
    fn resolve(&self, artifact_id: &str) -> Result<Arc<DecodedAudio>, ContentError>;
}
```

Note: Return `Arc<DecodedAudio>` for sharing between regions.

---

## CAS Integration

Artifacts are stored in CAS. To fetch:

```rust
// HTTP endpoint (hootenanny serves this)
// GET /artifact/{artifact_id} → raw bytes with Content-Type header
// GET /cas/{hash} → raw bytes

pub struct CasContentResolver {
    base_url: String,  // e.g., "http://localhost:8080"
    client: reqwest::blocking::Client,
    cache: HashMap<String, Arc<DecodedAudio>>,
}

impl ContentResolver for CasContentResolver {
    fn resolve(&self, artifact_id: &str) -> Result<DecodedAudio, ContentError> {
        // 1. Check cache
        // 2. Fetch from /artifact/{artifact_id}
        // 3. Decode WAV
        // 4. Cache and return
    }
}
```

---

## Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("Artifact not found: {0}")]
    NotFound(String),

    #[error("Failed to fetch: {0}")]
    FetchError(String),

    #[error("Failed to decode: {0}")]
    DecodeError(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}
```

Use existing `DecodedAudio` from `nodes/audio_file.rs`. Add helper method if needed:

```rust
impl DecodedAudio {
    /// Get sample at frame index, channel (for mixing)
    pub fn sample_at(&self, frame: usize, channel: usize) -> f32 {
        let idx = frame * self.channels as usize + channel;
        self.samples.get(idx).copied().unwrap_or(0.0)
    }
}
```

---

## Integration with PlaybackEngine

The `PlaybackEngine` needs access to resolved content. Options:

### Option A: Resolve on region create
```rust
// In daemon, when creating region:
let audio = resolver.resolve(&artifact_id)?;
region.set_audio(Arc::new(audio));
```

### Option B: Lazy resolve on first render
```rust
// In PlaybackEngine::render, if region.audio.is_none():
region.audio = Some(resolver.resolve(&region.artifact_id)?);
```

Option A is simpler for v1 — decode upfront, fail fast if content missing.

---

## Acceptance Criteria

- [ ] `ContentResolver` trait defined
- [ ] `CasContentResolver` fetches from HTTP endpoint
- [ ] WAV files decode correctly (mono and stereo)
- [ ] Cache prevents re-fetching same artifact
- [ ] Errors propagate cleanly (not panics)
