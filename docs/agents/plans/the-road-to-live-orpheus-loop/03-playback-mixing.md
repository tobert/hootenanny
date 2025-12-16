# 03: Playback Mixing

**File:** `crates/chaosgarden/src/playback.rs`
**Focus:** Mix multiple concurrent regions to stereo output
**Dependencies:** 01-pipewire-daemon, 02-content-resolver
**Unblocks:** 04-lua-workflow

---

## Task

Extend `PlaybackEngine` to mix N concurrent regions into a single stereo output buffer.

**Why now?** Tasks 01 and 02 give us output and content. Now we need to combine multiple audio sources (drums + melody) into one stream.

**Deliverables:**
1. `PlaybackEngine::render()` mixes all active regions
2. Simple summing with clipping protection
3. Position tracking per region (for proper sample offset)

**Definition of Done:**
```bash
cargo check -p chaosgarden
cargo test -p chaosgarden
# Manual: create 2 regions at same position, hear both mixed
```

## Out of Scope

- Per-region gain/pan — simple summing for v1
- Crossfades between regions — future work
- Sample rate conversion — assume all content is 48kHz

---

## Current State

Check `PlaybackEngine::render()` in `playback.rs`:
- May already handle single region
- Need to extend for concurrent regions

---

## Mixing Algorithm

```rust
impl PlaybackEngine {
    pub fn render(&mut self, output: &mut [f32], sample_rate: u32) {
        // Clear output buffer
        output.fill(0.0);

        let frames = output.len() / 2; // Stereo
        let current_beat = self.position;

        // Find all active regions at current position
        for region in self.active_regions(current_beat) {
            let audio = match &region.audio {
                Some(a) => a,
                None => continue, // Skip unresolved regions
            };

            // Calculate sample offset within region
            let region_offset_beats = current_beat - region.position;
            let region_offset_samples = self.beats_to_samples(region_offset_beats, sample_rate);

            // Mix this region's samples into output
            for frame in 0..frames {
                let src_frame = region_offset_samples + frame;
                if src_frame < audio.frames() {
                    output[frame * 2] += audio.sample_at(src_frame, 0);     // L
                    output[frame * 2 + 1] += audio.sample_at(src_frame, 1); // R
                }
            }
        }

        // Soft clip to prevent harsh distortion
        for sample in output.iter_mut() {
            *sample = soft_clip(*sample);
        }

        // Advance position
        self.position += self.samples_to_beats(frames, sample_rate);
    }
}

fn soft_clip(x: f32) -> f32 {
    // Tanh-style soft clipping
    if x.abs() < 1.0 {
        x
    } else {
        x.signum() * (1.0 - (-x.abs()).exp())
    }
}
```

---

## Types

Use existing `DecodedAudio` from `nodes/audio_file.rs` and existing `Region` from `primitives/region.rs`.

```rust
// Extend existing Region or create wrapper for playback
pub struct ActiveRegion {
    pub region_id: Uuid,
    pub position: Beat,      // Start position in beats
    pub duration: Beat,      // Duration in beats
    pub audio: Option<Arc<DecodedAudio>>,  // Resolved content
}

impl PlaybackEngine {
    /// Get regions that overlap the current playback position
    fn active_regions(&self, position: Beat) -> impl Iterator<Item = &ActiveRegion>;

    /// Convert beats to sample frames at given sample rate
    fn beats_to_samples(&self, beats: Beat, sample_rate: u32) -> usize;

    /// Convert sample frames to beats
    fn samples_to_beats(&self, samples: usize, sample_rate: u32) -> Beat;
}
```

Note: `DecodedAudio::sample_at()` method added in task 02.

---

## Beat/Time Conversion

```rust
impl PlaybackEngine {
    fn beats_to_samples(&self, beats: Beat, sample_rate: u32) -> usize {
        let seconds = beats.0 * 60.0 / self.tempo;
        (seconds * sample_rate as f64) as usize
    }

    fn samples_to_beats(&self, samples: usize, sample_rate: u32) -> Beat {
        let seconds = samples as f64 / sample_rate as f64;
        Beat(seconds * self.tempo / 60.0)
    }
}
```

---

## Testing Strategy

```rust
#[test]
fn test_mix_two_regions() {
    let mut engine = PlaybackEngine::new();
    engine.set_tempo(120.0);

    // Create two regions at beat 0
    let audio1 = Arc::new(DecodedAudio::sine(440.0, 48000, 1.0)); // A4
    let audio2 = Arc::new(DecodedAudio::sine(554.0, 48000, 1.0)); // C#5

    engine.add_region(ActiveRegion {
        position: Beat(0.0),
        duration: Beat(4.0),
        audio: Some(audio1),
        ..
    });
    engine.add_region(ActiveRegion {
        position: Beat(0.0),
        duration: Beat(4.0),
        audio: Some(audio2),
        ..
    });

    let mut output = vec![0.0f32; 1024];
    engine.render(&mut output, 48000);

    // Verify output is non-zero and mixed
    assert!(output.iter().any(|&s| s != 0.0));
}
```

---

## Acceptance Criteria

- [ ] Single region plays correctly
- [ ] Two overlapping regions mix (both audible)
- [ ] Non-overlapping regions play in sequence
- [ ] Soft clipping prevents harsh distortion
- [ ] Position advances correctly with tempo
