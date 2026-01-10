# Proposal: Reduce hooteproto to High-Level Creative Interfaces

**Date:** 2026-01-10
**Status:** Draft for discussion
**Author:** Claude + Al

---

## The Question

What if we reduce hooteproto to just the DAW tools (and vibeweaver), with everything else becoming internal to hootenanny?

---

## Current State

hooteproto currently defines **~80 tool variants** across the wire protocol:

| Category | Tools | Count |
|----------|-------|-------|
| **Orpheus** | generate, seeded, continue, bridge, loops, classify | 6 |
| **MusicGen** | generate | 1 |
| **YuE** | generate | 1 |
| **BeatThis** | analyze | 1 |
| **CLAP** | analyze | 1 |
| **ABC** | parse, to_midi, validate, transpose | 4 |
| **MIDI/Audio** | convert_midi_to_wav, soundfont_inspect, preset_inspect | 3 |
| **CAS** | store, inspect, get, upload_file, stats | 5 |
| **Artifacts** | upload, get, list, create | 4 |
| **Graph** | query, bind, tag, connect, find, context, add_annotation | 7 |
| **Jobs** | execute, status, poll, cancel, list, sleep | 6 |
| **Garden** | status, play, pause, stop, seek, tempo, query, regions, audio, input, monitor | 18 |
| **Vibeweaver** | eval, session, reset, help | 4 |
| **DAW** | sample, extend, bridge, project, analyze, schedule | 6 |
| **Other** | config, lua, resources, completion, sampleLlm, help | ~10 |

The DAW tools already abstract over the model-specific tools. `sample(space="orpheus")` calls `orpheus_generate()` internally. `analyze(tasks=["beats"])` calls `beatthis_analyze()` internally.

---

## Proposed Architecture

### Two High-Level Creative Interfaces

**1. DAW Tools** — Declarative music operations

```
sample(space, prompt?, seed?)     → Generate new content
extend(encoding)                  → Continue existing content
bridge(from, to?)                 → Create transitions
project(encoding, target)         → Convert formats (MIDI→audio)
analyze(encoding, tasks)          → Extract information
schedule(encoding, at, duration)  → Place on timeline
```

**2. Vibeweaver** — Programmatic Python interface

```
weave_eval(code)    → Execute Python in music kernel
weave_session()     → Get session state
weave_reset()       → Clear kernel
weave_help(topic?)  → Documentation
```

An agent chooses their paradigm:
- "Generate a jazzy piano loop" → `sample(space="orpheus", prompt=...)`
- "Build a generative pattern with probability rules" → `weave_eval("...")`

### What Stays in hooteproto (Public API)

```
hooteproto/
├── daw/           # sample, extend, bridge, project, analyze
├── garden/        # play, pause, stop, seek, tempo, schedule, regions
├── weave/         # eval, session, reset, help
└── jobs/          # poll, status, list, cancel (async orchestration)
```

### What Moves Internal to hootenanny

```
hootenanny (internal implementation)
├── models/
│   ├── orpheus/      # generate, continue, bridge, loops, classify
│   ├── musicgen/     # generate
│   ├── yue/          # generate
│   ├── beatthis/     # analyze
│   └── clap/         # analyze
├── cas/              # content-addressable storage
├── artifacts/        # metadata store (or expose through graph?)
├── graph/            # trustfall queries (or expose through vibeweaver?)
├── abc/              # notation parsing
└── soundfont/        # SF2 inspection
```

---

## Open Questions

### 1. Where do Artifacts belong?

**Option A: Internal** — Artifacts are an implementation detail. DAW tools return artifact_ids, but there's no direct artifact_list/artifact_get.

**Option B: Public** — Keep artifact tools in hooteproto. Agents need to browse/manage their creations.

**Option C: Through Vibeweaver** — `weave_eval("artifacts.list(tag='type:midi')")` exposes it programmatically.

Your thoughts?

---

### 2. Where does Graph belong?

**Option A: Internal** — Graph queries are implementation details. DAW/vibeweaver handle what agents need.

**Option B: Public** — Keep graph_query, graph_context in hooteproto for advanced queries.

**Option C: Through Vibeweaver** — `weave_eval("graph.query('{ Artifact { id } }')")` — Python becomes the query interface.

Your thoughts?

---

### 3. What about CAS?

Currently agents sometimes use `cas_upload_file` to bring in external files. Options:

**Option A: Internal** — Use `artifact_upload` instead (if artifacts stay public).

**Option B: Keep cas_upload_file** — Just this one tool for importing external content.

**Option C: Through Vibeweaver** — `weave_eval("cas.upload('/path/to/file.mid')")`

Your thoughts?

---

### 4. Config and other utilities?

`config_get` is useful for checking paths, model availability, etc.

**Option A: Remove** — Agents don't need to know implementation details.

**Option B: Keep** — Useful for debugging and understanding the environment.

**Option C: Read-only resource** — Expose as MCP resource instead of tool.

Your thoughts?

---

## Benefits of This Change

1. **Cleaner agent experience** — LLMs see 20 tools instead of 80. Less cognitive load, better tool selection.

2. **Stable public API** — hooteproto becomes the contract. We can refactor orpheus/musicgen/etc without breaking the public interface.

3. **Implementation flexibility** — Could swap Orpheus for a different MIDI model, add new analysis backends, change CAS implementation — all invisible to agents.

4. **Clearer separation of concerns** — hooteproto = "what can I do?", hootenanny = "how does it work?"

---

## Risks / Concerns

1. **Power user escape hatch** — Some agents/users want low-level control. Do we need a way to access internal tools for debugging?

2. **Migration path** — Existing code/scripts using orpheus_generate directly would break.

3. **Vibeweaver complexity** — If we route everything through vibeweaver, it becomes a god-object.

4. **Garden's role** — Is Garden part of DAW or its own interface? Timeline control feels DAW-ish, but it's also the audio engine.

---

## Next Steps

Once we align on scope:

1. Define the reduced hooteproto schema
2. Move internal tools to hootenanny-only dispatch
3. Update holler's MCP exposure
4. Migration guide for any breaking changes

---

## Your Notes

<!-- Add your thoughts, questions, or decisions here -->




