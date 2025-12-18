# Chaosgarden RT Mixing Design

Design document for real-time audio mixing in chaosgarden.

**Status**: Phase 1 & 2 Complete ✅

---

## Problem Statement

### Original Issues (Dec 2024)

1. **Underruns/Overruns in Monitor Loopback**
   - Output: 2275 underruns, Input: 1210 overruns (observed)
   - Root cause: Lock contention between input/output RT threads

2. **tick() was in Audio Data Path**
   - tick() runs at 1ms in tokio async (NOT realtime)
   - PipeWire callbacks run at buffer rate (e.g., 512 samples = 10.7ms at 48kHz)
   - Lock contention ~30% when tick() tried to mix audio

### The Fundamental Insight

**tick() should NOT be in the audio data path.**

```
BROKEN:
PipeWire Input → Ring → tick() → Ring → PipeWire Output
                        ↑
                   lock contention!

CORRECT:
PipeWire Input → SPSC Ring ────────────────┐
                  (lock-free)              ↓
PipeWire Output ← ─────────────── RT Mixer (in output callback)
```

---

## Architecture

### Separation of Concerns

| Plane | Thread | Responsibility | Timing |
|-------|--------|----------------|--------|
| **Control** | tokio async | Transport, regions, parameters | 1ms tick |
| **Data** | PipeWire RT | Audio mixing, I/O | Buffer-driven |

### Lock-Free Communication

Using `rtrb` crate for SPSC (Single-Producer Single-Consumer) ring buffer:

```rust
// external_io.rs
pub struct AudioRingProducer {
    inner: rtrb::Producer<f32>,
}

pub struct AudioRingConsumer {
    inner: rtrb::Consumer<f32>,
}

pub fn audio_ring_pair(capacity: usize) -> (AudioRingProducer, AudioRingConsumer) {
    let (producer, consumer) = rtrb::RingBuffer::new(capacity);
    (AudioRingProducer { inner: producer }, AudioRingConsumer { inner: consumer })
}
```

Key properties:
- **Wait-free writes**: Producer never blocks
- **Wait-free reads**: Consumer never blocks
- **No locks**: SPSC pattern eliminates contention
- **RT-safe**: No allocations, no syscalls in hot path

### Output Callback is Master

The PipeWire output callback does the mixing:

```rust
// In output callback (RT thread):
if monitor.enabled.load(Ordering::Relaxed) {
    let read = monitor.consumer.read(&mut temp_buffer);
    let gain = monitor.gain.load(Ordering::Relaxed);

    for i in 0..read {
        output_buffer[i] += temp_buffer[i] * gain;
    }
}
```

---

## Data Flow

```
┌─────────────────────────────────────────┐
│         CONTROL PLANE (tick)            │
│                                         │
│  Parameters via atomics:                │
│  - monitor.enabled (AtomicBool)         │
│  - monitor.gain (AtomicF32)             │
│  - master_gain (AtomicF32)              │
└─────────────────────────────────────────┘
             │
═════════════════════════════════════════
             │
┌────────────┼────────────────────────────┐
│   DATA PLANE (RT)                       │
│            │                            │
│            ▼ (atomics only)             │
│  ┌─────────────┐    ┌───────────────┐   │
│  │ Input Ring  │───►│   RT Mixer    │───────► Output
│  │ (SPSC)      │    │ (in callback) │   │
│  └─────────────┘    └───────────────┘   │
│       ▲                                 │
└───────┼─────────────────────────────────┘
        │
   PipeWire Input
```

---

## Implementation Details

### File Structure

| File | Purpose |
|------|---------|
| `monitor_input.rs` | PipeWire input stream, writes to `AudioRingProducer` |
| `pipewire_output.rs` | PipeWire output stream, reads from `AudioRingConsumer` |
| `external_io.rs` | Ring buffer wrapper types |
| `daemon.rs` | Lifecycle management, ring pair creation |

### Ring Buffer Sizing

```rust
// Half second of audio at sample_rate, stereo
let ring_capacity = (sample_rate as usize) * (channels as usize) / 2;
```

For 48kHz stereo: 48,000 samples = 500ms buffer.

### Warmup Logic

Ignore startup transients:

```rust
// Input: only count overruns after first complete write
if written < n_samples {
    if stats.warmed_up.load(Ordering::Relaxed) {
        stats.overruns.fetch_add(1, Ordering::Relaxed);
    }
} else {
    stats.warmed_up.store(true, Ordering::Relaxed);
}

// Output: only count underruns after first successful read
if read > 0 {
    stats.warmed_up.store(true, Ordering::Relaxed);
}
```

---

## Results

### 28-Hour Stress Test (Dec 2024)

| Metric | Value |
|--------|-------|
| **Callbacks** | 9,430,355 |
| **Underruns** | **0** |
| **Overruns** | **0** |
| **Monitor Read Success** | 99.99993% |
| **Samples Processed** | 4.8 billion |
| **Memory** | 13 MB |
| **CPU** | 2.6% |

Before lock-free implementation: ~30% lock failures.
After: 0% lock failures.

---

## Future Work

### Phase 3: Wire PlaybackEngine to Timeline Ring

- Add timeline ring buffer for PlaybackEngine output
- Mix timeline audio with monitor input in output callback

### Phase 4: Multiple Concurrent Inputs

- Support N inputs with independent gain/enable
- `MixerState` struct to manage multiple input streams
- MCP tools: `garden_add_mixer_input`, `garden_remove_mixer_input`

### Phase 5: Per-Input Controls

- Pan, mute, solo per input
- Effects sends

---

## Key Learnings

1. **Lock contention is death for RT audio** - Even `try_lock()` at ~30% failure rate causes audible artifacts

2. **SPSC is perfect for audio I/O** - One producer (input callback), one consumer (output callback) - natural fit

3. **Warmup matters** - Initial buffer fill creates false errors; ignore startup transients

4. **Memory is cheap, latency is expensive** - 500ms ring buffer costs 192KB but ensures zero underruns

5. **Don't fight the RT thread** - Let PipeWire drive timing, use atomics for control
