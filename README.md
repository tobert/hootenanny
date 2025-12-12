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
orpheus_bridge            Create bridges between sections
```

All async - launch jobs, get `job_id` back instantly, poll when ready.

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
beatthis_analyze          Detect beats/BPM in audio
```

### 💾 Content-Addressable Storage (CAS)
```
cas_store                 Store base64 content → hash
cas_inspect               Get metadata for hash
cas_upload_file           Upload file from disk → hash
```

BLAKE3 hashing, automatic deduplication, all content addressable.

### 📦 Artifacts (Shareable Links!)

Artifacts wrap CAS content with context:
- **HTTP Access**: `GET /artifact/{id}` streams content with MIME type
- **Metadata**: `GET /artifact/{id}/meta` returns JSON with lineage
- **Listing**: `GET /artifacts?tag=X&creator=Y` filters artifacts
- **Tracking**: Access counts, last accessed timestamps

```javascript
// Generate something
job = orpheus_generate({temperature: 1.0})
result = job_poll({job_ids: [job.job_id], timeout_ms: 60000})

// Share via artifact URL (not raw CAS hash!)
// http://localhost:8080/artifact/artifact_abc123def456
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
```

### 🤖 Agent Chat (LLM Sub-agents)
```
agent_chat_new            Create session with backend
agent_chat_send           Send message
agent_chat_poll           Poll for response
agent_chat_cancel         Cancel session
agent_chat_status         Get session status
agent_chat_history        Get message history
agent_chat_summary        Get AI summary
agent_chat_list           List sessions
agent_chat_backends       List available backends
```

## 🌐 HTTP Endpoints

```
GET  /health                    Server health, uptime, stats

GET  /artifact/{id}             Stream artifact content (MIME-typed)
GET  /artifact/{id}/meta        Artifact metadata as JSON
GET  /artifacts                 List artifacts (filterable)

POST /mcp                       MCP Streamable HTTP (recommended)
GET  /mcp/sse                   MCP SSE transport (legacy)
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
- `hootenanny` - MCP server, tools, job system
- `baton` - MCP protocol implementation
- `abc` - ABC notation parser and MIDI converter
- `resonode` - Musical domain types
- `audio-graph-mcp` - Audio routing graph


**Key Patterns:**
- **Async-by-design:** Slow tools return `job_id` immediately
- **Rich types:** `ContentHash`, `ArtifactId`, `VariationSetId` (no primitive obsession)
- **Artifact-centric:** Share artifacts, not raw hashes
- **Content-addressable:** BLAKE3 hashing, automatic dedup
- **Tool prefixes:** `cas_*`, `job_*`, `orpheus_*`, `abc_*`, `convert_*`, `graph_*`

## 🛠️ Development

```bash
cargo install cargo-watch  # Auto-reload

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
✅ Orpheus MIDI generation (4 modes)
✅ ABC notation → MIDI
✅ Audio rendering (MIDI → WAV)
✅ Artifact tracking with access logs
✅ Audio graph for routing
✅ LLM sub-agent support
✅ Beat detection (BeatThis)

---

**Contributors**: Amy Tobey, Claude, Gemini
**Last Updated**: 2025-12-02
