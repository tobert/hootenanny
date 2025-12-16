# 01: Wire PipeWire to Daemon

**File:** `crates/chaosgarden/src/daemon.rs`, `crates/chaosgarden/src/bin/chaosgarden.rs`
**Focus:** Connect PipeWireOutputStream to PlaybackEngine output
**Dependencies:** None (pipewire_output.rs already exists)
**Unblocks:** 03-playback-mixing

---

## Task

Wire the existing `PipeWireOutputStream` to the daemon so that `PlaybackEngine::render()` output goes to actual audio hardware.

**Why this first?** Without audio output, we can't verify anything works. This is the final link in the chain.

**Deliverables:**
1. Daemon creates `PipeWireOutputStream` on startup (when `--pipewire` flag or feature enabled)
2. Render loop pulls from `PlaybackEngine` and pushes to PipeWire ring buffer
3. Stats accessible for monitoring

**Definition of Done:**
```bash
cargo build -p chaosgarden --features pipewire
cargo test -p chaosgarden --features pipewire
# Manual: run daemon, create region with WAV artifact, hear audio
```

## Out of Scope

- Content resolution (artifact → samples) — that's task 02
- Multi-region mixing — that's task 03

## Testing Without Content

For this task, test with a simple tone generator (like `pipewire_tone` example):
```rust
// In render loop, if no regions active, generate test tone
if engine.active_regions().count() == 0 {
    generate_sine_tone(&mut buffer, 440.0, sample_rate, &mut phase);
}
```

This verifies the PipeWire wiring works before content resolution is ready.

---

## Existing Code

### PipeWireOutputStream (already built)

```rust
// crates/chaosgarden/src/pipewire_output.rs
pub struct PipeWireOutputStream { ... }

impl PipeWireOutputStream {
    pub fn new_paused(config: PipeWireOutputConfig) -> Result<Self, PipeWireOutputError>;
    pub fn start(&mut self) -> Result<(), PipeWireOutputError>;
    pub fn ring_buffer(&self) -> Arc<Mutex<RingBuffer>>;
    pub fn stats(&self) -> &Arc<StreamStats>;
    pub fn is_running(&self) -> bool;
    pub fn stop(&mut self);
}
```

### PlaybackEngine

```rust
// crates/chaosgarden/src/playback.rs
impl PlaybackEngine {
    pub fn render(&mut self, output: &mut [f32], sample_rate: u32);
}
```

### GardenDaemon

```rust
// crates/chaosgarden/src/daemon.rs
pub struct GardenDaemon {
    // ... existing fields
    // ADD: pipewire output
}
```

---

## Integration Pattern

The daemon needs a render thread that:
1. Calls `PlaybackEngine::render()` to fill a buffer
2. Writes that buffer to `PipeWireOutputStream::ring_buffer()`

Two approaches:

### Option A: Dedicated render thread (recommended)
```rust
// In daemon startup
let pw_stream = PipeWireOutputStream::new_paused(config)?;
let ring = pw_stream.ring_buffer();
pw_stream.start()?;

// Spawn render thread
std::thread::spawn(move || {
    let mut buffer = vec![0.0f32; 1024]; // ~10ms @ 48kHz stereo
    loop {
        engine.render(&mut buffer, 48000);
        if let Ok(mut ring) = ring.lock() {
            ring.write(&buffer);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
});
```

### Option B: Render in PipeWire callback
More complex — requires sharing PlaybackEngine across threads safely.

---

## DaemonConfig Extension

```rust
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    // ... existing
    #[cfg(feature = "pipewire")]
    pub pipewire: Option<PipeWireOutputConfig>,
}
```

---

## Acceptance Criteria

- [ ] `GardenDaemon` accepts optional PipeWire config
- [ ] When enabled, audio output goes to PipeWire
- [ ] `StreamStats` accessible via some mechanism (logs, status query)
- [ ] Graceful shutdown stops PipeWire thread
- [ ] Works with `--features pipewire`, compiles without
