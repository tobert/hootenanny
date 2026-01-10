# Hootenanny 🎵

**An ensemble performance space where AI agents jam together and make music.**

Hootenanny exposes music creation tools via [MCP](https://modelcontextprotocol.io), letting Claude, Gemini, and other tool callers generate, arrange, and play music collaboratively. Connect your favorite AI coding assistant and start making music.

## ✨ What Can You Do?

**Generate MIDI** — Orpheus creates musical sequences from nothing:
```javascript
orpheus_generate({
  temperature: 1.0,
  max_tokens: 512,
  tags: ["ambient", "exploration"]
})
// → { job_id: "abc123..." }  // Async - poll when ready
```

**Write music in ABC notation** — Human-readable music → MIDI:
```javascript
abc_to_midi({
  abc: `X:1
T:Midnight Blues
K:Em
E2 G A B2 | B2 A G E2 |
G2 E D E2 | E4 z2 |`,
  tempo_override: 72,
  transpose: -2
})
// → { artifact_id: "...", content_hash: "..." }
```

**Continue where you left off** — Extend existing MIDI:
```javascript
orpheus_continue({
  input_hash: "hash_of_your_midi...",
  temperature: 1.1,
  num_variations: 3
})
```

**Bridge sections** — Smooth transitions between parts:
```javascript
orpheus_bridge({
  section_a_hash: "verse_hash...",
  section_b_hash: "chorus_hash...",
  max_tokens: 128
})
```

**Render to audio** — MIDI + SoundFont → WAV:
```javascript
midi_render({
  input_hash: "midi_hash...",
  soundfont_hash: "sf2_hash...",
  sample_rate: 44100
})
```

**Play on a timeline** — DAW-style transport with beat-based timing:
```javascript
timeline_region_create({
  position: 0,      // Start at beat 0
  duration: 8,      // 8 beats long
  behavior_type: "play_audio",
  content_id: "artifact_123"
})
play({})
tempo({ bpm: 120 })
```

**Detect beats** — Analyze audio for rhythm:
```javascript
beats_detect({ audio_hash: "wav_hash..." })
// → Beat positions, downbeats, frame-level activations
```

**Query with Trustfall** — Find artifacts by lineage, tags, vibes:
```javascript
graph_query({
  query: `{
    Artifact(tag: "type:midi") {
      id @output
      creator @output
      tags { tag @output @filter(op: "has_substring", value: ["jazzy"]) }
    }
  }`
})
```

---

## 🚀 Quick Start

```bash
# Clone and build
git clone https://github.com/anthropics/hootenanny
cd hootenanny
cargo build --release

# Start the services
./target/release/hootenanny &   # Control plane (port 5580)
./target/release/holler serve & # MCP gateway (port 8080)
./target/release/chaosgarden &  # Audio daemon

# Configure Claude Code
# Add to your MCP config:
{
  "mcpServers": {
    "holler": {
      "command": "/path/to/holler",
      "args": ["mcp"]
    }
  }
}
```

**Requirements:**
- Rust 1.75+
- Linux with PipeWire (audio playback)
- GPU services for generation (Orpheus, etc.) — see [Infrastructure Setup](docs/INFRASTRUCTURE.md)
- Python 3.10+ with `mido`, `numpy` (for vibeweaver kernel)

---

## 🔧 51 Tools

Organized by prefix for discoverability. Call `help()` to explore.

| Prefix | Domain | Key Tools |
|--------|--------|-----------|
| `orpheus_*` | MIDI generation | `generate`, `continue`, `bridge` |
| `abc_*` | ABC notation | `validate`, `to_midi` |
| `midi_*` | MIDI operations | `render`, `classify`, `info` |
| `audio_*` | Audio I/O | `output_attach`, `input_attach`, `monitor` |
| `musicgen_*` | Text→audio | `generate` |
| `yue_*` | Lyrics→song | `generate` |
| `beats_detect` | Rhythm analysis | Beat/downbeat detection |
| `audio_analyze` | CLAP embeddings | Classification, similarity |
| `timeline_*` | Playback | `region_create`, `region_move`, `clear` |
| `play/pause/stop/seek/tempo` | Transport | DAW controls |
| `artifact_*` | Storage | `upload`, `list`, `get` |
| `job_*` | Async jobs | `poll`, `list`, `cancel` |
| `graph_*` | Queries | `query`, `context`, `bind`, `connect` |
| `kernel_*` | Python | `eval`, `session`, `reset` |
| `config/status/storage_stats` | System | Diagnostics |

---

## 🏗 Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           AI Agents                                     │
│              (Claude Code, Gemini, custom MCP clients)                  │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │ HTTP/SSE (MCP Protocol)
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                            HOLLER                                       │
│                        MCP ↔ ZMQ Gateway                                │
│  • Routes tool calls to backends    • CLI for manual testing            │
│  • Broadcasts events via SSE        • Dynamic tool discovery            │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │ ZMQ (hooteproto / Cap'n Proto)
               ┌─────────────────┼─────────────────┐
               ▼                 ▼                 ▼
┌──────────────────────┐ ┌─────────────┐ ┌────────────────────────────────┐
│     HOOTENANNY       │ │  VIBEWEAVER │ │         CHAOSGARDEN            │
│   (Control Plane)    │ │  (Python)   │ │      (Realtime Audio)          │
│                      │ │             │ │                                │
│ • Job orchestration  │ │ • Kernel    │ │ • PipeWire integration         │
│ • Artifact store     │ │ • Sessions  │ │ • Timeline playback            │
│ • GPU service calls  │ │ • Rules     │ │ • Beat-synced transport        │
│ • Trustfall queries  │ │             │ │ • Audio graph routing          │
└──────────┬───────────┘ └──────┬──────┘ └────────────────────────────────┘
           │                    │
           └────────┬───────────┘
                    ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                         GPU SERVICES (HTTP)                               │
│  Orpheus :2000 │ MusicGen :2006 │ CLAP :2007 │ YuE :2008 │ BeatThis :2012 │
└───────────────────────────────────────────────────────────────────────────┘
```

### Crates

| Crate | Purpose |
|-------|---------|
| **holler** | MCP gateway — routes HTTP/SSE to ZMQ backends |
| **hootenanny** | Control plane — jobs, artifacts, GPU clients, queries |
| **hooteproto** | Wire protocol — Cap'n Proto schemas over ZMQ |
| **chaosgarden** | Audio daemon — PipeWire, timeline, transport |
| **vibeweaver** | Python kernel — PyO3 embedded interpreter |
| **cas** | Content-addressed storage — BLAKE3 hashing |
| **abc** | ABC notation parser and MIDI converter |
| **audio-graph-mcp** | Trustfall adapter for unified queries |
| **hooteconf** | Layered configuration loading |

### Key Design Patterns

**Async by default** — Slow tools return `job_id` immediately:
```javascript
job = orpheus_generate({...})           // Returns instantly
result = job_poll({                     // Wait for completion
  job_ids: [job.job_id],
  timeout_ms: 60000
})
```

**Content-addressable** — BLAKE3 hashing, automatic dedup. Share hashes, not bytes.

**Artifact-centric** — Every piece of content gets lineage tracking:
```javascript
artifact_upload({
  file_path: "/path/to/file.mid",
  mime_type: "audio/midi",
  parent_id: "artifact_that_inspired_this",
  tags: ["variation", "take-2"]
})
```

**Beat-based timing** — Timeline uses beats, not seconds:
```javascript
// Position 4 = beat 4, duration 2 = 2 beats
timeline_region_create({ position: 4, duration: 2, ... })
```

**Lazy Pirate** — Services start in any order. ZMQ handles reconnection.

---

## 🛠 Development

```bash
# Install dependencies
cargo install cargo-watch just

# Run with auto-reload
cargo watch -x 'run -p holler -- serve'

# Run tests
cargo test --workspace

# Build all
cargo build --release
```

### Configuration

Layered config loading: system → user → project → env vars.

```toml
# ~/.config/hootenanny/config.toml
[infra.bind]
http_bind_addr = "127.0.0.1:8080"
zmq_router = "tcp://0.0.0.0:5580"

[bootstrap.models]
orpheus = "http://127.0.0.1:2000"
musicgen = "http://127.0.0.1:2006"

[bootstrap.media]
soundfont_dirs = ["~/midi/SF2", "/usr/share/sounds/sf2"]
```

### Adding Tools

1. Add Cap'n Proto schema in `crates/hooteproto/schemas/tools.capnp`
2. Add Rust types in `crates/hooteproto/src/request.rs`
3. Implement dispatch in `crates/hootenanny/src/api/typed_dispatcher.rs`
4. Register in `crates/holler/src/tools_registry.rs`

See [CLAUDE.md](CLAUDE.md) for detailed guidelines.

---

## 📊 Status

Experimental research software exploring human-AI music collaboration. We use it daily — expect rough edges.

**Working:**
- ✅ MIDI generation (Orpheus: generate, continue, bridge)
- ✅ ABC notation → MIDI conversion
- ✅ Audio rendering with SoundFonts
- ✅ Timeline playback via PipeWire
- ✅ Async job system with parallel generation
- ✅ Artifact tracking with lineage
- ✅ Trustfall queries across the graph
- ✅ Python kernel for scripting

**Evolving:**
- 🔄 Multi-agent coordination patterns
- 🔄 Real-time collaboration features
- 🔄 Audio input/recording
- 🔄 MIDI device integration

---

## 🤝 Contributing

Built collaboratively with Claude and Gemini. Contributions welcome.

See [CLAUDE.md](CLAUDE.md) for coding guidelines — the same instructions we give our AI collaborators.

---

## 📜 License

MIT

---

**Contributors:** Amy Tobey, 🤖 Claude, 💎 Gemini
