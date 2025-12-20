//! Help tool - provides detailed documentation for holler tools on demand.
//!
//! This allows minimal tool descriptions at startup while still providing
//! rich documentation when needed.

use hooteproto::{ToolOutput, ToolResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HelpRequest {
    /// Tool name or category (e.g., "sample", "garden", "all")
    pub topic: Option<String>,
}

/// Get help text for a topic
pub fn get_help(topic: Option<&str>) -> String {
    match topic {
        None | Some("") | Some("all") | Some("overview") => OVERVIEW.to_string(),
        Some("sample") => SAMPLE_HELP.to_string(),
        Some("project") => PROJECT_HELP.to_string(),
        Some("extend") => EXTEND_HELP.to_string(),
        Some("bridge") => BRIDGE_HELP.to_string(),
        Some("analyze") => ANALYZE_HELP.to_string(),
        Some("schedule") => SCHEDULE_HELP.to_string(),
        Some("garden") => GARDEN_HELP.to_string(),
        Some("artifacts") | Some("cas") => ARTIFACTS_HELP.to_string(),
        Some("jobs") => JOBS_HELP.to_string(),
        Some("graph") => GRAPH_HELP.to_string(),
        Some("abc") => ABC_HELP.to_string(),
        Some("soundfont") | Some("sf2") => SOUNDFONT_HELP.to_string(),
        Some("encoding") => ENCODING_HELP.to_string(),
        Some("spaces") => SPACES_HELP.to_string(),
        Some("inference") => INFERENCE_HELP.to_string(),
        Some(other) => format!("Unknown topic: '{}'. Try: sample, project, extend, bridge, analyze, schedule, garden, artifacts, jobs, graph, abc, soundfont, encoding, spaces, inference", other),
    }
}

pub async fn help(request: HelpRequest) -> ToolResult {
    let text = get_help(request.topic.as_deref());
    Ok(ToolOutput::text_only(text))
}

const OVERVIEW: &str = r#"# Holler MCP Tools

## Core Generation
- **sample** - Generate MIDI/audio from spaces (orpheus, musicgen, yue)
- **extend** - Continue existing MIDI content
- **bridge** - Create transitions between sections
- **project** - Convert between formats (MIDI→audio, ABC→MIDI)
- **analyze** - Classify, detect beats, extract embeddings

## Timeline & Playback (garden_*)
- garden_play/pause/stop/seek - Transport controls
- schedule - Place content on timeline
- garden_attach_audio - Connect to PipeWire output

## Storage
- cas_* - Content-addressable storage
- artifact_* - Managed artifacts with metadata
- soundfont_inspect - Browse SoundFont presets

## Jobs
- job_poll - Wait for async operations
- job_status/list/cancel - Job management

## Graph
- graph_context - Get artifacts for LLM context
- graph_query - Trustfall queries on artifacts

Use help(topic: "sample") for detailed tool docs.
"#;

const SAMPLE_HELP: &str = r#"# sample - Generate from model spaces

## Spaces
- orpheus: General MIDI (most versatile)
- orpheus_loops: Loopable patterns
- orpheus_children: Children's music style
- orpheus_mono_melodies: Single-voice melodies
- music_gen: Audio generation (needs prompt)
- yue: Lyrics-to-song (needs prompt)

## Key Parameters
- space: Required. Which model to use
- inference.temperature: 0.0-2.0, higher = more random (default: 1.0)
- inference.max_tokens: Length control (512-2048 typical for MIDI)
- num_variations: Generate multiple at once
- seed: Encoding to condition on (for seeded generation)
- tags: Organize output artifacts

## Examples
```json
{"space": "orpheus", "inference": {"temperature": 1.1}}
{"space": "orpheus_loops", "tags": ["drums", "120bpm"]}
{"space": "music_gen", "prompt": "ambient pad", "inference": {"duration_seconds": 10}}
```

Returns job_id - use job_poll to wait, then graph_context to find artifact.
"#;

const PROJECT_HELP: &str = r#"# project - Convert between formats

## Projections
- MIDI → Audio: Render with SoundFont
- ABC → MIDI: Convert notation to MIDI

## Parameters
- encoding: Source content (see help(topic: "encoding"))
- target: Output format

## Target Types
Audio target:
```json
{"type": "audio", "soundfont_hash": "...", "sample_rate": 44100}
```

MIDI target:
```json
{"type": "midi", "channel": 0, "velocity": 100}
```

## Example
```json
{
  "encoding": {"type": "midi", "artifact_id": "artifact_abc123"},
  "target": {"type": "audio", "soundfont_hash": "e3777f9f8fb3..."}
}
```
"#;

const EXTEND_HELP: &str = r#"# extend - Continue existing content

Uses Orpheus continuation to extend MIDI content.

## Parameters
- encoding: Content to continue from
- space: Optional, inferred from encoding
- inference: Temperature, max_tokens for generation

## Example
```json
{
  "encoding": {"type": "midi", "artifact_id": "artifact_abc123"},
  "inference": {"temperature": 1.0, "max_tokens": 512}
}
```
"#;

const BRIDGE_HELP: &str = r#"# bridge - Create transitions

Generate smooth transitions between MIDI sections.

## Parameters
- from: Starting section (required)
- to: Target section (optional, for future A→B bridging)
- inference: Generation parameters

## Example
```json
{
  "from": {"type": "midi", "artifact_id": "artifact_verse"},
  "to": {"type": "midi", "artifact_id": "artifact_chorus"}
}
```
"#;

const ANALYZE_HELP: &str = r#"# analyze - Content analysis

## Analysis Tasks
- classify: Orpheus MIDI classification
- beats: Detect beats/downbeats, estimate BPM
- embeddings: Extract CLAP audio embeddings
- genre: Classify genre
- mood: Classify mood
- zero_shot: Custom labels classification

## Example
```json
{
  "encoding": {"type": "audio", "artifact_id": "artifact_abc123"},
  "tasks": ["beats", "genre", "mood"]
}
```

For zero-shot:
```json
{
  "tasks": [{"zero_shot": {"labels": ["energetic", "calm", "dark"]}}]
}
```
"#;

const SCHEDULE_HELP: &str = r#"# schedule - Place on timeline

Schedule content for playback via chaosgarden.

## Parameters
- encoding: Content to schedule
- at: Position in beats
- duration: Optional, auto-detected
- gain: 0.0-1.0 volume
- rate: Playback speed multiplier

## Example
```json
{
  "encoding": {"type": "audio", "artifact_id": "artifact_abc123"},
  "at": 0,
  "gain": 0.8
}
```

After scheduling, use garden_play to start playback.
"#;

const GARDEN_HELP: &str = r#"# garden_* - Timeline & playback

## Transport
- garden_play: Start playback
- garden_pause: Pause (resumable)
- garden_stop: Stop and reset
- garden_seek: Jump to beat position
- garden_set_tempo: Set BPM

## Audio Output
- garden_attach_audio: Connect PipeWire output
- garden_detach_audio: Disconnect
- garden_audio_status: Check connection

## Monitor (mic passthrough)
- garden_attach_input: Connect input
- garden_set_monitor: Enable/disable passthrough

## Regions
- garden_get_regions: List scheduled regions
- garden_delete_region: Remove by ID
- garden_move_region: Reposition

## Status
- garden_status: Full state
- garden_query: Trustfall query on state
"#;

const ARTIFACTS_HELP: &str = r#"# Artifacts & CAS

## Artifacts (managed)
- artifact_upload: Upload file with metadata
- artifact_list: List with tag/creator filters
- artifact_get: Get by ID

## CAS (raw storage)
- cas_store: Store base64 content
- cas_upload_file: Upload file path
- cas_inspect: Get info by hash
- cas_stats: Storage statistics

Artifacts wrap CAS hashes with:
- Tags for organization (type:midi, source:orpheus)
- Creator tracking
- Parent/child relationships
- Access counting
"#;

const JOBS_HELP: &str = r#"# Job Management

Async operations return job_id immediately.

## Polling
```json
job_poll({"job_ids": ["uuid1", "uuid2"], "timeout_ms": 60000})
```
- mode: "any" (first complete) or "all" (wait for all)

## Status
- job_status: Check specific job
- job_list: All jobs with optional status filter
- job_cancel: Cancel running job

## Sleep
- job_sleep: Simple delay (max 30s)
"#;

const GRAPH_HELP: &str = r#"# Graph Tools

## Context
graph_context returns recent artifacts for LLM context:
```json
{"tag": "type:midi", "limit": 10, "within_minutes": 30}
```

## Queries
graph_query runs Trustfall queries:
```json
{"query": "{ Artifact(tag: \"type:midi\") { id creator @output } }"}
```

## Annotations
add_annotation attaches notes to artifacts:
```json
{"artifact_id": "...", "message": "Great bass line", "vibe": "groovy"}
```

## Device Graph
- graph_bind: Create identity for device
- graph_tag: Tag identity
- graph_connect: Record connections
- graph_find: Search identities
"#;

const ABC_HELP: &str = r#"# ABC Notation

## Tools
- abc_parse: Parse and extract info
- abc_validate: Check syntax
- abc_transpose: Change key/pitch

## Transpose
```json
{"abc": "X:1\nK:C\nCDEF", "semitones": 2}
```
or by key:
```json
{"abc": "X:1\nK:C\nCDEF", "target_key": "D"}
```

Use project to convert ABC to MIDI.
"#;

const SOUNDFONT_HELP: &str = r#"# SoundFont Tools

## Inspection
soundfont_inspect: List all presets
```json
{"soundfont_hash": "...", "include_drum_map": true}
```

soundfont_preset_inspect: Single preset details
```json
{"soundfont_hash": "...", "bank": 0, "program": 0}
```

## Banks
- Bank 0: Melodic instruments (piano, strings, brass...)
- Bank 128: Percussion/drums

## Common Presets
- 0: Piano
- 33: Fingered Bass
- 48: Strings
- 56: Trumpet
- 73: Flute
"#;

const ENCODING_HELP: &str = r#"# Encoding (Content References)

Encodings specify content for sample, project, extend, schedule, etc.

## Types
MIDI artifact:
```json
{"type": "midi", "artifact_id": "artifact_abc123"}
```

Audio artifact:
```json
{"type": "audio", "artifact_id": "artifact_def456"}
```

ABC notation (inline):
```json
{"type": "abc", "notation": "X:1\nK:C\nCDEF GABc"}
```

Raw CAS hash:
```json
{"type": "hash", "content_hash": "abc123...", "format": "audio/midi"}
```
"#;

const SPACES_HELP: &str = r#"# Generative Spaces

## Orpheus Family (MIDI output)
- orpheus: Base model, versatile general generation
- orpheus_loops: Loopable patterns, good for drums/bass
- orpheus_children: Simpler, children's music style
- orpheus_mono_melodies: Single-voice melodies
- orpheus_bridge: Transition generation

## Audio Generators
- music_gen: Text-to-audio, needs prompt
- yue: Lyrics-to-song, needs prompt with lyrics

## Output Types
- orpheus*: Returns MIDI artifacts
- music_gen/yue: Returns audio artifacts

Use project() to convert MIDI to audio via SoundFont.
"#;

const INFERENCE_HELP: &str = r#"# Inference Parameters

Control generation via inference object:

## Temperature (0.0-2.0)
- 0.0-0.5: Conservative, predictable
- 0.8-1.0: Balanced (default)
- 1.2-1.5: Creative, varied
- 1.5+: Experimental, may be chaotic

## Sampling
- top_p: Nucleus sampling (0.0-1.0, lower = focused)
- top_k: Limit vocabulary (0 = disabled)

## Length
- max_tokens: Token limit (512-2048 for MIDI)
- duration_seconds: For audio spaces

## Reproducibility
- seed: Random seed for reproducible results

## Example
```json
{
  "temperature": 1.1,
  "top_p": 0.9,
  "max_tokens": 1024,
  "seed": 42
}
```
"#;
