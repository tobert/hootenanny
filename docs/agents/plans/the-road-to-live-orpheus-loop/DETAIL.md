# Design Rationale

**Purpose:** Deep context for revision sessions. Read when you need to understand *why*.

---

## Why PipeWire (not JACK, ALSA, etc.)?

PipeWire is the modern Linux audio stack:
- Replaces both PulseAudio and JACK
- Low latency (~2.67ms callbacks measured)
- Session management built-in
- `pipewire-rs` crate is well-maintained

JACK would require separate server setup. ALSA is too low-level. PipeWire just works on modern systems.

## Why Dedicated Render Thread?

Two options for feeding audio to PipeWire:

1. **Render in PipeWire callback** — Complex because PlaybackEngine needs thread-safe access
2. **Dedicated render thread** — Simpler, just write to ring buffer

We chose (2) because:
- Ring buffer decouples render timing from PipeWire timing
- PlaybackEngine doesn't need to be `Send + Sync`
- Pre-fill buffer handles startup latency

Trade-off: Slightly higher latency (render thread → ring buffer → PipeWire). Acceptable for music playback.

## Why CAS HTTP API for Content Resolution?

Chaosgarden runs as a separate daemon. Options:

1. **Direct file access** — Requires shared filesystem paths
2. **CAS HTTP API** — Decoupled, works across network
3. **ZMQ content channel** — Complex, custom protocol

We chose (2) because:
- Hootenanny already serves `/artifact/{id}` and `/cas/{hash}`
- HTTP is simple, debuggable (curl it!)
- Caching at resolver layer is straightforward

## Why Simple Summing for Mixing?

Professional DAWs have complex mixing:
- Per-track gain, pan, EQ
- Send/return buses
- Compression, limiting

For v1, we just sum samples:
```rust
output[i] += region_a[i] + region_b[i];
```

With soft clipping to prevent harsh distortion. This is enough to prove the pipeline works. Per-track controls come later.

## Why Pre-Decode on Region Create?

Options:
1. **Pre-decode** — Load full WAV when region created
2. **Lazy decode** — Load on first render
3. **Streaming decode** — Decode in chunks during playback

We chose (1) because:
- Simplest implementation
- Fail-fast if content missing
- WAV files are typically small (a few MB)
- Memory is cheap

Streaming decode would be needed for very long files or memory-constrained systems.

## Why Luanette for Orchestration?

Options:
1. **Direct MCP calls** — Verbose, no local state
2. **Shell script** — Awkward for async jobs
3. **Luanette** — Clean syntax, async polling, local variables

Luanette gives us:
- `job:poll()` for async operations
- Local variables for intermediate results
- Loops and conditionals for complex workflows
- Readable scripts that document the process

---

## Cross-Cutting Concerns

### Error Handling

All async operations can fail. Pattern:

```lua
local result = job:poll(timeout)
if not result then
    error("Job timed out: " .. job.id)
end
if result.error then
    error("Job failed: " .. result.error)
end
```

### Content Lifetime

WAV data lives in:
1. CAS (persistent)
2. ContentResolver cache (memory, session lifetime)
3. Region's `audio` field (Arc, shared)

Cache eviction TBD for long-running sessions.

### Sample Rate

**Session sample rate is configurable** (default 48kHz, can use 96kHz for higher quality).

Set via `DaemonConfig::sample_rate` or `PipeWireOutputConfig::sample_rate`.

All content in a session must match the configured rate:
- v1: Fail with clear error at decode time if mismatch
- Future: Resample at decode time using `rubato` crate

MIDI→WAV conversion should use the session's configured rate:
```lua
-- In Lua workflow, get rate from config or use constant
local SAMPLE_RATE = 48000  -- or 96000 for high-quality sessions
convert.midi_to_wav(midi_hash, soundfont_hash, SAMPLE_RATE)
```

**Why not always 96kHz?** Memory and CPU cost doubles. For AI-generated playback, 48kHz is transparent. Use 96kHz when quality matters (final renders, archival).

---

## Open Questions

| Question | Context | Status |
|----------|---------|--------|
| Loop points | How to loop a section? | Defer — manual seek for now |
| Tempo sync | What if MIDI tempo != timeline tempo? | Defer — assume match |
| Multi-soundfont | Different instruments from different SF2? | Defer — single soundfont for now |

---

## Rejected Alternatives

| Alternative | Why Rejected |
|-------------|--------------|
| JACK audio | Requires separate server, not default on modern Linux |
| Direct ALSA | Too low-level, no session management |
| Render in PipeWire callback | Threading complexity with PlaybackEngine |
| Stream WAV during playback | Complexity not justified for small files |
| Complex mixer for v1 | YAGNI — summing is enough to prove pipeline |
