# 07: Patterns

**File:** `src/patterns.rs`
**Focus:** Convenient compositions from primitives — one interface among many
**Dependencies:** `primitives`, `graph`

---

## Task

Create `crates/chaosgarden/src/patterns.rs` with Track, Bus, Section, Timeline, and Project types. Implement `Timeline::build_graph()` to assemble a Graph from the pattern structure.

**Why this matters:** Patterns are ergonomics for humans familiar with DAW concepts. They're one way to organize a performance—not *the* way. Agents might use patterns, or work directly with regions and graphs.

**Philosophy note:** Patterns are not primitives. They don't appear in the philosophy section or architecture diagram. They're a convenience layer for participants who think in tracks and sections. Other interfaces (direct graph manipulation, voice commands, generative agents) are equally valid.

**Deliverables:**
1. `patterns.rs` with all types and methods
2. `build_graph()` producing valid Graph with correct routing
3. `Project::save/load` round-tripping via serde_json
4. Tests: create timeline with tracks/buses, build graph, verify connections

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ Mixer/gain DSP nodes — use placeholders
- ❌ Plugin hosting — future work
- ❌ Undo/redo — future work

Focus ONLY on Track, Bus, Section, Timeline, Project types and graph assembly.

---

## Patterns Are Optional

The performance space doesn't require patterns. Consider:

| Approach | Who Uses It | How They Work |
|----------|-------------|---------------|
| **Patterns** | Humans from DAW backgrounds | Tracks, buses, sections → `build_graph()` |
| **Direct graph** | Agents, procedural composers | Create nodes, connect edges directly |
| **Generative** | AI models | Produce regions with latent behaviors |
| **Hybrid** | Everyone | Mix approaches as needed |

Patterns exist because they're familiar and useful, not because they're fundamental.

---

## Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: Uuid,
    pub name: String,
    pub regions: Vec<Region>,
    pub mixer_node_id: Uuid,
    pub output: TrackOutput,
    pub sends: Vec<Send>,
    pub volume: f64,
    pub pan: f64,
    pub mute: bool,
    pub solo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrackOutput {
    Master,
    Bus(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Send {
    pub bus_id: Uuid,
    pub amount: f64,
    pub pre_fader: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bus {
    pub id: Uuid,
    pub name: String,
    pub mixer_node_id: Uuid,
    pub output: BusOutput,
    pub volume: f64,
    pub pan: f64,
    pub mute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BusOutput {
    Master,
    Bus(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: Uuid,
    pub name: String,
    pub start: Beat,
    pub end: Beat,
    pub hints: SectionHints,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectionHints {
    pub mood: Option<String>,      // "calm", "tense", "euphoric"
    pub energy: Option<f64>,       // 0.0 (ambient) to 1.0 (peak)
    pub density: Option<f64>,      // 0.0 (sparse) to 1.0 (dense)
    pub contrast_with: Option<Uuid>,
    pub style_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub id: Uuid,
    pub name: String,
    pub tempo_map: TempoMap,
    pub sections: Vec<Section>,
    pub tracks: Vec<Track>,
    pub buses: Vec<Bus>,
    pub master_node_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub created_at: String,
    pub modified_at: String,
    pub timeline: Timeline,
    pub metadata: serde_json::Value,
}
```

---

## SectionHints: Context for Generation

Sections aren't just organizational—they carry semantic hints for generation:

```rust
let chorus = Section::new("Chorus", Beat(32.0), Beat(64.0))
    .with_mood("euphoric")
    .with_energy(0.9)
    .with_density(0.7);
```

When an agent generates content for this section, hints inform the generation:
- "This is the chorus, make it energetic"
- "Density is high, layer multiple instruments"
- "Mood is euphoric, use major key and uplifting progressions"

Hints are suggestions, not constraints. Models interpret them creatively.

---

## Track Methods

```rust
impl Track {
    pub fn new(name: &str) -> Self;
    pub fn add_region(&mut self, region: Region);
    pub fn route_to_bus(&mut self, bus_id: Uuid);
    pub fn add_send(&mut self, bus_id: Uuid, amount: f64);
    pub fn regions_at(&self, beat: Beat) -> impl Iterator<Item = &Region>;
    pub fn end(&self) -> Beat;

    // Convenience for adding latent regions
    pub fn add_latent(
        &mut self,
        position: Beat,
        duration: Beat,
        tool: &str,
        params: serde_json::Value,
    ) -> Uuid;
}
```

## Section Methods

```rust
impl Section {
    pub fn new(name: &str, start: Beat, end: Beat) -> Self;
    pub fn with_mood(self, mood: &str) -> Self;
    pub fn with_energy(self, energy: f64) -> Self;
    pub fn with_density(self, density: f64) -> Self;
    pub fn with_contrast(self, other: Uuid) -> Self;
    pub fn duration(&self) -> Beat;
    pub fn contains(&self, beat: Beat) -> bool;
}
```

## Timeline Methods

```rust
impl Timeline {
    pub fn new(name: &str, bpm: f64) -> Self;
    pub fn add_section(&mut self, name: &str, start: Beat, end: Beat) -> &mut Section;
    pub fn add_track(&mut self, name: &str) -> &mut Track;
    pub fn add_bus(&mut self, name: &str) -> &mut Bus;
    pub fn section_at(&self, beat: Beat) -> Option<&Section>;
    pub fn all_regions(&self) -> impl Iterator<Item = &Region>;
    pub fn duration(&self) -> Beat;

    /// Build a Graph from the timeline structure
    pub fn build_graph(&self) -> Graph;

    /// Get hints for a beat position (from enclosing section)
    pub fn hints_at(&self, beat: Beat) -> Option<&SectionHints>;
}
```

## Project Methods

```rust
impl Project {
    pub fn new(name: &str, bpm: f64) -> Self;
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()>;
    pub fn load(path: impl AsRef<Path>) -> Result<Self>;
    pub fn touch(&mut self);  // update modified_at
}
```

---

## build_graph() Logic

Transforms pattern structure into executable Graph:

1. Create master output node
2. Create mixer node per bus
3. Create mixer node per track
4. Connect tracks to their output (master or bus)
5. Connect buses to their output (master or parent bus)
6. Wire sends (track → bus with gain)
7. Return assembled Graph

```rust
pub fn build_graph(&self) -> Graph {
    let mut graph = Graph::new();

    // 1. Master output
    let master = graph.add_node(MasterOutputNode::new(self.master_node_id));

    // 2. Buses (in dependency order)
    for bus in &self.buses {
        let node = graph.add_node(MixerNode::new(bus.mixer_node_id, &bus.name));
        match bus.output {
            BusOutput::Master => graph.connect(node, "out", master, "in"),
            BusOutput::Bus(parent) => {
                let parent_idx = graph.index_of(parent).unwrap();
                graph.connect(node, "out", parent_idx, "in");
            }
        }
    }

    // 3. Tracks
    for track in &self.tracks {
        let node = graph.add_node(MixerNode::new(track.mixer_node_id, &track.name));
        match track.output {
            TrackOutput::Master => graph.connect(node, "out", master, "in"),
            TrackOutput::Bus(bus_id) => {
                let bus_idx = graph.index_of(bus_id).unwrap();
                graph.connect(node, "out", bus_idx, "in");
            }
        }

        // Wire sends
        for send in &track.sends {
            let bus_idx = graph.index_of(send.bus_id).unwrap();
            graph.connect_with_gain(node, "send", bus_idx, "in", send.amount);
        }
    }

    graph
}
```

---

## Using Patterns with Latents

Patterns work naturally with the latent lifecycle:

```rust
// Create a track with latent content
let mut drums = timeline.add_track("Drums");
drums.add_latent(
    Beat(0.0),
    Beat(16.0),
    "orpheus_generate",
    json!({"prompt": "funk drums 120bpm", "temperature": 0.8}),
);

// Section hints inform generation
let verse = timeline.add_section("Verse", Beat(0.0), Beat(32.0))
    .with_mood("groovy")
    .with_energy(0.6);

// When agent generates for the verse, it can query hints:
let hints = timeline.hints_at(Beat(8.0));  // Returns verse hints
```

---

## Patterns and Trustfall

Patterns can be exposed through Trustfall for agents that prefer working at that level:

```graphql
# Future: Query by section
{
    Section(name: "Chorus") {
        start @output
        end @output
        hints {
            mood @output
            energy @output
        }
        regions {
            name @output
            is_playable @output
        }
    }
}
```

This is optional—agents can also query regions directly without knowing about sections.

---

## Acceptance Criteria

- [ ] Track/Bus/Section serialize via serde_json
- [ ] `Timeline::build_graph()` produces valid Graph
- [ ] Routing respects TrackOutput and BusOutput
- [ ] Sends wire correctly with gain
- [ ] `Project::save/load` round-trips correctly
- [ ] SectionHints available via `hints_at()`
- [ ] `add_latent()` convenience creates properly-formed latent regions
