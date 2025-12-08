# Flayer: Compute-Graph DAW Engine

**Status:** Approved for Implementation
**Date:** December 8, 2025
**Authors:** Claude + Gemini + Human

---

## Vision

Flayer is a headless DAW engine designed for AI-first music composition. Unlike traditional DAWs where humans draw automation curves and place clips with a mouse, flayer is built for agents to compose programmatically.

**Core innovations:**
1. **Latent Regions** - Generative placeholders resolved by AI models
2. **Structure-Aware Sections** - Arrangement sections with mood/energy hints that guide generation
3. **Queryable Graph** - Trustfall integration lets agents reason about signal flow
4. **Two-Layer Audio Graph** - Internal (dasp_graph) + External (PipeWire) routing

## Design Principles

### 1. Automation Over Effects

Traditional trackers have per-note effects (arpeggio, vibrato, pitch slide). We rejected this in favor of **automation lanes**—continuous parameter curves that modulate anything over time.

Why: One concept (automation) replaces many. Fade in/out, volume swells, filter sweeps, panning motion—all automation.

### 2. Sections Over Markers

Sections aren't just labels—they're **generation contexts**. A latent in the "chorus" section inherits `mood: "euphoric"`, `energy: 0.9`. The AI knows what kind of music to generate without explicit prompting.

### 3. Explicit Routing

Tracks route to buses or master. Buses can chain. Sends enable parallel effects (reverb, delay). This is standard DAW architecture, done simply.

### 4. Immutable Assets, Mutable Arrangement

Audio/MIDI content lives in CAS (content-addressed storage)—immutable, deduplicated. The arrangement (clips, automation, routing) is mutable and serializable.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                           Project                                │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                        Timeline                            │  │
│  │                                                            │  │
│  │  Sections:  [intro]──[verse]──[chorus]──[verse]──[outro]  │  │
│  │              mood:    mood:    mood:     mood:    mood:    │  │
│  │              calm     building euphoric  intimate fading   │  │
│  │                                                            │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │ Track: "Drums"                        → Bus: "Drums" │  │  │
│  │  │ [clip]    [latent]    [clip]    [latent]            │  │  │
│  │  │ ~~~~automation: volume~~~~                          │  │  │
│  │  └─────────────────────────────────────────────────────┘  │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │ Track: "Bass"                         → Master       │  │  │
│  │  │     [latent──────────────────────]                  │  │  │
│  │  └─────────────────────────────────────────────────────┘  │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │ Bus: "Drums"                          → Master       │  │  │
│  │  │ ~~~~automation: compression ratio~~~~               │  │  │
│  │  └─────────────────────────────────────────────────────┘  │  │
│  │  ┌─────────────────────────────────────────────────────┐  │  │
│  │  │ Master                                → PipeWire     │  │  │
│  │  └─────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Data Model Summary

```
Project
└── Timeline
    ├── sections: [Section]        # Arrangement structure with AI hints
    ├── tracks: [Track]            # Audio/MIDI lanes
    │   ├── clips: [Clip]          # Concrete audio/MIDI
    │   ├── latents: [Latent]      # Generative placeholders
    │   ├── automation: [Lane]     # Parameter curves
    │   ├── output: Master | Bus   # Routing destination
    │   └── sends: [Send]          # Aux sends
    ├── buses: [Bus]               # Submix groups
    │   ├── automation: [Lane]
    │   └── output: Master | Bus
    └── master: MasterBus          # Final output
```

## What We Removed (And Why)

| Removed | Reason |
|---------|--------|
| Per-clip `Effect` enum | Automation is more general. VolumeRamp → volume automation. PitchSlide → pitch automation. |
| Per-clip `fade_in/out` | Automation handles fades. Crossfades handle overlaps. |
| `row_resolution` | Tracker-specific. Not needed for AI-first workflow. |
| `Embed` (nested timelines) | Complexity. Deferred to v2. |

## What We Added

| Added | Purpose |
|-------|---------|
| `Section` | AI-native arrangement structure. Mood, energy, contrast hints. |
| `AutomationLane` | Universal parameter modulation over time. |
| `Bus` + `Send` | Proper submix routing and parallel effects. |
| `MasterBus` | Explicit master output with its own automation. |
| `Project` | Serialization wrapper for save/load. |
| `Crossfade` | Automatic or manual crossfades between overlapping clips. |

## File Structure

```
crates/flayer/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public exports
    ├── project.rs          # Project, serialization
    ├── timeline.rs         # Timeline, Section
    ├── track.rs            # Track, Clip, Latent
    ├── routing.rs          # Bus, Send, MasterBus, OutputTarget
    ├── automation.rs       # AutomationLane, AutomationPoint, Curve
    ├── midi.rs             # Sequence, TempoMap, transformations
    ├── render.rs           # Rendering engine
    ├── resolve.rs          # Latent resolution with quality filtering
    └── graph/
        ├── mod.rs
        ├── internal.rs     # dasp_graph render graph
        ├── external.rs     # PipeWire integration
        └── adapter.rs      # Unified Trustfall adapter
```

## Implementation Tasks

1. [Core Structs](./01-core-structs.md) - Timeline, Track, Clip, Latent, Section, Automation
2. [MIDI Module](./02-midi-module.md) - Sequence, TempoMap, transformations
3. [Routing](./03-routing.md) - Bus, Send, MasterBus, signal flow
4. [Renderer](./04-renderer.md) - Audio mixing, MIDI synthesis, automation
5. [Latent Resolution](./05-latent-resolution.md) - MCP integration, quality filtering
6. [Audio Graph](./06-audio-graph.md) - dasp_graph + PipeWire + Trustfall
7. [Project Serialization](./07-project.md) - Save/load, versioning

## Dependencies

```toml
[dependencies]
uuid = { version = "1", features = ["v4", "serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
midly = "0.5"
hound = "3.5"
rustysynth = "1.3"
rubato = "0.15"
dasp_graph = { version = "0.11", features = ["node-boxed"] }
petgraph = "0.6"
trustfall = "0.8"
tracing = "0.1"
```

## Example Usage (Lua API)

```lua
local flayer = require("flayer")

-- Create project
local project = flayer.Project.new("my_song", 120)  -- 120 BPM
local tl = project.timeline

-- Define structure
tl:add_section("intro", 0, 8, { mood = "calm", energy = 0.3 })
tl:add_section("verse", 8, 24, { mood = "building", energy = 0.5 })
tl:add_section("chorus", 24, 40, { mood = "euphoric", energy = 0.9 })

-- Create routing
local drum_bus = tl:add_bus("Drums")
local drums = tl:add_track("Kick", { output = drum_bus })
local hats = tl:add_track("Hats", { output = drum_bus })
local bass = tl:add_track("Bass")  -- defaults to master

-- Add content
drums:add_latent({
    at = 0,
    duration = 40,
    model = "orpheus_generate",
    params = { instrument = "drums", density = 0.7 }
    -- Inherits mood/energy from sections automatically
})

bass:add_latent({
    at = 0,
    duration = 40,
    model = "orpheus_generate",
    mode = "infill",  -- AI fills gaps based on context
})

-- Add automation
drums:automate("volume", {
    { beat = 0, value = 0.0, curve = "linear" },
    { beat = 4, value = 1.0 },  -- fade in over 4 beats
})

drum_bus:automate("compression_ratio", {
    { beat = 24, value = 2.0 },  -- light compression in verse
    { beat = 24, value = 6.0 },  -- heavy compression in chorus
})

-- Resolve and render
tl:resolve_latents()
tl:render("output.wav")

-- Save project
project:save("my_song.flayer")
```

## Querying the Graph (Trustfall)

```graphql
# Find all sources in the chorus section
query {
    Section(name: "chorus") {
        start_beat @output
        end_beat @output
        tracks_active {
            name @output
            sources_in_range {
                hash @output
                signal_path {
                    ... on Bus { name @output(name: "via_bus") }
                    ... on Master { pipewire_node { name @output(name: "hw_output") } }
                }
            }
        }
    }
}
```
