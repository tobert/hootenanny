# 04: Rendering

**File:** `src/rendering.rs`
**Focus:** Graph execution, buffer management, hot path
**Dependencies:** `primitives`, `graph`

---

## Task

Create `crates/flayer/src/rendering.rs` with CompiledGraph and RenderEngine. The render loop must be allocation-free.

**Why this first?** This is where sound actually happens. Graph and primitives are useless without rendering. Offline rendering validates the whole stack before adding realtime complexity.

**Deliverables:**
1. `rendering.rs` with CompiledGraph::compile(), RenderEngine::process()
2. `render_to_file()` function producing WAV output
3. Tests: compile a simple graph, render to buffer, verify output

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ PipeWire callbacks — task 05
- ❌ Realtime session management — task 05
- ❌ Trustfall exposure — task 06

Focus ONLY on graph compilation, buffer management, and offline rendering.

**Critical:** Verify no allocations in process() hot path.

---

## Core Invariant

**The render loop never allocates.** All buffers pre-allocated during compilation.

---

## Types

```rust
pub struct CompiledGraph {
    nodes: Vec<BoxedNode>,
    buffers: Vec<SignalBuffer>,          // pre-allocated output buffers
    buffer_map: Vec<BufferSlot>,         // (node, port) → buffer index
    routes: Vec<Route>,                  // edge routing table
    failed_nodes: HashSet<usize>,        // skip these
    latency: Vec<LatencyState>,          // per-node latency (atomic)
    delay_lines: Vec<DelayLine>,         // pre-allocated PDC buffers
}

#[derive(Clone, Copy)]
struct BufferSlot {
    buffer_idx: usize,
    signal_type: SignalType,
}

#[derive(Clone, Copy)]
struct Route {
    src_buffer: usize,
    dest_node: usize,
    dest_port: usize,
    gain: f32,
}

pub struct LatencyState {
    pub latency_samples: AtomicU64,      // node's own latency
    pub compensation_samples: AtomicU64, // delay to apply
}

pub struct DelayLine {
    buffer: Vec<f32>,
    write_pos: usize,
    read_pos: usize,
    channels: u8,
}

pub struct RenderEngine {
    sample_rate: u32,
    buffer_size: usize,
    tempo_map: Arc<TempoMap>,
    input_gather: Vec<Vec<(usize, f32)>>, // reused each frame
    output: AudioBuffer,
    master_buffer_idx: Option<usize>,
}

pub struct InputView<'a> {
    gather: &'a [Vec<(usize, f32)>],
    buffers: &'a [SignalBuffer],
}
```

---

## CompiledGraph Methods

- `compile(graph: &Graph, buffer_size: usize) -> Result<Self>`
- `recalculate_compensation(&self)` — background thread, atomic stores
- `update_node_latency(&self, node_idx: usize, latency_samples: u64)`

**Compilation allocates everything upfront:**
1. Collect nodes in topological order
2. Pre-allocate output buffer for each node output port
3. Build routing table from edges
4. Initialize latency state and delay lines

---

## RenderEngine Methods

- `new(sample_rate, buffer_size, tempo_map) -> Self`
- `prepare(&mut self, graph: &CompiledGraph)` — setup input_gather size
- `process(&mut self, graph: &mut CompiledGraph, ctx: &mut ProcessContext) -> Result<&AudioBuffer>`

**Process loop (hot path, no allocation):**
```
for each node in topological order:
    if failed: skip
    clear input_gather
    populate input_gather from routes
    build InputView (borrows)
    clear output buffers
    node.process(ctx, inputs, outputs)
    handle ProcessError (Skipped → silence, Failed → mark failed)
    apply latency compensation via delay_line
copy master output
ctx.advance()
```

---

## InputView Methods

- `audio(&self, port_idx: usize, out: &mut AudioBuffer)` — mix all sources
- `midi(&self, port_idx: usize, out: &mut MidiBuffer)` — merge all sources
- `is_connected(&self, port_idx: usize) -> bool`

---

## Signal Merge Semantics

| Signal | Merge |
|--------|-------|
| Audio | Sum with per-edge gain |
| MIDI | Event union, sorted by frame |
| Control | Average |
| Trigger | Union, sorted by frame |

---

## Offline Rendering

```rust
pub fn render_to_file(
    graph: &Graph,
    tempo_map: &TempoMap,
    duration_beats: Beat,
    sample_rate: u32,
    buffer_size: usize,
    path: impl AsRef<Path>,
) -> Result<()>
```

Uses `hound::WavWriter`. ProcessContext mode is `Offline`.

---

## Realtime Session (sketch)

```rust
pub struct RealtimeSession {
    engine: RenderEngine,
    ctx: ProcessContext,
    compiled: CompiledGraph,
    graph: Graph,
    needs_recompile: AtomicBool,
}
```

Methods: `process()`, `modify_graph(f)`, `play()`, `stop()`, `seek(beat)`

Graph modifications trigger recompile on background thread, swap atomically.

---

## Acceptance Criteria

- [ ] `CompiledGraph::compile` builds routing table
- [ ] `process()` calls nodes in topological order
- [ ] No allocation in `process()` — verify with custom allocator or profiler
- [ ] `ProcessError::Skipped` outputs silence, continues
- [ ] `ProcessError::Failed` marks node failed, skips in future
- [ ] `render_to_file` produces valid WAV
