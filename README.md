# HalfRemembered MCP 🎵

**Where AI agents jam together and make music.**

An MCP server for collaborative human-AI music creation. We're building an ensemble performance space where Claude, Gemini, DeepSeek, and Orpheus create music together through intention, emergence, and a little chaos.

## ⚡ Quick Start

```bash
# Run the server with auto-reload
cargo watch -x 'run -p hootenanny'

# Connect from Claude Code, Gemini CLI, or any MCP client
# Server runs on http://127.0.0.1:8080/sse
```

## 🎭 What We Built

This isn't your typical MCP server. We've got:

### 🎵 **Music Generation** (Orpheus Models)
- `orpheus_generate` - Generate MIDI from scratch (async!)
- `orpheus_loops` - Multi-instrumental loops
- `orpheus_bridge` - Musical bridges between sections
- `orpheus_continue` - Continue existing MIDI sequences
- `orpheus_generate_seeded` - Seed-based variations
- `orpheus_classify` - Human vs AI music detector

All async-by-design. Launch jobs, get job_id back instantly, poll when ready.

### 🔊 **Audio Rendering**
- `midi_to_wav` - RustySynth rendering (async)
- SoundFont support
- HTTP streaming via `http://0.0.0.0:8080/cas/<hash>`

### 💾 **Content-Addressable Storage**
- BLAKE3 hashing for all content
- Store MIDI, audio, text, anything
- `cas_store`, `cas_inspect`, `upload_file`
- Automatic deduplication

### 🤖 **Code Generation**
- `deepseek_query` - Local DeepSeek Coder (async)
- Chat-style API
- Results stored in CAS

### ⚡ **Async Job System**
All slow operations return job_id immediately:

```javascript
// Launch 3 generations in parallel
job1 = orpheus_generate({temp: 0.8})
job2 = orpheus_generate({temp: 1.0})
job3 = orpheus_generate({temp: 1.2})

// Wait for first one
result = poll({timeout_ms: 60000, job_ids: [job1, job2, job3], mode: "any"})

// Or wait for all
result = poll({timeout_ms: 120000, job_ids: [...], mode: "all"})
```

Job management tools:
- `poll(timeout, jobs, mode)` - Flexible waiting (any/all modes)
- `sleep(milliseconds)` - Simple delays
- `get_job_status(job_id)` - Check one job
- `wait_for_job(job_id)` - Block until complete
- `list_jobs()` - See everything running
- `cancel_job(job_id)` - Abort running job

### 📦 **Artifact Tracking**
Every generation is tracked with metadata:
- Variation sets and indices
- Parent/child lineage
- Tags for organization
- Creator attribution
- Rich type system (no primitive obsession!)

### 🌳 **Conversation Tree** (Musical Intentions)
Event Duality architecture - intentions become sounds:

```rust
enum Event {
    Abstract(Intention),  // "play C softly"
    Concrete(Sound),      // pitch:60, velocity:40
}
```

Tools:
- `play` - Transform intention → sound
- `add_node` - Add to conversation tree
- `fork_branch` - Explore alternative directions
- `merge_branches` - Bring ideas together
- `get_tree_status` - See the full state

## 🚀 Running the Server

### Development Mode (recommended)
```bash
cargo watch -x 'run -p hootenanny'
```

Auto-rebuilds when you change code. Pairs beautifully with Claude Code's `/mcp` reconnect.

### Basic Mode
```bash
cargo run -p hootenanny
```

### Connecting from Clients

**Claude Code:** Just run `/mcp` after starting the server

**Claude CLI:**
```bash
claude mcp add --transport sse hrmcp http://127.0.0.1:8080/sse
```

**Gemini CLI:**
```bash
gemini mcp add hrmcp http://127.0.0.1:8080/sse
```

## 🎯 Real-World Examples

### Generate Music Variations
```javascript
// Launch 3 variations with different temperatures
jobs = []
for temp in [0.8, 1.0, 1.2]:
    job = orpheus_generate({
        temperature: temp,
        max_tokens: 256,
        num_variations: 1,
        variation_set_id: "experiment-1"
    })
    jobs.push(job.job_id)

// Wait for all to complete
result = poll({timeout_ms: 120000, job_ids: jobs, mode: "all"})

// Now all variations are ready with shared variation_set_id
```

### Render to Audio
```javascript
// Generate MIDI
gen = orpheus_loops({max_tokens: 512})
result = wait_for_job(gen.job_id)

// Render to WAV
wav = midi_to_wav({
    input_hash: result.output_hashes[0],
    soundfont_hash: "<your-soundfont-hash>",
    sample_rate: 44100
})
wav_result = wait_for_job(wav.job_id)

// Play in browser: http://0.0.0.0:8080/cas/<wav_result.output_hash>
```

### Ask DeepSeek for Help
```javascript
job = deepseek_query({
    messages: [{
        role: "user",
        content: "Write a Rust function to parse MIDI events"
    }]
})

result = wait_for_job(job.job_id)
// Code is in result.text and stored in CAS
```

## 🏗️ Architecture

**Crates:**
- `hootenanny` - MCP server, job system, tools
- `resonode` - Musical domain (Event Duality, scales, realization)

**Key Patterns:**
- **Async-by-design:** All slow tools return job_id immediately
- **Type-rich domain:** No primitive obsession, enums tell stories
- **Content-addressable:** Everything has a hash, nothing duplicates
- **Lineage tracking:** Know where every artifact came from
- **SSE transport:** Multi-client HTTP streaming

## 🧠 For Agent Developers

**Using the async tools:**
- ALL Orpheus tools are async (return job_id)
- `midi_to_wav` and `deepseek_query` are async
- Use `poll()` for flexible waiting
- Use `sleep()` when you just need a delay
- Check `CLAUDE.md` / `BOTS.md` for full agent context

**The pattern:**
1. Launch job → get `job_id`
2. Do other work (or launch more jobs!)
3. `poll()` or `wait_for_job()` to get results
4. Results include artifact_ids with full lineage

**Natural parallelism:**
```javascript
// This is the way
jobs = [
    orpheus_generate({temp: 0.9}),
    orpheus_loops({num_variations: 2}),
    deepseek_query({messages: [...]})
]

// All running in parallel!
results = poll({timeout_ms: 120000, job_ids: jobs.map(j => j.job_id), mode: "all"})
```

## 📊 Current Status

✅ **Async job system** - Background tasks, polling, status tracking
✅ **Orpheus integration** - 6 model variants, all async
✅ **Audio rendering** - MIDI→WAV with RustySynth
✅ **CAS** - BLAKE3 content storage
✅ **Artifact tracking** - Variations, lineage, metadata
✅ **DeepSeek** - Local code generation
✅ **Conversation tree** - Musical intention → sound

## 🎨 The Vision

Building a real-time music generation system that is **fast, weird, and expressive**. An ensemble of AI models (Orpheus for music, DeepSeek for code, Claude/Gemini for reasoning) jamming together through:

- Multi-agent musical dialogue
- Conversation trees for improvisation
- Temporal forking (explore alternate takes)
- Real-time performance

Think: **Git for music improvisation** + **MCP for agent collaboration** + **Actually sounds good**

## 🛠️ Development

**Tools you'll want:**
```bash
cargo install cargo-watch  # Auto-reload on changes
```

**Workflow:**
```bash
# Terminal 1: Server with auto-reload
cargo watch -x 'run -p hootenanny'

# Terminal 2: Claude Code or other MCP client
# Just /mcp to reconnect after code changes
```

**Using jj (Jujutsu):**
We use jj for version control. See `CLAUDE.md` for the full workflow. Key commands:

```bash
jj new -m "feat: your feature"     # Start new work
jj describe                         # Update description as you learn
jj git push -c @                    # Share your work
```

## 📝 Documentation

- `CLAUDE.md` / `BOTS.md` - Agent context (read this if you're an LLM!)
- `docs/agents/` - Agent memory system (NOW.md, PATTERNS.md, CONTEXT.md)
- Tool descriptions - Built into MCP (list_tools)

## 🎵 Try It

```javascript
// The original hello world - still works!
play({what: "C", how: "softly"})
// → {pitch: 60, velocity: 40}

// But now we can do this:
job1 = orpheus_loops({temperature: 1.1, max_tokens: 256})
job2 = orpheus_generate({temperature: 0.9, max_tokens: 128})

results = poll({timeout_ms: 60000, job_ids: [job1, job2], mode: "all"})

// Two pieces of original music, generated in parallel, ready to jam
```

---

**Status**: ✅ Making music
**Contributors**: Amy Tobey, Claude, Gemini, DeepSeek
**Vibe**: 🎵 Let's jam
**Last Updated**: 2025-11-22 (Async job system release)
