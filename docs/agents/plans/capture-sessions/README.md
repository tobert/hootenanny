# Capture Sessions

**Status:** Design Complete
**Crates:** `crates/cas`, `crates/chaosgarden`, `crates/hootenanny`

Record audio/MIDI streams from hardware into artifacts with seamless slicing.

---

## Architecture Summary

```
hootenanny (control)                   chaosgarden (RT)
      │                                      │
      │── StreamStart{uri, path_0} ─────────▶│ mmap, write samples
      │◀── StreamHeadPosition ───────────────│ (Broadcast)
      │◀── StreamChunkFull{path_0} ──────────│
      │  seal(path_0), create path_1         │
      │── StreamSwitchChunk{path_1} ────────▶│ close, mmap new
      │                                      │
```

- **CAS** owns staging layout (`staging/` + `content/`, seal = rename or copy)
- **Hootenanny** manages file lifecycle, manifests, sessions, slicing
- **Chaosgarden** only does I/O: open → mmap → write → notify → close

---

## Progress

| Task | Status | Group | Crate | Notes |
|------|--------|-------|-------|-------|
| 01-cas-staging | **done** | A | cas | Staging API complete |
| 02-protocol-types | **done** | A | hooteproto | Cap'n Proto schemas |
| 03-chaosgarden-io | **done** | B | chaosgarden | stream_io.rs with mmap + tests |
| 04-stream-management | **done** | B | hootenanny | StreamManager + manifest + slicing |
| 05-session-management | **done** | C | hootenanny | SessionManager + timeline |
| 06-slicing | **done** | C | hootenanny | SlicingEngine with virtual/materialized |
| 07-mcp-tools | **done** | D | hootenanny | Tools scaffolded, need impl |
| 08-zmq-integration | **done** | E | both | ZMQ protocol complete (ad96460, e8527df) |
| 09-pipewire-binding | pending | E | chaosgarden | Connect to hardware |
| 10-end-to-end-test | pending | F | hootenanny | Full capture flow |

**Current Phase:** ZMQ protocol integration complete (8 tasks, ~4,200 LOC total)

### Next Steps (Task 09: PipeWire Binding)

1. **Implement PipeWire stream creation** in chaosgarden:
   - Create PipeWire ports for audio/MIDI input
   - Connect to hardware devices via PipeWire graph
   - Set up process callbacks for sample delivery

2. **Wire PipeWire callbacks to stream_io**:
   - In process callback, call stream_manager.write_samples()
   - Handle chunk full condition returned from write_samples()
   - Trigger StreamChunkFull broadcast when chunk fills

3. **Device identity mapping**:
   - Map PipeWire node names to device identities
   - Handle device connect/disconnect events
   - Update stream status on device state changes

4. **Test with real hardware**:
   - Capture from audio interface
   - Verify chunk rotation with real data flow
   - Test MIDI device capture

---

## Execution Flow

```mermaid
graph TD
    subgraph A[Group A - parallel]
        A1[01-cas-staging]
        A2[02-protocol-types]
    end
    subgraph B[Group B - parallel]
        B1[03-chaosgarden-io]
        B2[04-stream-management]
    end
    subgraph C[Group C - parallel]
        C1[05-session-management]
        C2[06-slicing]
    end
    subgraph D[Group D]
        D1[07-mcp-tools]
    end
    A1 --> B1
    A1 --> B2
    A2 --> B1
    A2 --> B2
    B1 --> C1
    B2 --> C1
    B2 --> C2
    C1 --> D1
    C2 --> D1
```

---

## Agent Dispatch

### Group A (2 agents, parallel)
```
Agent 1: "Read 01-cas-staging.md and implement. No other tasks."
Agent 2: "Read 02-protocol-types.md and implement. No other tasks."
```

### Group B (2 agents, parallel, after A complete)
```
Agent 1: "Read 03-chaosgarden-io.md and implement."
Agent 2: "Read 04-stream-management.md and implement."
```

### Group C (2 agents, parallel, after B complete)
```
Agent 1: "Read 05-session-management.md and implement."
Agent 2: "Read 06-slicing.md and implement."
```

### Group D (1 agent, after C complete)
```
Agent 1: "Read 07-mcp-tools.md and implement."
```

### Output Format
When complete, report:
- Files modified (paths)
- Tests added/passing
- Blockers or follow-up discovered

---

## Success Metrics

- [x] `cargo test -p cas` passes with staging tests
- [x] `cargo test -p hooteproto` passes with message types
- [x] `cargo test -p chaosgarden` passes with stream I/O tests
- [x] `cargo test -p hootenanny` passes with stream/session tests
- [x] All builds clean: lib ✓, bin ✓, tests ✓
- [ ] **ZMQ protocol handlers** - StreamStart/Stop/SwitchChunk
- [ ] **Message flow works** - Commands → chaosgarden → broadcasts
- [ ] **End-to-end:** start stream → write samples → seal chunk → slice → artifact created
- [ ] **PipeWire integration** - Bind to real hardware devices
- [ ] No new warnings (`cargo clippy`)

---

## Key Types (Quick Reference)

```capnp
# New schema: schemas/streams.capnp

# Commands (add to envelope.capnp Payload union)
struct StreamStart { uri, definition, chunkPath }
struct StreamSwitchChunk { uri, newChunkPath }
struct StreamStop { uri }

# Events (add to broadcast.capnp Broadcast union)
struct StreamHeadPosition { streamUri, samplePosition, bytePosition, wallClock }
struct StreamChunkFull { streamUri, path, bytesWritten, samplesWritten, wallClock }
struct StreamError { streamUri, error, recoverable }
```

```rust
// Rust domain types (crates/hootenanny)
struct StreamUri(String);           // "stream://eurorack-audio/main"
enum CasAddress { Content(Blake3Hash), Staging(Blake3Hash) }
struct StreamManifest { stream_uri, chunks: Vec<ChunkRef>, .. }
struct CaptureSession { id, streams, segments, timeline, status }
```

Full types in [DETAIL.md](./DETAIL.md#type-definitions).

---

## Documents

| Doc | Purpose | When to Read |
|-----|---------|--------------|
| [DETAIL.md](./DETAIL.md) | Design rationale, full types, boundary spec | Deep context, revisions |
| 01-cas-staging.md | CAS staging implementation | Implementing task 01 |
| 02-protocol-types.md | ZMQ messages, stream types | Implementing task 02 |
| 03-chaosgarden-io.md | RT stream I/O | Implementing task 03 |
| 04-stream-management.md | Hootenanny stream lifecycle | Implementing task 04 |
| 05-session-management.md | Session/segment handling | Implementing task 05 |
| 06-slicing.md | Slice API implementation | Implementing task 06 |
| 07-mcp-tools.md | MCP tool exposure | Implementing task 07 |

---

## Open Questions

| Question | Status |
|----------|--------|
| Chunk size tuning (128K samples) | Experiment after basic impl |
| mmap flags for RT | Experiment |
| Device disconnect handling | Design during impl |
