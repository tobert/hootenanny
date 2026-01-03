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
