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
**Status**: Observed 2026-01-14
**Context**: During RAVE demo playback

When playing RAVE-generated audio through timeline regions, observed 131,581 underruns
on the audio output. The audio wasn't audible during timeline playback, though the
artifacts play fine when downloaded directly.

**Symptoms:**
- `audio_output_status` shows high underrun count
- Timeline shows `state: "playing"` with advancing position
- No audible audio output
- Direct artifact download/playback works fine

**Possible causes to investigate:**
- Timeline region audio loading from CAS may be blocking the audio thread
- Buffer size (256 frames) may be too small for disk I/O latency
- Region behavior `play_audio` implementation may have issues with CAS-backed content
- Possible thread contention between region loading and audio callback

**When to fix:**
- Before live performance use cases
- When timeline playback is critical path

**Workaround:**
- Download artifacts directly via `/artifact/{id}` and play externally
