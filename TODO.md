# Hootenanny TODO

## Actionable

### 1. Audio Capture Tool
**Files:** `crates/hootenanny/src/api/typed_dispatcher.rs`, `crates/hooteproto/`
**Effort:** Medium

Add `audio_capture` tool to record from monitor input to CAS for offline processing (RAVE, etc).

**Interface:**
```json
{
  "tool": "audio_capture",
  "duration_seconds": 5.0,
  "source": "monitor"  // or "timeline", "mix"
}
```

**Returns:** `{ "artifact_id": "...", "content_hash": "...", "duration_seconds": 5.0 }`

**Implementation:**
- Read from `streaming_tap_consumer` in chaosgarden
- Accumulate samples for duration
- Encode to WAV, store in CAS
- Create artifact

---

## Completed Work

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
