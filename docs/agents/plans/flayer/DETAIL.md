# Flayer Design Rationale

**Purpose:** Deep design context for revision sessions. Read this when you need to understand *why* decisions were made, not *what* to build.

---

## Why Four Primitives?

We tried many decompositions. Four survived:

| Primitive | Why It's Primitive |
|-----------|-------------------|
| **Time** | Everything else depends on time representation. Musical vs physical is a fundamental choice. |
| **Signal** | Typed data flow enables compile-time correctness. Audio/MIDI/Control/Trigger have different semantics. |
| **Node** | Uniform processing interface lets AI models and DSP share the same graph. |
| **Region** | Timeline behaviors need identity, position, duration. This is the minimal representation. |

**What we rejected:**
- "Clip" — too specific, implies audio. Region with PlayContent behavior is more general.
- "Track" — organizational, not primitive. Built from regions + routing.
- "Parameter" — subsumed by Control signals and node descriptors.

## Why Musical Time Primary?

DAWs traditionally use samples as ground truth, converting to beats for display. This causes drift and complexity.

**Our choice:** Beats are primary. Samples are derived via TempoMap.

**Implications:**
- A region at beat 4 stays at beat 4 regardless of tempo changes
- Tempo automation "just works" — it changes the TempoMap, not region positions
- Live tempo sync updates TempoMap, everything follows
- Sample-accurate rendering derives positions at render time

**Trade-off:** Quantization when converting beats→samples. We accept this; music is inherently quantized.

## Why Signals Are Typed?

Four signal types with different merge semantics:

| Type | Merge Behavior | Why |
|------|----------------|-----|
| Audio | Additive (sum) | Physics: sound waves superpose |
| MIDI | Event union (sorted) | Multiple instruments, one timeline |
| Control | Average | Competing automation should blend |
| Trigger | Union (sorted) | Events don't cancel each other |

**Alternative considered:** Untyped buffers with runtime checks. Rejected because:
- Compile-time errors are better than runtime errors
- Type information enables optimization (e.g., MIDI buffers are sparse)
- Schema for Trustfall queries needs types

## Why Nodes Are Uniform?

AI models (Orpheus, RAVE, Notochord) and DSP (gain, EQ, compressor) share the same `Node` trait.

**Why this matters:**
- Graph doesn't care what's inside a node
- Routing is declarative, not procedural
- Agents can reason about the graph without understanding node internals
- Latency compensation works uniformly

**External process pattern:** Most AI models run in Python. Flayer nodes wrap IPC:
- RPC for offline (blocks until result)
- Ring buffers for real-time (non-blocking read)
- Streaming for continuous processing

The graph sees uniform nodes. Communication complexity is encapsulated.

## Why Regions Have Behaviors?

A region is position + duration + behavior. Behaviors include:
- PlayContent — play audio/MIDI from CAS
- GenerateContent — call MCP tool, store result
- ApplyProcessing — modulate a parameter over time
- EmitTrigger — fire a discrete event

**Why not separate types?** We considered AudioRegion, MidiRegion, GenerativeRegion...

Rejected because:
- Regions can transform (generate → resolved → playable)
- Uniform querying: "all regions in chorus" shouldn't care about type
- Behaviors are extensible via Custom variant

## Why Content-Addressed Storage?

All content (audio, MIDI, generated) lives in CAS (content-addressed storage).

**Benefits:**
- **Lineage:** Know what generated what (artifact parent tracking)
- **Deduplication:** Same content = same hash = one copy
- **Reproducibility:** Same generation params = same hash = can verify
- **Caching:** Already have this hash? Don't regenerate

**Integration:** Flayer uses `hootenanny` crate for CAS. Regions reference content by hash.

## Why Same Graph for Offline and Realtime?

Traditional approach: separate "arrangement" and "live" modes with different architectures.

**Our approach:** One graph, different `ProcessContext.mode`:
- `Offline` — can block, take as long as needed
- `Realtime { deadline_ns }` — must complete in time

**Node behavior adapts:**
- RPC nodes work offline, skip in realtime
- Ring buffer nodes work in both modes
- Nodes report capabilities via descriptor

**Why this matters:**
- What you hear in preview = what you render
- No "it worked in preview but not in render" bugs
- Simpler mental model

## Cross-Cutting Concerns

### Error Handling in Nodes

Nodes can fail. Two error types:
- **Skipped** — transient (network timeout, buffer underrun). Output silence, try again.
- **Failed** — permanent (process died). Mark node failed, skip in future.

The render loop continues either way. No panics in the hot path.

### Latency Compensation (PDC)

Network AI models have variable latency. The graph compensates:
1. Each node reports latency (atomically updated for network nodes)
2. Background thread calculates compensation delays
3. Faster paths get delayed to match slowest path
4. Delay lines are pre-allocated (no hot path allocation)

### Buffer Management

**Hot path rule:** The render loop never allocates.

All buffers pre-allocated during graph compilation:
- Output buffers for each node
- Input gather structures
- Delay lines for PDC

This is non-negotiable for real-time audio.

### Multi-Graph Variants

Multiple compiled graphs can exist simultaneously (A/B comparison, undo stack).

Each is a "slot" with:
- Active/inactive state
- Gain (for crossfading)
- Fade rate

Crossfade between variants for smooth transitions.

## Integration Points

### With Hootenanny

Flayer depends on `hootenanny` for:
- CAS (content storage)
- Artifacts (lineage tracking)
- MCP tools (resolution calls)

Flayer does NOT re-implement these. It calls them.

### With Trustfall

The query layer exposes everything to Trustfall:
- Regions (by position, by tag, by resolution state)
- Nodes (by type, traversal)
- Time (conversions)

Agents use queries to reason about the graph.

### With PipeWire

External I/O uses PipeWire for:
- Hardware audio I/O
- Inter-app routing
- MIDI devices

Feature-gated (`pipewire` feature). Flayer works without it (offline only).

## Open Questions

_Record unresolved design questions here._

| Question | Context | Status |
|----------|---------|--------|
| - | - | - |

## Rejected Alternatives

_Record alternatives we considered and rejected._

| Alternative | Why Rejected |
|-------------|--------------|
| Sample-based time primary | Drift, complexity with tempo changes |
| Untyped signal buffers | Runtime errors, no optimization |
| Separate region types | Transformation complexity, querying pain |
| Separate offline/realtime graphs | "Works in preview not render" bugs |
