# Artifact System Quick Reference

> **Universal tracking for all generated content with variation semantics built-in**

---

## What It Does

**9 content-generating MCP tools** now automatically create Artifacts that track:
- The generated content (via CAS hash)
- Variation relationships (sets, parents, siblings)
- Arbitrary tags for organization
- Creator and metadata

**Supported Tools:**
- **Music Generation** (5 tools): `orpheus_generate`, `orpheus_generate_seeded`, `orpheus_continue`, `orpheus_bridge`, `orpheus_loops` → MIDI files
- **Audio Rendering** (1 tool): `midi_to_wav` → WAV audio files
- **Code Generation** (1 tool): `deepseek_query` → Text/code
- **Musical Events** (1 tool): `play` → Realized sound events
- **Intentions** (1 tool): `add_node` → Musical contributions

**Utility Tools (CAS only, no artifacts):**
- `upload_file` - Upload files from disk to CAS (e.g., SoundFonts)
- `cas_store` - Store base64 content in CAS
- `cas_inspect` - Get CAS metadata and local paths

All artifacts stored in `state/artifacts.json` (JSON file, human-readable).

---

## Quick Start

### Basic Usage (No Variation Tracking)

```json
orpheus_generate({
  "temperature": 1.0,
  "max_tokens": 512
})

// Returns:
{
  "output_hash": "5ca7815abc...",
  "artifact_id": "artifact_5ca7815abc12",
  "variation_set_id": null,
  "variation_index": null
}
```

Artifact created automatically with auto-generated ID.

### With Variation Tracking

```json
orpheus_generate({
  "temperature": 1.0,
  "variation_set_id": "vset_intro",
  "tags": ["phase:initial", "experiment:upbeat"],
  "creator": "agent_claude"
})

// Returns:
{
  "output_hash": "abc123...",
  "artifact_id": "artifact_abc123def45",
  "variation_set_id": "vset_intro",
  "variation_index": 0              // Auto-assigned!
}
```

**Next call with same `variation_set_id` gets `variation_index: 1`, then 2, etc.**

### Refinement (Parent/Child)

```json
// First generation
orpheus_generate({
  "variation_set_id": "vset_intro",
  "creator": "agent_claude"
})
// → artifact_id: "artifact_abc123"

// Refine it
orpheus_generate_seeded({
  "seed_hash": "abc123...",
  "variation_set_id": "vset_intro_refined",
  "parent_id": "artifact_abc123",           // Link to parent!
  "tags": ["phase:refinement"]
})
// → Creates child artifact with parent link
```

---

## Core Fields

Every artifact has:

| Field | Type | Description |
|-------|------|-------------|
| `id` | String | Unique artifact ID (auto-generated from hash) |
| `variation_set_id` | Option<String> | Group related variations together |
| `variation_index` | Option<u32> | Position in set (0, 1, 2...) - auto-assigned |
| `parent_id` | Option<String> | Parent artifact for refinements |
| `tags` | Vec<String> | Arbitrary tags (e.g., `["phase:initial"]`) |
| `creator` | String | Who created it (agent ID or user ID) |
| `data` | JSON | Type-specific data (hash, tokens, model, etc.) |

---

## Tool Parameters

All 8 content-generating tools support these **optional** artifact tracking parameters:

```typescript
{
  // Standard generation params (tool-specific)
  temperature?: number,
  max_tokens?: number,
  messages?: Message[],
  // ... other tool-specific params

  // Artifact tracking (all optional)
  variation_set_id?: string,    // Group with other variations
  parent_id?: string,            // Link to parent artifact
  tags?: string[],               // Custom tags
  creator?: string               // Defaults vary by tool
}
```

**Default Creators:**
- Orpheus tools: `"agent_orpheus"`
- DeepSeek: `"unknown"` (specify your agent ID)
- Play/Add Node: Uses `agent_id` from request

---

## Common Patterns

### Pattern 1: Explore Multiple Options

```json
// Generate 3 variations in same set
for (let i = 0; i < 3; i++) {
  orpheus_generate({
    temperature: 1.0,
    variation_set_id: "vset_explore_intro",
    tags: ["phase:exploration"]
  })
}

// All get same variation_set_id, different variation_index (0, 1, 2)
```

### Pattern 2: Progressive Refinement

```json
// 1. Initial generation
const v1 = orpheus_generate({
  variation_set_id: "vset_intro",
  tags: ["phase:initial"]
})
// → artifact_id: "artifact_abc"

// 2. Refine it
const v2 = orpheus_generate_seeded({
  seed_hash: v1.output_hash,
  variation_set_id: "vset_intro_v2",
  parent_id: v1.artifact_id,
  tags: ["phase:refinement", "change:add_harmony"]
})
// → artifact_id: "artifact_def", parent_id: "artifact_abc"

// 3. Refine again
const v3 = orpheus_continue({
  input_hash: v2.output_hash,
  variation_set_id: "vset_intro_v3",
  parent_id: v2.artifact_id,
  tags: ["phase:refinement", "change:extend"]
})
// → artifact_id: "artifact_ghi", parent_id: "artifact_def"
```

Creates a refinement chain: `artifact_abc → artifact_def → artifact_ghi`

### Pattern 3: Ensemble Collaboration

```json
// Each agent generates in same set
agent_claude.orpheus_generate({
  variation_set_id: "vset_ensemble_intro",
  creator: "agent_claude",
  tags: ["role:melody_specialist"]
})

agent_gemini.orpheus_generate({
  variation_set_id: "vset_ensemble_intro",
  creator: "agent_gemini",
  tags: ["role:harmony_specialist"]
})

// Both in same set, different creators and tags
```

### Pattern 4: Code Generation with Refinement

```json
// Initial code generation
deepseek_query({
  messages: [{role: "user", content: "Write a Rust function to parse MIDI"}],
  variation_set_id: "vset_midi_parser",
  tags: ["language:rust", "task:parsing"],
  creator: "agent_claude"
})
// → Returns: {text: "...", artifact_id: "artifact_abc", cas_hash: "..."}

// Refine the code
deepseek_query({
  messages: [{role: "user", content: "Add error handling to that parser"}],
  variation_set_id: "vset_midi_parser_v2",
  parent_id: "artifact_abc",
  tags: ["language:rust", "task:parsing", "phase:refinement"],
  creator: "agent_claude"
})
```

### Pattern 5: Musical Event Tracking

```json
// Track individual musical contributions
play({
  what: "C",
  how: "boldly",
  valence: 0.8,
  arousal: 0.7,
  agency: 0.6,
  agent_id: "agent_claude",
  variation_set_id: "vset_jam_session_1",
  tags: ["role:lead", "emotion:energetic"]
})
// → Returns: {sound: {...}, artifact_id: "artifact_def", cas_hash: "..."}

// Another agent responds
play({
  what: "E",
  how: "softly",
  valence: 0.5,
  arousal: 0.3,
  agency: -0.2,
  agent_id: "agent_gemini",
  variation_set_id: "vset_jam_session_1",
  parent_id: "artifact_def",
  tags: ["role:harmony", "emotion:calm"]
})
// → Creates call-and-response chain
```

### Pattern 6: Conversational Intentions

```json
// Track musical ideas in conversation tree
add_node({
  what: "D",
  how: "questioning",
  valence: 0.0,
  arousal: 0.5,
  agency: 0.3,
  agent_id: "agent_claude",
  description: "Exploring modal shift",
  variation_set_id: "vset_exploration_phase",
  tags: ["phase:exploration", "technique:modal"]
})
// → Adds to conversation tree + creates artifact
```

### Pattern 7: MIDI to WAV Rendering Pipeline

```json
// 1. Upload SoundFont to CAS (no artifact, utility only)
const sf2 = upload_file({
  file_path: "/path/to/soundfont.sf2",
  mime_type: "audio/soundfont"
})
// → Returns: {hash: "abc123...", size_bytes: 407218}

// 2. Generate MIDI with Orpheus
const midi = orpheus_generate({
  temperature: 1.0,
  max_tokens: 512,
  variation_set_id: "vset_composition_v1",
  tags: ["phase:generation", "instrument:piano"]
})
// → Returns: {output_hash: "def456...", artifact_id: "artifact_def456..."}

// 3. Render MIDI to WAV
const wav = midi_to_wav({
  input_hash: midi.output_hash,
  soundfont_hash: sf2.hash,
  sample_rate: 44100,
  variation_set_id: "vset_composition_v1_audio",
  parent_id: midi.artifact_id,  // Links audio back to MIDI
  tags: ["phase:rendering", "format:wav", "soundfont:tr808"]
})
// → Returns: {
//     output_hash: "ghi789...",
//     artifact_id: "artifact_ghi789...",
//     duration_seconds: 25.16,
//     size_bytes: 4438264
//   }

// 4. Get local path to listen
cas_inspect({hash: wav.output_hash})
// → {local_path: "/path/to/cas/objects/gh/i789...", ...}
```

**Creates lineage:** MIDI artifact → WAV artifact (via parent_id)
**See:** [SoundFont Compatibility](soundfont-compatibility.md) for working SF2 files

---

## Tag Conventions

Suggested tag format: `category:value`

```bash
# Type (auto-applied by tools)
type:midi               # Orpheus tools
type:audio              # MIDI to WAV tool
type:text               # DeepSeek
type:musical_event      # Play tool
type:intention          # Add node tool

# Phase (auto-applied + custom)
phase:generation        # Orpheus, DeepSeek, MIDI to WAV
phase:rendering         # Custom (for audio conversion)
phase:realization       # Play
phase:contribution      # Add node
phase:initial           # Custom
phase:exploration       # Custom
phase:refinement        # Custom
phase:final             # Custom

# Language (for code)
language:rust
language:python
language:javascript

# Task (for code/music)
task:parsing
task:debugging
task:refactoring
task:melody
task:harmony

# Role (ensemble)
role:melody_specialist
role:harmony_specialist
role:producer
role:lead
role:harmony

# Emotion (musical events)
emotion:energetic
emotion:calm
emotion:joyful
emotion:melancholy

# Technique
technique:modal
technique:counterpoint
technique:improvisation

# Experiment
experiment:upbeat
experiment:jazzy

# Quality
quality:high_energy
quality:smooth

# Format (for audio/MIDI)
format:wav
format:midi
format:mp3

# SoundFont (for rendered audio)
soundfont:tr808
soundfont:ff6
soundfont:timber
soundfont:generaluser

# Instrument
instrument:piano
instrument:drums
instrument:synth

# Custom (anything!)
my_project_tag
test_run_42
```

---

## Storage

**Location:** `state/artifacts.json`

**Format:** Pretty-printed JSON array

```json
[
  {
    "id": "artifact_5ca7815abc12",
    "variation_set_id": "vset_intro",
    "variation_index": 0,
    "parent_id": null,
    "tags": ["type:midi", "phase:initial", "experiment:upbeat"],
    "created_at": "2025-11-21T03:00:00Z",
    "creator": "agent_claude",
    "data": {
      "hash": "5ca7815abc...",
      "tokens": 512,
      "model": "base",
      "temperature": 1.0,
      "task": "generate"
    }
  }
]
```

**Automatic:** Saved to disk after every tool call.

---

## Querying (Future: Lua)

Currently: Just a JSON file (read with any JSON tool).

**Coming soon:** Lua query interface

```lua
-- Get all variations in a set
variations = get_variation_set(store, "vset_intro")

-- Get refinement chain
chain = get_refinement_chain(store, "artifact_abc")

-- Filter by tags
high_energy = filter_by_tag(store, "quality:high_energy")
```

---

## Workflow Example

**Goal:** Create intro melody, explore variations, refine best one

```javascript
// 1. Initial exploration (5 variations)
const set_id = "vset_morning_intro"
for (let i = 0; i < 5; i++) {
  orpheus_generate({
    temperature: 1.0,
    variation_set_id: set_id,
    tags: ["phase:exploration"],
    creator: "agent_claude"
  })
}

// 2. (Manually review artifacts.json to pick best)
// Say variation 2 was best: artifact_xyz789

// 3. Refine the winner
orpheus_generate_seeded({
  seed_hash: "xyz789...",
  variation_set_id: "vset_morning_intro_refined",
  parent_id: "artifact_xyz789",
  tags: ["phase:refinement", "action:add_harmony"],
  creator: "agent_claude"
})

// 4. Continue/extend it
orpheus_continue({
  input_hash: "...",
  variation_set_id: "vset_morning_intro_final",
  parent_id: "artifact_refined_id",
  tags: ["phase:final"],
  creator: "agent_claude"
})
```

**Result:** Clear artifact lineage tracking exploration → refinement → final.

---

## Tips

✅ **DO:**
- Use descriptive `variation_set_id` (e.g., `vset_morning_intro`)
- Tag consistently (`phase:`, `role:`, `experiment:`)
- Set `creator` to track multi-agent work
- Use `parent_id` for refinement chains

❌ **DON'T:**
- Reuse `variation_set_id` for unrelated work
- Leave important context out of tags
- Forget that variation_index is auto-assigned (don't specify it!)

---

## Auto-Applied Tags

Every artifact automatically gets tags based on the tool used:

### Orpheus Tools (5)
- `type:midi`
- `phase:generation`
- `tool:orpheus_generate`, `tool:orpheus_generate_seeded`, `tool:orpheus_continue`, `tool:orpheus_bridge`, or `tool:orpheus_loops`

### DeepSeek Tool (1)
- `type:text`
- `phase:generation`
- `tool:deepseek_query`

### Play Tool (1)
- `type:musical_event`
- `phase:realization`
- `tool:play`

### Add Node Tool (1)
- `type:intention`
- `phase:contribution`
- `tool:add_node`

### MIDI to WAV Tool (1)
- `type:audio`
- `phase:generation`
- `tool:midi_to_wav`

Your custom tags are **added** to these auto-applied tags.

---

## See Also

- **Architecture:** `docs/ARCHITECTURE.md`
- **CAS HTTP API:** `docs/CAS_HTTP_API.md`
- **SoundFont Compatibility:** `docs/soundfont-compatibility.md`
