# 09: Audio Integration (rustysynth + symphonia)

**Focus:** Bridging generation (hootenanny) and playback (chaosgarden)
**Dependencies:** `symphonia`, `rustysynth`, CAS access

---

## The Core Insight

**Generation is slow. Playback must be fast.**

```
Intent → Generation → Rendering → Playback
(latent)  (orpheus)    (rustysynth)  (symphonia)
   │         │             │            │
   │         │             │            └─ RT: stream pre-rendered audio
   │         │             └─ Offline: MIDI → WAV via SoundFont
   │         └─ Async: seconds to minutes
   └─ Declarative: "I want a melody here"
```

Each stage produces artifacts consumed by the next. The key is:
- **hootenanny** handles slow generation/rendering (job-based, async)
- **chaosgarden** handles fast playback (stream samples, RT)
- **CAS** bridges them (content hashes)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    hootenanny (control plane)                │
│                                                              │
│  Generation Tools         Rendering Tools        Analysis    │
│  ┌──────────────┐        ┌──────────────┐     ┌──────────┐  │
│  │orpheus_gen   │        │midi_to_wav   │     │beat_detect│  │
│  │musicgen_gen  │        │(rustysynth)  │     │(symphonia)│  │
│  │abc_to_midi   │        └──────────────┘     └──────────┘  │
│  └──────────────┘               │                           │
│         │                       │                           │
│         ▼                       ▼                           │
│    ┌─────────────────────────────────────┐                  │
│    │              CAS (artifacts)         │                  │
│    │  MIDI files, WAV files, SoundFonts  │                  │
│    └─────────────────────────────────────┘                  │
└─────────────────────────────────────────────────────────────┘
                            │
              (ZMQ: content hashes, commands)
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                   chaosgarden (RT daemon)                    │
│                                                              │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐     │
│  │AudioFileNode │──▶│  MixerNode   │──▶│ExternalOutput│     │
│  │ (symphonia)  │   │              │   │  (PipeWire)  │     │
│  └──────────────┘   └──────────────┘   └──────────────┘     │
│         │                                                    │
│    CasClient (reads audio by content_hash)                  │
└─────────────────────────────────────────────────────────────┘
```

---

## Why This Split?

| Concern | hootenanny | chaosgarden |
|---------|------------|-------------|
| **Latency** | Seconds OK | Milliseconds required |
| **CPU** | Burst OK | Steady, bounded |
| **Blocking** | Acceptable | Never |
| **Memory** | Flexible | Predictable |
| **Dependencies** | Heavy OK | Minimal preferred |

**rustysynth** in hootenanny because:
- MIDI → WAV is a batch operation (render whole file)
- Not latency-critical (job can take seconds)
- Heavy initialization (load SoundFont)

**symphonia** in chaosgarden because:
- Decoding is incremental (stream chunks)
- Latency-critical (feed RT audio graph)
- Light per-frame cost after init

---

## Phase 1: AudioFileNode (Immediate Need)

Enable chaosgarden to play back pre-rendered audio from CAS.

### Types

```rust
// In chaosgarden/src/nodes/audio_file.rs

/// Streams audio from a decoded file
pub struct AudioFileNode {
    descriptor: NodeDescriptor,
    content_hash: String,

    // Decoded audio (loaded on first process or explicit load)
    samples: Option<DecodedAudio>,

    // Playback state
    playhead: usize,  // current sample position
    looping: bool,
}

/// Decoded audio ready for playback
pub struct DecodedAudio {
    pub samples: Vec<f32>,      // Interleaved stereo
    pub sample_rate: u32,
    pub channels: u8,
    pub duration_samples: usize,
}

/// How to access CAS content
pub trait ContentResolver: Send + Sync {
    fn resolve(&self, content_hash: &str) -> Result<Vec<u8>>;
}
```

### Integration with CAS

```rust
impl AudioFileNode {
    pub fn new(content_hash: &str, resolver: Arc<dyn ContentResolver>) -> Self;

    /// Pre-load audio (call before RT playback)
    pub fn preload(&mut self) -> Result<()>;

    /// Seek to position
    pub fn seek(&mut self, sample: usize);
}

impl Node for AudioFileNode {
    fn process(&mut self, ctx: &ProcessContext, ...) -> Result<(), ProcessError> {
        let audio = self.samples.as_ref()
            .ok_or(ProcessError::Skipped { reason: "not loaded" })?;

        // Copy samples to output buffer
        let start = self.playhead;
        let end = (start + ctx.buffer_size).min(audio.duration_samples);

        // Handle end of file
        if start >= audio.duration_samples {
            if self.looping {
                self.playhead = 0;
            } else {
                return Err(ProcessError::Skipped { reason: "end of file" });
            }
        }

        // Copy to output...
        self.playhead = end;
        Ok(())
    }
}
```

### Symphonia Decoder

```rust
// Wrap symphonia for our use case
pub fn decode_audio(data: &[u8]) -> Result<DecodedAudio> {
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::probe::Hint;

    let cursor = std::io::Cursor::new(data);
    let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

    let probed = symphonia::default::get_probe()
        .format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let mut format = probed.format;
    let track = format.default_track().ok_or(anyhow!("no audio track"))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

    let mut samples = Vec::new();

    while let Ok(packet) = format.next_packet() {
        let decoded = decoder.decode(&packet)?;
        // Convert to f32 interleaved...
        append_samples(&mut samples, &decoded);
    }

    Ok(DecodedAudio {
        samples,
        sample_rate: track.codec_params.sample_rate.unwrap_or(48000),
        channels: track.codec_params.channels.map(|c| c.count() as u8).unwrap_or(2),
        duration_samples: samples.len() / 2,
    })
}
```

---

## Phase 2: CAS Client for chaosgarden

chaosgarden needs to read from CAS without going through MCP.

### Option A: Direct File Access
```rust
// CAS is just a directory structure
pub struct FileCasClient {
    base_path: PathBuf,
}

impl ContentResolver for FileCasClient {
    fn resolve(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.base_path.join(&hash[0..2]).join(hash);
        std::fs::read(path).context("CAS read failed")
    }
}
```

### Option B: HTTP Access
```rust
// CAS exposed via hootenanny HTTP
pub struct HttpCasClient {
    base_url: String,
    client: reqwest::Client,
}

impl ContentResolver for HttpCasClient {
    fn resolve(&self, hash: &str) -> Result<Vec<u8>> {
        let url = format!("{}/cas/{}", self.base_url, hash);
        let resp = self.client.get(&url).send()?.bytes()?;
        Ok(resp.to_vec())
    }
}
```

### Recommendation

**Phase 1:** Direct file access (simple, fast, no network)
**Later:** HTTP for distributed setups

---

## Phase 3: Region → AudioFileNode Wiring

When PlaybackEngine encounters a resolved region, it needs to create/manage AudioFileNodes.

```rust
impl PlaybackEngine {
    /// Called when a region becomes active
    fn activate_region(&mut self, region: &Region, resolver: &dyn ContentResolver) {
        match &region.behavior {
            Behavior::PlayContent { content_hash, content_type, .. } => {
                match content_type {
                    ContentType::Audio => {
                        let mut node = AudioFileNode::new(content_hash, resolver);
                        node.preload()?;
                        self.active_nodes.insert(region.id, Box::new(node));
                    }
                    ContentType::Midi => {
                        // MIDI needs rendering first - should already be rendered
                        // or we request rendering via hootenanny
                    }
                }
            }
            _ => {}
        }
    }
}
```

---

## Phase 4: RT MIDI Synthesis (Future)

For live MIDI input → audio, we'd need synthesis in chaosgarden.

```rust
// Future: Only if we need live MIDI → audio
pub struct SoundFontSynthNode {
    synth: rustysynth::Synthesizer,
    soundfont: Arc<rustysynth::SoundFont>,
}

impl Node for SoundFontSynthNode {
    fn process(&mut self, ctx: &ProcessContext, inputs: &[SignalBuffer], outputs: &mut [SignalBuffer]) {
        // Get MIDI events from input
        let midi = inputs[0].as_midi();

        // Feed to synth
        for event in &midi.events {
            match event.message {
                MidiMessage::NoteOn { channel, pitch, velocity } => {
                    self.synth.note_on(channel, pitch, velocity);
                }
                // ...
            }
        }

        // Render audio
        let audio = outputs[0].as_audio_mut();
        self.synth.render(&mut audio.samples);
    }
}
```

**When to add this:**
- When we have hardware MIDI input routed through chaosgarden
- When we want "play and hear immediately" without render step
- NOT needed for playback of pre-composed content

---

## Dependency Strategy

### Cargo.toml Features

```toml
[features]
default = []

# Audio file decoding for playback
symphonia-decode = [
    "dep:symphonia",
    "dep:symphonia-bundle-mp3",
    "dep:symphonia-bundle-flac",
]

# RT MIDI synthesis (future)
realtime-synth = ["dep:rustysynth"]

# Full audio support
audio-full = ["symphonia-decode", "realtime-synth"]

[dependencies]
symphonia = { version = "0.5", optional = true }
symphonia-bundle-mp3 = { version = "0.5", optional = true }
symphonia-bundle-flac = { version = "0.5", optional = true }
rustysynth = { version = "1.3", optional = true }
```

### CI Builds

```yaml
# Fast CI (no audio)
cargo check -p chaosgarden

# Full CI (with audio)
cargo check -p chaosgarden --features audio-full
cargo test -p chaosgarden --features symphonia-decode
```

---

## Content Flow Example

```
1. User creates latent region:
   timeline.add_latent(Beat(0.0), Beat(16.0), "orpheus_generate", {...})

2. Agent triggers generation:
   orpheus_generate({...}) → job_id

3. Job completes:
   → CAS artifact: midi_abc123 (MIDI file)

4. Agent triggers rendering:
   midi_to_wav({midi_hash: "midi_abc123", soundfont_hash: "sf_xyz"})

5. Job completes:
   → CAS artifact: wav_def456 (WAV file)

6. Region resolved:
   region.behavior = PlayContent { content_hash: "wav_def456", content_type: Audio }

7. Playback:
   - PlaybackEngine sees region at Beat(0.0)
   - Creates AudioFileNode with hash "wav_def456"
   - Node loads via CasClient
   - Symphonia decodes WAV
   - Samples stream to graph output
```

---

## Implementation Order

1. **AudioFileNode** — Core playback node with symphonia
2. **FileCasClient** — Direct CAS file access
3. **Region activation** — Wire regions to nodes in PlaybackEngine
4. **Demo update** — Generate MIDI, render to WAV, play back

### Task 09a: AudioFileNode
- `src/nodes/audio_file.rs`
- DecodedAudio struct
- symphonia integration
- Tests with embedded WAV

### Task 09b: CAS Integration
- ContentResolver trait
- FileCasClient implementation
- Config for CAS path

### Task 09c: Playback Wiring
- Extend PlaybackEngine to manage AudioFileNodes
- Region lifecycle (activate/deactivate)
- Seek support

---

## Acceptance Criteria

- [ ] `AudioFileNode` decodes WAV/MP3/FLAC via symphonia
- [ ] `ContentResolver` trait abstracts CAS access
- [ ] `FileCasClient` reads from local CAS directory
- [ ] Regions with `PlayContent::Audio` create AudioFileNodes
- [ ] Demo plays back actual audio through the graph
- [ ] Feature flags work: `--features symphonia-decode`
- [ ] CI passes without audio features (graceful degradation)
