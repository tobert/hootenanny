# Hootenanny 🎵

**An ensemble performance space where AI agents jam together and make music.**

Hootenanny exposes music creation tools via [MCP](https://modelcontextprotocol.io), letting Claude, Gemini, and other AI agents generate, arrange, and play music collaboratively. Connect your favorite AI coding assistant and start making music.

## What It Does

```
┌─────────────────────────────────────────────────────────────┐
│                     Your AI Agent                           │
│              (Claude Code, Gemini CLI, etc.)                │
└─────────────────────────┬───────────────────────────────────┘
                          │ MCP (HTTP)
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                       Holler                                │
│                    (MCP Gateway)                            │
└─────────────────────────┬───────────────────────────────────┘
                          │ ZMQ
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
    ┌───────────┐  ┌───────────┐  ┌───────────┐
    │Hootenanny │  │ Luanette  │  │Chaosgarden│
    │  (Jobs,   │  │   (Lua    │  │ (Timeline │
    │ Artifacts)│  │ Scripting)│  │  Playback)│
    └─────┬─────┘  └───────────┘  └───────────┘
          │
          ▼ HTTP
    ┌───────────┐
    │GPU Models │
    │ (Orpheus, │
    │ MusicGen) │
    └───────────┘
```

**Generate MIDI** with Orpheus - a transformer model that creates musical sequences:
```javascript
result = orpheus_generate({temperature: 1.0, max_tokens: 512})
// Returns job_id immediately, poll when ready
```

**Write music in ABC notation** and convert to MIDI:
```javascript
abc_to_midi({abc: "X:1\nT:Blues\nK:E\nE2 G A B2|"})
```

**Render to audio** with SoundFonts, play on a timeline, analyze with beat detection - all through the same MCP interface.

## Quick Start

```bash
# Clone and build
git clone https://github.com/your-username/hootenanny
cd hootenanny
cargo build --release

# Start the services (requires systemd user units or manual startup)
# See systemd/README.md for service configuration

# Connect from Claude Code
# MCP endpoint: http://127.0.0.1:8080/mcp
```

**Requirements:**
- Rust 1.75+
- Python 3.10+ with `mido`, `numpy`, `pretty-midi`
- GPU services running (Orpheus, etc.) - see infrastructure setup
- Linux with PipeWire (for audio playback)

## 51 Tools

Hootenanny exposes tools organized by function:

| Category | Tools | What They Do |
|----------|-------|--------------|
| **Generation** | `orpheus_*` | MIDI generation, continuation, bridging, loops |
| **ABC** | `abc_*` | Parse, validate, convert ABC notation to MIDI |
| **Audio AI** | `musicgen_*`, `yue_*`, `audio_analyze`, `beats_detect` | Text-to-audio, lyrics-to-song, embeddings, beat detection |
| **Rendering** | `midi_render`, `soundfont_*` | MIDI to WAV with SoundFonts |
| **Timeline** | `timeline_*`, `play`, `pause`, `stop`, `seek`, `tempo` | DAW-like transport and region management |
| **Artifacts** | `artifact_*` | Content-addressed storage with lineage tracking |
| **Jobs** | `job_*` | Async job management with polling |
| **Graph** | `graph_*` | Audio routing and Trustfall queries |
| **Kernel** | `kernel_*` | Embedded Python for music scripting |

Call `help()` from any connected agent to explore.

## Architecture

**Crates:**
- `holler` - MCP gateway, routes tools to backends via ZMQ
- `hootenanny` - Backend: artifacts, jobs, GPU service clients
- `hooteproto` - Wire protocol (Cap'n Proto over ZMQ)
- `chaosgarden` - Timeline engine with PipeWire integration
- `luanette` - Lua scripting for custom workflows
- `baton` - MCP transport library
- `abc` - ABC notation parser

**Patterns:**
- Async by default - slow tools return `job_id` immediately
- Content-addressable - BLAKE3 hashing, automatic dedup
- Artifact-centric - share artifacts with lineage, not raw bytes

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full system diagram.

## Status

This is experimental research software exploring human-AI music collaboration. It works, we use it daily, but expect rough edges.

**What works:**
- MIDI generation with Orpheus (multiple modes)
- ABC notation parsing and MIDI conversion
- Audio rendering with SoundFonts
- Timeline-based playback via PipeWire
- Async job system with parallel generation
- Artifact tracking and content-addressed storage

**What's evolving:**
- Multi-agent coordination patterns
- Real-time collaboration features
- Audio input/recording

## Contributing

Built with Claude and Gemini. Contributions welcome.

```bash
# Development setup
cargo install cargo-watch just
cargo watch -x 'run -p holler'  # Auto-reload
```

## License

MIT

---

**Contributors:** Amy Tobey, Claude, Gemini
