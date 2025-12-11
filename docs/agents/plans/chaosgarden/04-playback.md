# 04: Playback Engine

**File:** `src/playback.rs`
**Focus:** Realtime graph execution, buffer management, mix-in handling
**Dependencies:** `primitives`, `graph`, `latent`

---

## Task

Create `crates/chaosgarden/src/playback.rs` with PlaybackEngine that executes the realtime graph. The render loop must be allocation-free.

**Why this matters:** This is where sound happens. The playback engine consumes resolved regions, handles mix-in schedules, and produces audio in real time.

**Key distinction:** Playback is separate from generation. The engine only deals with resolved, approved content. Latent regions are visible but silent—handled by LatentManager.

**Deliverables:**
1. `playback.rs` with CompiledGraph, PlaybackEngine
2. Mix-in schedule consumption
3. `render_to_file()` for offline export
4. Tests: compile graph, render buffer, verify output

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ Latent job dispatch — task 03
- ❌ PipeWire callbacks — task 05
- ❌ Trustfall exposure — task 06

Focus ONLY on graph compilation, buffer management, and the playback loop.

**Critical:** Verify no allocations in `process()` hot path.

---

## Core Invariant

**The render loop never allocates.** All buffers pre-allocated during compilation.

---

## Types

```rust
/// Pre-compiled graph ready for realtime execution
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

/// The realtime playback engine
pub struct PlaybackEngine {
    sample_rate: u32,
    buffer_size: usize,
    tempo_map: Arc<TempoMap>,
    position: PlaybackPosition,
    transport: TransportState,

    // Pre-allocated work buffers (never reallocated)
    input_gather: Vec<Vec<(usize, f32)>>,
    output: AudioBuffer,
    master_buffer_idx: Option<usize>,

    // Mix-in handling
    mix_in_queue: VecDeque<MixInSchedule>,
    active_crossfades: Vec<ActiveCrossfade>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlaybackPosition {
    pub samples: Sample,
    pub beats: Beat,
}

/// Tracks an in-progress crossfade
struct ActiveCrossfade {
    old_region_id: Uuid,
    new_region_id: Uuid,
    start_beat: Beat,
    end_beat: Beat,
    progress: f32,
}

pub struct InputView<'a> {
    gather: &'a [Vec<(usize, f32)>],
    buffers: &'a [SignalBuffer],
}
```

---

## CompiledGraph Methods

```rust
impl CompiledGraph {
    /// Compile graph for realtime execution
    pub fn compile(graph: &Graph, buffer_size: usize) -> Result<Self>;

    /// Recalculate latency compensation (background thread)
    pub fn recalculate_compensation(&self);

    /// Update a node's reported latency
    pub fn update_node_latency(&self, node_idx: usize, latency_samples: u64);

    /// Mark node as failed (skip in future)
    pub fn mark_failed(&mut self, node_idx: usize);

    /// Get processing order
    pub fn processing_order(&self) -> &[usize];
}
```

**Compilation allocates everything upfront:**
1. Collect nodes in topological order
2. Pre-allocate output buffer for each node output port
3. Build routing table from edges
4. Initialize latency state and delay lines

---

## PlaybackEngine Methods

```rust
impl PlaybackEngine {
    /// Create new engine
    pub fn new(sample_rate: u32, buffer_size: usize, tempo_map: Arc<TempoMap>) -> Self;

    /// Prepare for a compiled graph (allocate input_gather)
    pub fn prepare(&mut self, graph: &CompiledGraph);

    /// Process one buffer (hot path, no allocation)
    pub fn process(
        &mut self,
        graph: &mut CompiledGraph,
        regions: &[Region],
    ) -> Result<&AudioBuffer>;

    /// Transport control
    pub fn play(&mut self);
    pub fn stop(&mut self);
    pub fn seek(&mut self, beat: Beat);
    pub fn position(&self) -> PlaybackPosition;
    pub fn is_playing(&self) -> bool;

    /// Queue a mix-in from LatentManager
    pub fn queue_mix_in(&mut self, schedule: MixInSchedule);

    /// Current tempo at playhead
    pub fn current_tempo(&self) -> f64;
}
```

---

## The Process Loop (Hot Path)

```rust
fn process(&mut self, graph: &mut CompiledGraph, regions: &[Region]) -> Result<&AudioBuffer> {
    // 1. Check for scheduled mix-ins at current position
    self.apply_pending_mix_ins(regions);

    // 2. Process each node in topological order
    for node_idx in graph.processing_order() {
        if graph.failed_nodes.contains(&node_idx) {
            continue;  // skip failed nodes
        }

        // Clear and populate input gather (no allocation)
        self.input_gather[node_idx].clear();
        for route in &graph.routes {
            if route.dest_node == node_idx {
                self.input_gather[node_idx].push((route.src_buffer, route.gain));
            }
        }

        // Build input view (borrows only)
        let input_view = InputView {
            gather: &self.input_gather[node_idx],
            buffers: &graph.buffers,
        };

        // Clear output buffer
        graph.buffers[graph.buffer_map[node_idx].buffer_idx].clear();

        // Process node
        let ctx = self.make_context(graph);
        match graph.nodes[node_idx].process(&ctx, &input_view, &mut output_buf) {
            Ok(()) => {}
            Err(ProcessError::Skipped { .. }) => {
                // Output silence, continue
            }
            Err(ProcessError::Failed { reason }) => {
                tracing::error!("Node {} failed: {}", node_idx, reason);
                graph.mark_failed(node_idx);
            }
        }

        // Apply latency compensation via delay line
        self.apply_delay(node_idx, graph);
    }

    // 3. Copy master output
    if let Some(master_idx) = self.master_buffer_idx {
        self.output.copy_from(&graph.buffers[master_idx]);
    }

    // 4. Advance position
    self.advance_position();

    Ok(&self.output)
}
```

**Critical constraints:**
- No heap allocation in this function
- No locks held during audio processing
- All buffers pre-sized in `prepare()`

---

## Mix-In Handling

When a mix-in is scheduled:

```rust
fn apply_pending_mix_ins(&mut self, regions: &[Region]) {
    while let Some(schedule) = self.peek_next_mix_in() {
        if schedule.target_beat > self.position.beats {
            break;  // not yet
        }

        let schedule = self.mix_in_queue.pop_front().unwrap();

        match schedule.strategy {
            MixInStrategy::HardCut => {
                // Instant swap at beat boundary
                self.activate_region(schedule.region_id, regions);
            }
            MixInStrategy::Crossfade { beats } => {
                // Start crossfade
                self.active_crossfades.push(ActiveCrossfade {
                    old_region_id: self.find_current_region_at(schedule.target_beat),
                    new_region_id: schedule.region_id,
                    start_beat: schedule.target_beat,
                    end_beat: Beat(schedule.target_beat.0 + beats),
                    progress: 0.0,
                });
            }
            MixInStrategy::Bridge { .. } => {
                // Bridge already resolved as separate region
                // Just activate it, then the target region after
                self.activate_region(schedule.region_id, regions);
            }
        }
    }
}
```

---

## Signal Merge Semantics

| Signal | Merge |
|--------|-------|
| Audio | Sum with per-edge gain |
| MIDI | Event union, sorted by frame |
| Control | Average |
| Trigger | Union, sorted by frame |

---

## InputView Methods

```rust
impl<'a> InputView<'a> {
    /// Mix all audio sources to output buffer
    pub fn audio(&self, port_idx: usize, out: &mut AudioBuffer);

    /// Merge all MIDI sources to output buffer
    pub fn midi(&self, port_idx: usize, out: &mut MidiBuffer);

    /// Check if port has any connections
    pub fn is_connected(&self, port_idx: usize) -> bool;
}
```

---

## Offline Rendering

For exporting to file (not realtime):

```rust
pub fn render_to_file(
    graph: &Graph,
    regions: &[Region],
    tempo_map: &TempoMap,
    duration_beats: Beat,
    sample_rate: u32,
    buffer_size: usize,
    path: impl AsRef<Path>,
) -> Result<()>
```

Uses `hound::WavWriter`. ProcessContext mode is `Offline`.

**Note:** Offline rendering still only plays resolved regions. Latent regions must be resolved first (via LatentManager) before export.

---

## Graph Hot-Swap

For live modification without glitches:

```rust
pub struct PlaybackSession {
    engine: PlaybackEngine,
    current_graph: CompiledGraph,
    pending_graph: Option<CompiledGraph>,
    crossfade_state: GraphCrossfade,
}

impl PlaybackSession {
    /// Queue a new graph to swap in
    pub fn queue_graph_swap(&mut self, new_graph: CompiledGraph);

    /// Process with automatic graph crossfade
    pub fn process(&mut self, regions: &[Region]) -> Result<&AudioBuffer>;
}
```

Graph swaps happen at buffer boundaries with brief crossfade.

---

## Acceptance Criteria

- [ ] `CompiledGraph::compile` builds routing table
- [ ] `process()` calls nodes in topological order
- [ ] No allocation in `process()` — verify with custom allocator or profiler
- [ ] `ProcessError::Skipped` outputs silence, continues
- [ ] `ProcessError::Failed` marks node failed, skips in future
- [ ] `queue_mix_in()` schedules content introduction
- [ ] HardCut mix-in works at beat boundary
- [ ] Crossfade mix-in blends over specified duration
- [ ] `render_to_file` produces valid WAV
- [ ] Transport controls (play/stop/seek) work correctly
