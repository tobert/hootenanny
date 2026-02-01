# Hootenanny TODO

## Incoming

Dumping stuff here so we don't lose it.

### WebSocket Audio Streaming Exploration (2026-01-27)

**Current State - it works!**
- PCM2902 audio capture via PipeWire monitor input
- Monitor passthrough to output (can hear input in speakers)
- Streaming tap ring buffer captures final mix
- WebSocket endpoint at `/stream/live`
- HTTP API at `/ui`, `/artifacts`, `/stream/live/status`
- TLS enabled, access via `https://100.83.138.103:8082/ui`

**Issues Found:**

1. **Bind address not localhost**
   - Server binds to `100.83.138.103:8082` (Tailscale IP) instead of `127.0.0.1:8082`
   - Likely config in `~/.config/hootenanny/config.toml`

2. **No volume/level visualization in UI**
   - The UI has a canvas visualizer but no waveform drawing code
   - Just shows an empty dark rectangle

3. **Dead code warnings in test files** (from diagnostics)
   - `client_concurrency.rs:85` - `delayed_router` never used
   - `integration.rs:27` - `frames_to_multipart` never used
   - `integration.rs:58` - `HubStats.clients_seen` never read
   - `integration.rs:65-66` - `frontend_endpoint`, `backend_endpoint` never read
   - `mcp_client.rs:13-70` - `ToolInfo`, `McpClient` and fields never used
   - These are test scaffolding that was never finished or used

**Architecture:**
```
PCM2902 → MonitorInputStream → AudioRingBuffer → PipeWireOutputStream → Speakers
                                                         │
                                                         ▼
                                                   StreamingTap ──► WebSocket ──► Browser AudioWorklet ──► Speakers
```

---

- let's keep a note - clap is too slow to be blocking for audio analyze, maybe it could include a job id for it, and only when asked to
  with a param. also clap seems to be running on cpu. we'll pursue that in another session. also analyze could look at just how much of
  file is sparse / 0's to help with this. a lot of our empty wavs are all zeroes. could maybe also do some band pass analysis in there
  that's quick?

### MIDI Validation Session (2026-01-20)

**What worked:**
- Full MIDI chain validated: `midi_send` → NiftyCASE → PCM2902 capture → CAS artifact
- Audio capture confirmed with ffmpeg volumedetect: -18.2 dB mean (real audio) vs -91 dB (silence)
- MIDI code improvements landed (10 commits): mutex safety, duplicate prevention, partial send failure handling, raw MIDI, publisher infrastructure

**Gaps / Bugs found:**

1. **Monitor UX confusion**: `audio_capture source=monitor` silently captures silence if `audio_monitor enabled=false`.
   Could either auto-enable monitor when capturing from that source, or return a warning/error.

2. **Job tracking lost job**: CLAP `audio_analyze` job timed out at 30s, then disappeared from `job_list` entirely,
   but the Python process kept running (303 min CPU). Jobs should persist in list with failed/timeout status.

3. **Missing tool: quick audio level check**: Had to shell out to `ffmpeg -af volumedetect` to verify audio wasn't silent.
   Could add `audio_info` or `audio_stats` tool that returns peak/mean dB, duration, sample rate without needing GPU.

4. **Beat detection silent failure**: `beats_detect` on silent audio just fails with no useful message.
   Could detect silence first and return "audio appears silent" error.

5. **No tool to cancel orphaned jobs**: `job_cancel` exists but if job disappears from list, no way to kill it.
   Could add process tracking or `job_kill_all` for cleanup.


## Actionable

(none)

---

## Completed Work

### RAVE Streaming RT Callback Fix
**Completed**: 2026-01-14
Fixed double `try_lock()` bug in RT callback that prevented audio from reaching RAVE.
- Combined check-and-write into single lock scope
- Added RT stats: `rave_writes`, `rave_samples_written`, `rave_reads`, `rave_samples_read`
- Exposed stats in `rave_stream_status` response for debugging

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
