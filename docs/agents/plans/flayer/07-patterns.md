# 07: Patterns

**File:** `src/patterns.rs`
**Focus:** Convenient DAW-like compositions from primitives
**Dependencies:** `primitives`, `graph`

---

## Task

Create `crates/flayer/src/patterns.rs` with Track, Bus, Section, Timeline, and Project types. Implement `Timeline::build_graph()` to assemble a Graph from the pattern structure.

**Why this first?** Patterns are ergonomics — familiar DAW concepts built on primitives. Once the core works, patterns make it usable. Timeline→Graph is the bridge from user intent to processing.

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

## Philosophy

Patterns are **not primitives**. They're ergonomic wrappers that:
- Organize primitives in familiar ways
- Handle common wiring automatically
- Can be ignored when primitives suffice

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

## Track Methods

- `new(name) -> Self`
- `add_region(region)`
- `route_to_bus(bus_id)`
- `add_send(bus_id, amount)`
- `regions_at(beat) -> impl Iterator<Item = &Region>`
- `end() -> Beat`

## Section Methods

- `new(name, start, end) -> Self`
- `with_mood(mood) -> Self`
- `with_energy(energy) -> Self`
- `with_density(density) -> Self`
- `duration() -> Beat`
- `contains(beat) -> bool`

## Timeline Methods

- `new(name, bpm) -> Self`
- `add_section(name, start, end) -> &mut Section`
- `add_track(name) -> &mut Track`
- `add_bus(name) -> &mut Bus`
- `section_at(beat) -> Option<&Section>`
- `all_regions() -> impl Iterator<Item = &Region>`
- `duration() -> Beat`
- `build_graph() -> Graph`

## Project Methods

- `new(name, bpm) -> Self`
- `save(path) -> Result<()>`
- `load(path) -> Result<Self>`
- `touch()` — update modified_at

---

## build_graph() Logic

1. Create master output node
2. Create mixer node per bus
3. Create mixer node per track
4. Connect tracks to their output (master or bus)
5. Connect buses to their output (master or parent bus)
6. Return assembled Graph

---

## Acceptance Criteria

- [ ] Track/Bus/Section serialize via serde_json
- [ ] `Timeline::build_graph()` produces valid Graph
- [ ] Routing respects TrackOutput and BusOutput
- [ ] `Project::save/load` round-trips correctly
- [ ] SectionHints available for AI generation context
