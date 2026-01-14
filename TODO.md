# Hootenanny TODO

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
**Status**: Analyzed 2026-01-14
**Context**: During RAVE demo playback

When playing RAVE-generated audio through timeline regions, observed 131,581 underruns
on the audio output. The audio wasn't audible during timeline playback, though the
artifacts play fine when downloaded directly.

**Symptoms:**
- `audio_output_status` shows high underrun count
- Timeline shows `state: "playing"` with advancing position
- No audible audio output
- Direct artifact download/playback works fine

**Root causes identified:**

1. **Vec allocation in RT callback** (`pipewire_output.rs:421-422`)
   ```rust
   let mut output_buffer = vec![0.0f32; samples_needed];
   let mut temp_buffer = vec![0.0f32; samples_needed];
   ```
   Allocates ~2KB per callback. Memory allocation can stall the RT thread.
   **Fix:** Pre-allocate buffers outside the callback, reuse them.

2. **Mutex contention on timeline_ring** (`pipewire_output.rs:445-448`)
   ```rust
   let timeline_read = timeline_ring
       .try_lock()
       .map(|mut r| r.read(&mut temp_buffer))
       .unwrap_or(0);
   ```
   When `process_playback()` holds the lock, RT callback gets 0 samples → underrun.
   **Fix:** Replace Mutex with lock-free SPSC ring buffer (like monitor input uses).

3. **tick() rate** (`daemon.rs:554-614`)
   If tick() can't keep up with audio consumption, the ring buffer drains.

**Evidence:**
- Monitor input uses lock-free SPSC → 97% success rate (14.9M/15.3M callbacks)
- Timeline uses Mutex → causes the 0.9% underrun rate
- Underruns only counted when BOTH sources return nothing

**When to fix:**
- Before live performance use cases
- When timeline playback is critical path

**Workaround:**
- Download artifacts directly via `/artifact/{id}` and play externally

---

### Artifact URL Accessibility
**Status**: Observed 2026-01-14
**Context**: Trying to share RAVE output with user

Getting the URL for an artifact is too hard. The `artifact_url` field augmentation exists
in holler but only works when:
1. The response contains `artifact_id` field (not `id`)
2. A `base_url` is configured in hooteconf

**Current pain:**
- `artifact_get` returns `id` not `artifact_id`, so no URL augmentation
- User has to manually construct URL: `http://localhost:8082/artifact/{id}`
- No easy way to get the configured base URL via MCP tools

**Desired behavior:**
- Every artifact response should include a ready-to-use URL
- `artifact_get` should return `url` or `content_url` field
- URL should work whether local or behind reverse proxy

**Fix options:**
1. Add `url` field directly to artifact responses in hootenanny
2. Fix augmentation to also check `id` field, not just `artifact_id`
3. Add `artifact_url` tool that returns full URL for an artifact ID
