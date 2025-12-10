# 05: External I/O

**File:** `src/external_io.rs`
**Focus:** PipeWire integration for hardware audio/MIDI
**Dependencies:** `pipewire` (feature-gated)

---

## Task

Create `crates/flayer/src/external_io.rs` with ExternalIOManager and I/O node types. Feature-gate behind `pipewire`.

**Why this first?** Once rendering works offline, external I/O makes it real. This bridges the graph to actual speakers and MIDI devices. Feature-gated so flayer works without PipeWire for CI/offline use.

**Deliverables:**
1. `external_io.rs` with types and ExternalIOManager skeleton
2. Compiles with `--features pipewire` and without
3. ExternalInputNode/ExternalOutputNode implement Node trait

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo check --features pipewire
cargo test
```

## Out of Scope

- ❌ JACK support — PipeWire only for now
- ❌ MIDI device enumeration — use hootenanny's audio-graph
- ❌ Trustfall queries on I/O — task 06

Focus ONLY on PipeWire stream setup and I/O node types.

**Note:** Full integration requires runtime testing with actual hardware.

---

## pipewire-rs Pattern

```rust
use pipewire as pw;
use pw::stream::{Stream, StreamFlags};
use pw::spa::utils::Direction;

// Create stream for output
let stream = Stream::new(&core, "flayer-out", properties)?;

stream.connect(
    Direction::Output,
    None,
    StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
    &mut params,
)?;

// In process callback (real-time safe):
// - buffer is memory-mapped, no allocation
// - write samples directly
```

---

## Types

```rust
#[derive(Debug, Clone)]
pub struct PipeWireOutput {
    pub id: Uuid,
    pub pw_node_id: Option<u32>,
    pub name: String,
    pub channels: u8,
}

#[derive(Debug, Clone)]
pub struct PipeWireInput {
    pub id: Uuid,
    pub pw_node_id: Option<u32>,
    pub name: String,
    pub port_pattern: Option<String>,
    pub channels: u8,
}

#[derive(Debug, Clone)]
pub struct MidiDevice {
    pub id: Uuid,
    pub name: String,
    pub direction: MidiDirection,
    pub pw_node_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiDirection {
    Input,
    Output,
}

pub struct ExternalIOManager {
    outputs: HashMap<Uuid, PipeWireOutput>,
    inputs: HashMap<Uuid, PipeWireInput>,
    midi_devices: HashMap<Uuid, MidiDevice>,
    sample_rate: u32,
    buffer_size: usize,
    // pw_context, pw_core would go here
}
```

---

## ExternalIOManager Methods

- `new(sample_rate, buffer_size) -> Result<Self>`
- `create_output(name, channels) -> Result<Uuid>`
- `create_input(name, channels) -> Result<Uuid>`
- `connect_input(id, port_pattern) -> Result<()>`
- `register_midi(name, direction) -> Result<Uuid>`
- `sample_rate(&self) -> u32`
- `buffer_size(&self) -> usize`
- `outputs(&self) -> impl Iterator<Item = &PipeWireOutput>`
- `inputs(&self) -> impl Iterator<Item = &PipeWireInput>`

---

## I/O Nodes

Nodes that bridge PipeWire callbacks to the graph:

```rust
pub struct ExternalInputNode {
    descriptor: NodeDescriptor,
    // ring buffer from PipeWire callback → graph
}

pub struct ExternalOutputNode {
    descriptor: NodeDescriptor,
    // ring buffer from graph → PipeWire callback
}

pub struct MidiInputNode {
    descriptor: NodeDescriptor,
    event_queue: Arc<Mutex<Vec<(usize, MidiMessage)>>>,
}
```

**Pattern:**
- Input nodes: no graph inputs, produce SignalBuffer from external source
- Output nodes: consume SignalBuffer, no graph outputs
- Ring buffers bridge callback ↔ graph (lock-free)

---

## Feature Gate

```toml
[features]
pipewire = ["dep:pipewire"]
```

External I/O only available when `pipewire` feature enabled.
Without it, flayer works for offline rendering only.

---

## Acceptance Criteria

- [ ] Feature gate compiles with and without `pipewire`
- [ ] `ExternalIOManager::create_output` registers with PipeWire
- [ ] Audio flows from graph to speakers via output node
- [ ] Audio flows from mic to graph via input node
- [ ] MIDI events captured and available in graph
