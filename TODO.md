# Hootenanny TODO

## Actionable

### 1. Debug RAVE Streaming Audio Flow
**Files:** `crates/chaosgarden/src/rave_streaming.rs`, `pipewire_output.rs`
**Effort:** Debug session

Audio not reaching Python RAVE service. Infrastructure is in place but `frames_processed: 0`.

**Symptoms:**
- `rave_stream_start` succeeds (Python binds ZMQ PAIR, chaosgarden connects)
- Monitor audio captured: ✅ (569k samples)
- Monitor reads in RT callback: ✅ (1026 reads)
- Python RAVE frames_processed: ❌ (0)

**Debug checklist:**
1. Check ZMQ PAIR connection: `journalctl -u rave.service` / `chaosgarden.service`
2. Verify `rave_active` check passes in RT callback (add tracing)
3. Confirm `producer.write()` is called and returns > 0
4. Check if RaveStreamingClient thread is running and reading from consumer
5. Verify ZMQ send/recv in streaming loop

**Possible issues:**
- ZMQ PAIR not connecting (bind/connect timing)
- RT callback `try_lock` always failing
- Ring buffer producer/consumer mismatch
- Python streaming loop not receiving data

---

## Completed Work

### RAVE Streaming Audio Path Routing
**Completed**: 2026-01-14
When RAVE streaming is active, raw monitor is muted - only RAVE-processed audio to output.

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
