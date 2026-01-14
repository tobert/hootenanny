# Hootenanny TODO

## Actionable

### 1. RAVE Streaming Audio Path Fix
**Files:** `crates/chaosgarden/src/pipewire_output.rs`
**Effort:** Small (~10 lines)

When RAVE streaming is active, raw monitor audio should NOT be mixed into output.
Currently both raw monitor and RAVE-processed audio are mixed, causing doubled audio.

**Fix:**
In the RT callback, check if RAVE input is active before adding raw monitor to output:
```rust
// Only add raw monitor to output if RAVE is NOT active
let rave_active = rave_input.as_ref()
    .and_then(|r| r.try_lock().ok())
    .map(|g| g.is_some())
    .unwrap_or(false);

if !rave_active {
    // Add raw monitor to output
    for i in 0..read {
        output_slice[i] += temp_slice[i] * gain;
    }
}
// Always send to RAVE if available
if let Some(ref rave_in) = rave_input { ... }
```

---

## Completed Work

### Audio Capture Tool
**Completed**: 2026-01-14
Added `audio_capture` MCP tool to record from streaming tap to CAS artifact.

### RAVE Realtime Streaming Infrastructure
**Completed**: 2026-01-14
- `rave_streaming.rs` - ZMQ PAIR client with dedicated thread
- Lock-free SPSC rings for RT ↔ non-RT audio transport
- Coordinated startup via hootenanny (Python RAVE + chaosgarden)
- PipeWire RT callback forks monitor to RAVE, mixes RAVE output

### Artifact URL Augmentation
**Completed**: 2026-01-14
`augment_artifact_urls()` now checks both `artifact_id` and `id` fields.

### Pre-allocated RT Buffers
**Completed**: 2026-01-14
Moved buffer allocation outside RT callback. Buffers sized for 8192 frames max.

### Lock-free Timeline Ring
**Completed**: 2026-01-14
Replaced `Arc<Mutex<RingBuffer>>` with `AudioRingProducer`/`AudioRingConsumer` SPSC pair.
Timeline audio now uses the same lock-free pattern as monitor input.

---

## Deferred Work

### Vibeweaver Clock Sync (Phase 5)
**Status**: Deferred 2026-01-03
**Context**: `~/.claude/plans/hazy-soaring-candy.md`

Current implementation uses simple approach - `@on_beat` callbacks fire when hootenanny
broadcasts `BeatTick` events. No local clock in vibeweaver.

**When to implement full clock sync:**
- Sub-beat timing precision needed for live performance
- Lookahead scheduling (knowing beat N+4 is coming before it happens)
- Jitter-free callback timing independent of network latency

**What it would involve:**
- `vibeweaver/src/local_clock.rs` - Local clock with PLL-style drift correction
- Sync events from chaosgarden (~1/second or on transport change)
- Local tick loop (~100Hz) firing callbacks from local time
- Transport events (play/stop/seek) bypass normal sync for immediacy

For now, broadcast-driven callbacks are good enough for generative scheduling.
