# Flayer: Compute Graph for Musical Time

**Location:** `crates/flayer`
**Status:** Design Complete, Implementation Pending

---

## Progress Tracking

| Task | Status | Assignee | Notes |
|------|--------|----------|-------|
| 01-primitives | pending | - | Time, Signal, Node, Region types |
| 02-graph | pending | - | petgraph DAG, topology |
| 03-resolution | pending | - | MCP tool dispatch |
| 04-rendering | pending | - | Buffer management, hot path |
| 05-external-io | pending | - | PipeWire integration |
| 06-query | pending | - | Trustfall adapter |
| 07-patterns | pending | - | Track, Bus, Section conveniences |

## Current Status

- **Completed**: None
- **In Progress**: None
- **Next Up**: 01-primitives
- **Blocked**: None

## Success Metrics

We'll know we've succeeded when:
- [ ] Primitives compile and TempoMap converts time correctly
- [ ] Graph builds, toposorts, and detects cycles
- [ ] Resolution calls MCP tools and stores results
- [ ] Rendering produces valid WAV from a simple graph
- [ ] PipeWire feature compiles and plays audio
- [ ] Trustfall queries return regions and nodes
- [ ] Timeline builds a routed graph from tracks/buses

## Open Questions

- [ ] Should regions support overlapping on same track?
- [ ] How to handle tempo changes mid-render for realtime?
- [ ] Latency compensation strategy for network AI nodes?

## Signoffs & Decisions

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-12-08 | Design docs complete | Claude/Gemini/Human collaboration |
| - | - | - |

## Session Notes

_Use this section to record context for future sessions._

---

## What Flayer Is

A **compute graph engine** for musical time:
- **Signals** (audio, MIDI, control, triggers) flow through **nodes**
- **Nodes** process signals (sources, effects, mixers, AI models)
- **Regions** place behaviors on a **timeline**
- **Time** is musical (beats) not physical (samples)

Flayer is infrastructure. Not a DAW — the engine a DAW could use.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Query Layer (Trustfall) — agents reason about everything    │
├─────────────────────────────────────────────────────────────┤
│ Patterns — Track, Bus, Section, Project conveniences        │
├─────────────────────────────────────────────────────────────┤
│ Resolution — latent → concrete via MCP tools                │
├─────────────────────────────────────────────────────────────┤
│ Rendering — allocation-free graph processing                │
├─────────────────────────────────────────────────────────────┤
│ External I/O — PipeWire, MIDI devices                       │
├─────────────────────────────────────────────────────────────┤
│ Graph — petgraph DAG, topological processing                │
├─────────────────────────────────────────────────────────────┤
│ Primitives — Time • Signal • Node • Region                  │
├─────────────────────────────────────────────────────────────┤
│ Content Store (CAS) — via hootenanny crate                  │
└─────────────────────────────────────────────────────────────┘
```

## Crate Structure

```
crates/flayer/
├── src/
│   ├── lib.rs
│   ├── primitives.rs     # 01: Time, Signal, Node, Region
│   ├── graph.rs          # 02: Graph topology
│   ├── resolution.rs     # 03: Latent → concrete
│   ├── rendering.rs      # 04: ProcessContext, buffers
│   ├── external_io.rs    # 05: PipeWire (feature-gated)
│   ├── query.rs          # 06: Trustfall adapter
│   └── patterns.rs       # 07: Track, Bus, Timeline
└── Cargo.toml
```

## Dependencies

```toml
[dependencies]
hootenanny = { path = "../hootenanny" }
uuid = { version = "1", features = ["v4", "serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
petgraph = "0.6"
trustfall = "0.8"
hound = "3.5"
midly = "0.5"
rustysynth = "1.3"
tracing = "0.1"

[features]
default = []
pipewire = ["dep:pipewire"]
```

## Design Principles (for reference)

1. **Primitives over patterns** — foundation is stable, patterns evolve
2. **Agents are first-class** — AI models are nodes in the graph
3. **Queryable everything** — Trustfall enables reasoning
4. **Time is musical** — beats, not samples
5. **Content is immutable** — CAS for lineage/dedup
6. **Offline = realtime** — same graph, different ProcessContext

## Documents

| Document | Focus | Read When |
|----------|-------|-----------|
| [DETAIL.md](./DETAIL.md) | Full design rationale, cross-cutting concerns | Deep revision sessions |
| [01-primitives](./01-primitives.md) | Time, Signal, Node, Region | Implementing primitives.rs |
| [02-graph](./02-graph.md) | DAG topology, petgraph usage | Implementing graph.rs |
| [03-resolution](./03-resolution.md) | MCP dispatch, quality filters | Implementing resolution.rs |
| [04-rendering](./04-rendering.md) | Hot path, buffer management | Implementing rendering.rs |
| [05-external-io](./05-external-io.md) | PipeWire integration | Implementing external_io.rs |
| [06-query](./06-query.md) | Trustfall adapter | Implementing query.rs |
| [07-patterns](./07-patterns.md) | Track, Bus, Section | Implementing patterns.rs |
