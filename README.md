# Hootenanny 🎵

**Where AI agents jam together and make music.**

Hootenanny is an MCP server for collaborative human-AI music creation. We're building an ensemble performance space where Claude, Gemini, and Orpheus create music together through intention, emergence, and a little chaos.

## ⚡ Quick Start

```bash
# Run the server
cargo run -p hootenanny

# With auto-reload (recommended for development)
cargo watch -x 'run -p hootenanny'

# Connect from Claude Code, Gemini CLI, or any MCP client
# Streamable HTTP (recommended): http://127.0.0.1:8080/mcp
# SSE (legacy):                  http://127.0.0.1:8080/mcp/sse
```

## 🎭 What We Built

### 🎵 Music Generation (Orpheus Models)
```
orpheus_generate          Generate MIDI from scratch
orpheus_generate_seeded   Generate from a seed MIDI
orpheus_continue          Continue existing MIDI
orpheus_bridge            Create transitions between sections
orpheus_loops             Generate loopable MIDI patterns
orpheus_classify          Classify MIDI as human/AI generated
```

All async - launch jobs, get `job_id` back instantly, poll when ready.

### 🤖 Audio AI Models
```
musicgen_generate         Text-to-audio generation (Meta MusicGen)
yue_generate              Lyrics-to-song with structure markers
clap_analyze              Genre/mood classification, embeddings, similarity
beatthis_analyze          Beat/downbeat detection, BPM estimation
```

### 🎼 ABC Notation
```
abc_parse                 Parse ABC notation → AST
abc_to_midi               Convert ABC → MIDI artifact
abc_validate              Validate syntax, get feedback
abc_transpose             Transpose by semitones or to key
```

### 🔊 Audio & Conversion
```
convert_midi_to_wav       Render MIDI → WAV with SoundFont
soundfont_inspect         List SoundFont presets
soundfont_preset_inspect  Inspect specific preset
```

### 💾 Content-Addressable Storage (CAS)
```
cas_store                 Store base64 content → hash
cas_inspect               Get metadata for hash
cas_get                   Retrieve content by hash
```

BLAKE3 hashing, automatic deduplication, all content addressable.

### 📦 Artifacts (Shareable Links!)

Artifacts wrap CAS content with context:
- **HTTP Access**: `GET /artifact/{id}` streams content with MIME type
- **Metadata**: `GET /artifact/{id}/meta` returns JSON with lineage
- **Listing**: `GET /artifacts?tag=X&creator=Y` filters artifacts
- **Tracking**: Access counts, last accessed timestamps

```
artifact_upload           Upload file from disk as artifact
artifact_list             List artifacts (filter by tag, creator)
artifact_get              Get artifact metadata by ID
add_annotation            Add vibes/notes to artifacts
```

```javascript
// Generate something
job = orpheus_generate({temperature: 1.0})
result = job_poll({job_ids: [job.job_id], timeout_ms: 60000})

// Share via artifact URL (not raw CAS hash!)
// http://localhost:8082/artifact/artifact_abc123def456
```

### ⚡ Async Job System
```
job_status                Check job state
job_list                  List all jobs
job_cancel                Cancel running job
job_poll                  Wait for completion (any/all modes)
job_sleep                 Sleep for duration
```

All slow operations return `job_id` immediately:

```javascript
// Launch 3 generations in parallel
jobs = [
    orpheus_generate({temperature: 0.8}),
    orpheus_generate({temperature: 1.0}),
    orpheus_generate({temperature: 1.2})
]

// Wait for first one
result = job_poll({
    timeout_ms: 60000,
    job_ids: jobs.map(j => j.job_id),
    mode: "any"
})

// Or wait for all
result = job_poll({timeout_ms: 120000, job_ids: [...], mode: "all"})
```

### 🎛️ Audio Graph
```
graph_bind                Bind identity to device
graph_tag                 Tag an identity
graph_connect             Connect nodes
graph_find                Query identities
graph_context             Get recent artifacts/context for LLM
graph_query               Trustfall queries across artifacts & devices
```

### 🎬 Timeline (Chaosgarden)
```
garden_status             Get playback state
garden_play/pause/stop    Transport controls
garden_seek               Seek to beat position
garden_set_tempo          Set BPM
garden_create_region      Place content on timeline
garden_delete_region      Remove region
garden_move_region        Reposition region
garden_get_regions        Query timeline regions
garden_query              Trustfall queries on garden state
```

### ⚙️ Configuration
```
config_get                Get configuration values (paths, ports, models)
```

## 🚀 Deployment

Systemd user units are provided for running the full suite as a service.
See `systemd/README.md` for installation instructions.

## 🌐 HTTP Endpoints

**Holler (MCP Gateway) - Port 8080**
```
POST /mcp                       MCP Streamable HTTP (recommended)
GET  /mcp/sse                   MCP SSE transport (legacy)
GET  /health                    Gateway health
```

**Hootenanny (Artifacts & Backend) - Port 8082**
```
GET  /artifact/{id}             Stream artifact content (MIME-typed)
GET  /artifact/{id}/meta        Artifact metadata as JSON
GET  /artifacts                 List artifacts (filterable)
GET  /health                    Backend health
```

**Luanette (Lua Scripts) - Port 8081**
```
POST /mcp                       MCP Streamable HTTP (for script tools)
```

## 🎯 Real-World Examples

### Generate and Render Music
```javascript
// Generate MIDI
gen = orpheus_generate({temperature: 1.0, max_tokens: 512})
result = job_poll({job_ids: [gen.job_id], timeout_ms: 60000})

// Render to WAV
wav = convert_midi_to_wav({
    input_hash: result.completed[0].result.output_hashes[0],
    soundfont_hash: "<your-soundfont-hash>",
    sample_rate: 44100
})
wav_result = job_poll({job_ids: [wav.job_id], timeout_ms: 30000})

// Play via artifact URL
// http://localhost:8080/artifact/artifact_...
```

### ABC Notation to MIDI
```javascript
abc = abc_to_midi({
    abc: `X:1
T:Simple Melody
M:4/4
K:C
CDEF|GABc|`,
    tempo_override: 120
})
// Returns artifact_id for the MIDI
```

### Parallel Generation
```javascript
// Launch multiple jobs
jobs = []
for (const temp of [0.8, 1.0, 1.2]) {
    const job = orpheus_generate({
        temperature: temp,
        variation_set_id: "experiment-1"
    })
    jobs.push(job.job_id)
}

// Wait for all
result = job_poll({timeout_ms: 120000, job_ids: jobs, mode: "all"})
// All variations tagged with same variation_set_id
```

## 🏗️ Architecture

**Crates:**
- `hootenanny` - Backend server: artifacts, CAS, Orpheus, audio graph
- `holler` - MCP gateway: routes tools to backends via ZMQ
- `hooteproto` - ZMQ protocol: Envelope, Payload, frame codec
- `hooteconf` - Configuration: bootstrap and infrastructure settings
- `chaosgarden` - Timeline engine: regions, transport, playback
- `luanette` - Lua scripting: custom tools and automation
- `baton` - MCP protocol implementation
- `abc` - ABC notation parser and MIDI converter
- `resonode` - Musical domain types
- `audio-graph-mcp` - Audio routing graph with Trustfall

**Key Patterns:**
- **Async-by-design:** Slow tools return `job_id` immediately
- **Rich types:** `ContentHash`, `ArtifactId`, `VariationSetId` (no primitive obsession)
- **Artifact-centric:** Share artifacts, not raw hashes
- **Content-addressable:** BLAKE3 hashing, automatic dedup
- **Tool prefixes:** `cas_*`, `job_*`, `orpheus_*`, `abc_*`, `convert_*`, `graph_*`

## 🐍 Python / Vibeweaver Setup

Vibeweaver embeds system Python via PyO3 for interactive music scripting. Install the music modules:

```bash
pip install --user mido numpy mingus pretty-midi
```

These are then available in `weave_eval` for creating MIDI, working with music theory, etc.

## 🛠️ Development

```bash
cargo install cargo-watch just  # Auto-reload + task runner

# Terminal 1: Server with auto-reload
cargo watch -x 'run -p hootenanny'

# Terminal 2: Claude Code or other MCP client
# /mcp to reconnect after changes
```

**Using jj (Jujutsu):**
```bash
jj new -m "feat: your feature"     # Start new work
jj describe                         # Update as you learn
jj git push -c @                    # Share your work
```

## 📝 Documentation

- `CLAUDE.md` / `docs/BOTS.md` - Agent context
- `docs/agents/` - Agent memory system
- Tool descriptions built into MCP (`list_tools`)

## 📊 Status

✅ Async job system with polling
✅ Orpheus MIDI generation (6 modes including loops & classify)
✅ ABC notation → MIDI
✅ Audio rendering (MIDI → WAV)
✅ Artifact tracking with access logs
✅ Audio graph with Trustfall queries
✅ Beat detection (BeatThis)
✅ Audio AI: MusicGen, CLAP analysis, YuE
✅ Chaosgarden timeline/playback
✅ ZMQ mesh: Holler ↔ Hootenanny ↔ Luanette ↔ Chaosgarden

---

**Contributors**: Amy Tobey, Claude, Gemini
**Last Updated**: 2025-12-15
