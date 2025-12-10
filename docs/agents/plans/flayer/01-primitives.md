# 01: Primitives

**File:** `src/primitives.rs`
**Focus:** Time, Signal, Node, Region — types only
**Dependencies:** `uuid`, `serde`

---

## Task

Create `crates/flayer/src/primitives.rs` with the types defined below. Implement the methods listed for each type. Write tests for TempoMap time conversions.

**Why this first?** Everything else depends on these types. Graph nodes process Signals. Regions use Beat positions. Rendering needs ProcessContext. Get these right and the rest follows.

**Deliverables:**
1. `primitives.rs` with all types compiling
2. TempoMap methods working with tempo changes
3. Tests proving tick↔beat↔second↔sample round-trips

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ Graph topology — task 02
- ❌ MCP resolution — task 03
- ❌ Buffer management — task 04
- ❌ PipeWire I/O — task 05

Focus ONLY on type definitions and time conversions.

---

## Time Types

Musical time representation with tempo-aware conversion.

```rust
pub const DEFAULT_PPQ: u16 = 960;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Tick(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Beat(pub f64);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Second(pub f64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Sample(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TimeSignature {
    pub numerator: u8,    // default 4
    pub denominator: u8,  // default 4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoChange {
    pub tick: Tick,
    pub bpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSignatureChange {
    pub tick: Tick,
    pub time_sig: TimeSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoMap {
    pub ppq: u16,
    pub tempo_changes: Vec<TempoChange>,      // sorted by tick
    pub time_sig_changes: Vec<TimeSignatureChange>, // sorted by tick
}
```

**TempoMap methods to implement:**
- `new(bpm, time_sig) -> Self`
- `tempo_at(tick) -> f64`
- `time_sig_at(tick) -> TimeSignature`
- `tick_to_beat`, `beat_to_tick`
- `tick_to_second`, `second_to_tick` (accounts for tempo changes)
- `tick_to_sample`, `sample_to_tick` (given sample_rate)

**Implement `Add`, `Sub` for** Tick, Beat, Second, Sample.

---

## Signal Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalType {
    Audio,
    Midi,
    Control,
    Trigger,
}

#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,  // interleaved L/R
    pub channels: u8,
}

#[derive(Debug, Clone, Default)]
pub struct MidiBuffer {
    pub events: Vec<MidiEvent>,  // sorted by frame
}

#[derive(Debug, Clone)]
pub struct MidiEvent {
    pub frame: usize,
    pub message: MidiMessage,
}

#[derive(Debug, Clone)]
pub enum MidiMessage {
    NoteOn { channel: u8, pitch: u8, velocity: u8 },
    NoteOff { channel: u8, pitch: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: i16 },
}

#[derive(Debug, Clone)]
pub struct ControlBuffer {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Default)]
pub struct TriggerBuffer {
    pub triggers: Vec<Trigger>,
}

#[derive(Debug, Clone)]
pub struct Trigger {
    pub frame: usize,
    pub kind: TriggerKind,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerKind {
    SectionStart,
    SectionEnd,
    BarStart,
    BeatStart,
    Cue(String),
    Custom(String),
}

#[derive(Debug, Clone)]
pub enum SignalBuffer {
    Audio(AudioBuffer),
    Midi(MidiBuffer),
    Control(ControlBuffer),
    Trigger(TriggerBuffer),
}
```

**Buffer methods to implement:**
- `AudioBuffer::new(frames, channels)`, `mix(&mut self, other, gain)`, `clear()`
- `MidiBuffer::new()`, `merge(&mut self, other)`, `clear()`
- `ControlBuffer::constant(value)`, `at(t) -> f64` (linear interpolation)

---

## Node Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub signal_type: SignalType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDescriptor {
    pub id: Uuid,
    pub name: String,
    pub type_id: String,  // e.g., "source.audio", "effect.gain", "ai.orpheus"
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub latency_samples: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMode {
    Offline,
    Realtime { deadline_ns: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransportState {
    #[default]
    Stopped,
    Playing,
    Recording,
}

#[derive(Clone)]
pub struct ProcessContext {
    pub sample_rate: u32,
    pub buffer_size: usize,
    pub position_samples: Sample,
    pub position_beats: Beat,
    pub tempo_map: Arc<TempoMap>,
    pub mode: ProcessingMode,
    pub transport: TransportState,
}

#[derive(Debug, Clone)]
pub enum ProcessError {
    Skipped { reason: &'static str },
    Failed { reason: String },
}

pub trait Node: Send + Sync {
    fn descriptor(&self) -> &NodeDescriptor;
    fn process(
        &mut self,
        ctx: &ProcessContext,
        inputs: &[SignalBuffer],
        outputs: &mut [SignalBuffer],
    ) -> Result<(), ProcessError>;
    fn reset(&mut self) {}
    fn shutdown(&mut self) {}
}

pub type BoxedNode = Box<dyn Node>;
```

---

## Region Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    Audio,
    Midi,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaybackParams {
    pub gain: f64,      // 1.0 = unity
    pub rate: f64,      // 1.0 = normal
    pub offset: Beat,
    pub reverse: bool,
    pub fade_in: Beat,
    pub fade_out: Beat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CurveType {
    #[default]
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurvePoint {
    pub position: f64,  // 0.0 = start, 1.0 = end
    pub value: f64,
    pub curve: CurveType,
}

#[derive(Debug, Clone)]
pub struct ResolvedContent {
    pub content_hash: String,
    pub content_type: ContentType,
    pub artifact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Behavior {
    PlayContent {
        content_hash: String,
        content_type: ContentType,
        params: PlaybackParams,
    },
    GenerateContent {
        tool: String,
        params: serde_json::Value,
        #[serde(skip)]
        resolved: Option<ResolvedContent>,
    },
    ApplyProcessing {
        target_node: Uuid,
        parameter: String,
        curve: Vec<CurvePoint>,
    },
    EmitTrigger {
        kind: TriggerKind,
        data: Option<serde_json::Value>,
    },
    Custom {
        behavior_type: String,
        config: serde_json::Value,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegionMetadata {
    pub name: Option<String>,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub id: Uuid,
    pub position: Beat,
    pub duration: Beat,
    pub behavior: Behavior,
    pub metadata: RegionMetadata,
}
```

**Region methods to implement:**
- `play_audio(position, duration, content_hash) -> Self`
- `play_midi(position, duration, content_hash) -> Self`
- `generate(position, duration, tool, params) -> Self`
- `trigger(position, kind) -> Self`
- `end(&self) -> Beat`
- `contains(&self, beat) -> bool`
- `overlaps(&self, other) -> bool`
- `is_resolved(&self) -> bool`
- Builder methods: `with_name()`, `with_tag()`

---

## Acceptance Criteria

- [ ] All types compile with derives as shown
- [ ] TempoMap converts tick↔beat↔second↔sample accurately with tempo changes
- [ ] `cargo test` passes for time conversion round-trips
- [ ] Node trait is object-safe (`Box<dyn Node>` works)
- [ ] Region serializes/deserializes via serde_json
