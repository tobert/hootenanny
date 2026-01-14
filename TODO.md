# Hootenanny TODO

## Actionable

### 1. Fix Artifact URL Augmentation
**File:** `crates/holler/src/handler.rs:261-289`
**Effort:** Small

The `augment_artifact_urls()` function only looks for `artifact_id` field, but `artifact_get`
returns `id`. Fix the augmentation to also check `id` field.

```rust
// Current (line 265):
if let Some(serde_json::Value::String(id)) = map.get("artifact_id") {

// Should also check:
} else if let Some(serde_json::Value::String(id)) = map.get("id") {
```

### 2. Audio Capture Tool
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

### 3. Pre-allocate RT Buffers
**File:** `crates/chaosgarden/src/pipewire_output.rs:421-422`
**Effort:** Small

Move buffer allocation outside the RT callback. Currently allocates on every callback:
```rust
let mut output_buffer = vec![0.0f32; samples_needed];
let mut temp_buffer = vec![0.0f32; samples_needed];
```

**Fix:** Store pre-allocated buffers in the listener user data, sized for max expected frames.

### 4. Lock-free Timeline Ring
**File:** `crates/chaosgarden/src/pipewire_output.rs:445-448`, `daemon.rs`
**Effort:** Medium

Replace `Arc<Mutex<RingBuffer>>` for timeline with `AudioRingProducer`/`AudioRingConsumer`
(same pattern monitor input already uses successfully).

**Changes needed:**
- `daemon.rs`: Create SPSC pair, keep producer for `process_playback()` writes
- `pipewire_output.rs`: Take consumer, use lock-free `consumer.read()` instead of `try_lock()`
- Remove `timeline_ring: RwLock<Option<Arc<Mutex<RingBuffer>>>>` field

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

### Audio Output Underruns (chaosgarden)
**Status**: Analyzed 2026-01-14 → See Actionable #3, #4

Root causes identified: RT allocations and Mutex contention on timeline ring.
Monitor input (lock-free SPSC) has 97% success rate; timeline (Mutex) causes underruns.

---

### Artifact URL Accessibility
**Status**: Observed 2026-01-14 → See Actionable #1

The `augment_artifact_urls()` only checks `artifact_id`, not `id` field.
