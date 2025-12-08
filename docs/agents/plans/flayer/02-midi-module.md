# Task 02: MIDI Module

**Priority:** High
**Estimated Sessions:** 2-3
**Depends On:** 01-core-structs

---

## Objective

Create the MIDI manipulation module wrapping `midly`. All transformations are immutable - they return new `Sequence` objects, preserving the original.

## Files to Create/Modify

### Add to `crates/flayer/Cargo.toml`

```toml
[dependencies]
midly = "0.5"
```

### Create `crates/flayer/src/midi.rs`

```rust
use anyhow::{anyhow, Context, Result};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, Track as MidlyTrack, TrackEvent, TrackEventKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A parsed MIDI sequence with absolute timing and tempo map
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sequence {
    /// Initial tempo (may be overridden by tempo_map)
    pub bpm: f64,
    pub time_sig: (u8, u8),
    pub ppq: u16,
    pub tracks: Vec<MidiTrack>,

    /// Tempo changes throughout the sequence
    /// Essential for accurate time→tick and tick→time conversion
    pub tempo_map: TempoMap,
}

/// Tempo map for sequences with tempo changes
///
/// Music generation models often output variable tempo. Proper handling
/// requires tracking tempo changes for accurate time conversion.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TempoMap {
    /// Tempo changes: (tick, bpm)
    /// First entry should be at tick 0
    pub changes: Vec<TempoChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoChange {
    pub tick: u64,
    pub bpm: f64,
}

impl TempoMap {
    pub fn new(initial_bpm: f64) -> Self {
        Self {
            changes: vec![TempoChange { tick: 0, bpm: initial_bpm }],
        }
    }

    pub fn add_change(&mut self, tick: u64, bpm: f64) {
        // Insert in sorted order
        let pos = self.changes.iter().position(|c| c.tick > tick).unwrap_or(self.changes.len());
        self.changes.insert(pos, TempoChange { tick, bpm });
    }

    /// Get tempo at a given tick
    pub fn tempo_at(&self, tick: u64) -> f64 {
        self.changes.iter()
            .rev()
            .find(|c| c.tick <= tick)
            .map(|c| c.bpm)
            .unwrap_or(120.0)
    }

    /// Convert ticks to seconds, accounting for tempo changes
    pub fn ticks_to_seconds(&self, ticks: u64, ppq: u16) -> f64 {
        let mut seconds = 0.0;
        let mut current_tick: u64 = 0;

        for i in 0..self.changes.len() {
            let change = &self.changes[i];
            let next_tick = if i + 1 < self.changes.len() {
                self.changes[i + 1].tick
            } else {
                ticks
            };

            if current_tick >= ticks {
                break;
            }

            let segment_end = next_tick.min(ticks);
            let segment_ticks = segment_end.saturating_sub(current_tick.max(change.tick));

            // seconds = ticks / ppq / (bpm / 60)
            let segment_seconds = segment_ticks as f64 / ppq as f64 * 60.0 / change.bpm;
            seconds += segment_seconds;

            current_tick = segment_end;
        }

        seconds
    }

    /// Convert seconds to ticks, accounting for tempo changes
    pub fn seconds_to_ticks(&self, seconds: f64, ppq: u16) -> u64 {
        let mut remaining_seconds = seconds;
        let mut current_tick: u64 = 0;

        for i in 0..self.changes.len() {
            let change = &self.changes[i];
            let next_tick = if i + 1 < self.changes.len() {
                self.changes[i + 1].tick
            } else {
                u64::MAX
            };

            // How many seconds does this tempo segment span?
            let segment_ticks = next_tick.saturating_sub(change.tick);
            let segment_seconds = segment_ticks as f64 / ppq as f64 * 60.0 / change.bpm;

            if remaining_seconds <= segment_seconds {
                // Target is within this segment
                let ticks_in_segment = (remaining_seconds * ppq as f64 * change.bpm / 60.0) as u64;
                return current_tick + ticks_in_segment;
            }

            remaining_seconds -= segment_seconds;
            current_tick = next_tick;
        }

        current_tick
    }

    /// Get average tempo across the sequence
    pub fn average_tempo(&self, total_ticks: u64, ppq: u16) -> f64 {
        if self.changes.is_empty() {
            return 120.0;
        }

        let total_seconds = self.ticks_to_seconds(total_ticks, ppq);
        let total_beats = total_ticks as f64 / ppq as f64;

        if total_seconds > 0.0 {
            total_beats * 60.0 / total_seconds
        } else {
            self.changes[0].bpm
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiTrack {
    pub name: Option<String>,
    pub channel: Option<u8>,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub tick: u64,          // Absolute tick position
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventKind {
    NoteOn { channel: u8, pitch: u8, velocity: u8 },
    NoteOff { channel: u8, pitch: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: i16 },
    Tempo(f64),             // BPM
    TimeSignature(u8, u8),  // num, denom
}

impl Sequence {
    // ==================== LOADING ====================

    /// Parse from SMF bytes
    pub fn from_smf(bytes: &[u8]) -> Result<Self> {
        let smf = Smf::parse(bytes).context("Failed to parse MIDI file")?;

        let ppq = match smf.header.timing {
            Timing::Metrical(ticks) => ticks.as_int(),
            Timing::Timecode(_, _) => return Err(anyhow!("Timecode timing not supported")),
        };

        let mut bpm = 120.0;
        let mut time_sig = (4u8, 4u8);
        let mut tracks = Vec::new();
        let mut tempo_map = TempoMap::new(120.0);

        for midly_track in smf.tracks.iter() {
            let mut track = MidiTrack {
                name: None,
                channel: None,
                events: Vec::new(),
            };

            let mut abs_tick: u64 = 0;

            for event in midly_track.iter() {
                abs_tick += event.delta.as_int() as u64;

                match event.kind {
                    TrackEventKind::Meta(MetaMessage::TrackName(name)) => {
                        track.name = Some(String::from_utf8_lossy(name).to_string());
                    }
                    TrackEventKind::Meta(MetaMessage::Tempo(t)) => {
                        bpm = 60_000_000.0 / t.as_int() as f64;
                        // Add to tempo map for accurate time conversion
                        tempo_map.add_change(abs_tick, bpm);
                        track.events.push(Event {
                            tick: abs_tick,
                            kind: EventKind::Tempo(bpm),
                        });
                    }
                    TrackEventKind::Meta(MetaMessage::TimeSignature(num, denom, _, _)) => {
                        time_sig = (num, 2u8.pow(denom as u32));
                        track.events.push(Event {
                            tick: abs_tick,
                            kind: EventKind::TimeSignature(num, 2u8.pow(denom as u32)),
                        });
                    }
                    TrackEventKind::Midi { channel, message } => {
                        let ch = channel.as_int();
                        if track.channel.is_none() {
                            track.channel = Some(ch);
                        }

                        let kind = match message {
                            // Note: vel is u7 type, need as_int() for comparison
                            MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                                EventKind::NoteOn { channel: ch, pitch: key.as_int(), velocity: vel.as_int() }
                            }
                            MidiMessage::NoteOn { key, .. } => {
                                // NoteOn with velocity 0 = NoteOff
                                EventKind::NoteOff { channel: ch, pitch: key.as_int() }
                            }
                            MidiMessage::NoteOff { key, .. } => {
                                EventKind::NoteOff { channel: ch, pitch: key.as_int() }
                            }
                            MidiMessage::Controller { controller, value } => {
                                EventKind::ControlChange { channel: ch, controller: controller.as_int(), value: value.as_int() }
                            }
                            MidiMessage::ProgramChange { program } => {
                                EventKind::ProgramChange { channel: ch, program: program.as_int() }
                            }
                            MidiMessage::PitchBend { bend } => {
                                EventKind::PitchBend { channel: ch, value: bend.as_int() }
                            }
                            _ => continue,
                        };

                        track.events.push(Event { tick: abs_tick, kind });
                    }
                    _ => {}
                }
            }

            if !track.events.is_empty() {
                tracks.push(track);
            }
        }

        Ok(Self { bpm, time_sig, ppq, tracks, tempo_map })
    }

    // ==================== TRANSFORMATIONS (IMMUTABLE) ====================

    /// Extract events from a single channel
    pub fn extract_channel(&self, ch: u8) -> Self {
        let tracks = self.tracks.iter()
            .map(|t| MidiTrack {
                name: t.name.clone(),
                channel: Some(ch),
                events: t.events.iter()
                    .filter(|e| match &e.kind {
                        EventKind::NoteOn { channel, .. } => *channel == ch,
                        EventKind::NoteOff { channel, .. } => *channel == ch,
                        EventKind::ControlChange { channel, .. } => *channel == ch,
                        EventKind::ProgramChange { channel, .. } => *channel == ch,
                        EventKind::PitchBend { channel, .. } => *channel == ch,
                        EventKind::Tempo(_) | EventKind::TimeSignature(_, _) => true,
                    })
                    .cloned()
                    .collect(),
            })
            .filter(|t| !t.events.is_empty())
            .collect();

        Self { tracks, ..self.clone() }
    }

    /// Extract events from multiple channels
    pub fn extract_channels(&self, channels: &[u8]) -> Self {
        let tracks = self.tracks.iter()
            .map(|t| MidiTrack {
                name: t.name.clone(),
                channel: t.channel,
                events: t.events.iter()
                    .filter(|e| match &e.kind {
                        EventKind::NoteOn { channel, .. } => channels.contains(channel),
                        EventKind::NoteOff { channel, .. } => channels.contains(channel),
                        EventKind::ControlChange { channel, .. } => channels.contains(channel),
                        EventKind::ProgramChange { channel, .. } => channels.contains(channel),
                        EventKind::PitchBend { channel, .. } => channels.contains(channel),
                        EventKind::Tempo(_) | EventKind::TimeSignature(_, _) => true,
                    })
                    .cloned()
                    .collect(),
            })
            .filter(|t| !t.events.is_empty())
            .collect();

        Self { tracks, ..self.clone() }
    }

    /// Filter notes by pitch range
    pub fn filter_notes(&self, min_pitch: u8, max_pitch: u8) -> Self {
        let tracks = self.tracks.iter()
            .map(|t| MidiTrack {
                name: t.name.clone(),
                channel: t.channel,
                events: t.events.iter()
                    .filter(|e| match &e.kind {
                        EventKind::NoteOn { pitch, .. } => *pitch >= min_pitch && *pitch <= max_pitch,
                        EventKind::NoteOff { pitch, .. } => *pitch >= min_pitch && *pitch <= max_pitch,
                        _ => true,
                    })
                    .cloned()
                    .collect(),
            })
            .collect();

        Self { tracks, ..self.clone() }
    }

    /// Filter by minimum velocity
    pub fn filter_velocity(&self, min_vel: u8) -> Self {
        let tracks = self.tracks.iter()
            .map(|t| MidiTrack {
                name: t.name.clone(),
                channel: t.channel,
                events: t.events.iter()
                    .filter(|e| match &e.kind {
                        EventKind::NoteOn { velocity, .. } => *velocity >= min_vel,
                        _ => true,
                    })
                    .cloned()
                    .collect(),
            })
            .collect();

        Self { tracks, ..self.clone() }
    }

    /// Transpose all notes by semitones
    pub fn transpose(&self, semitones: i8) -> Self {
        let tracks = self.tracks.iter()
            .map(|t| MidiTrack {
                name: t.name.clone(),
                channel: t.channel,
                events: t.events.iter()
                    .map(|e| Event {
                        tick: e.tick,
                        kind: match &e.kind {
                            EventKind::NoteOn { channel, pitch, velocity } => {
                                let new_pitch = (*pitch as i16 + semitones as i16).clamp(0, 127) as u8;
                                EventKind::NoteOn { channel: *channel, pitch: new_pitch, velocity: *velocity }
                            }
                            EventKind::NoteOff { channel, pitch } => {
                                let new_pitch = (*pitch as i16 + semitones as i16).clamp(0, 127) as u8;
                                EventKind::NoteOff { channel: *channel, pitch: new_pitch }
                            }
                            other => other.clone(),
                        },
                    })
                    .collect(),
            })
            .collect();

        Self { tracks, ..self.clone() }
    }

    /// Quantize to grid (e.g., "1/16", "1/8", "1/4", "1/8T" for triplets)
    pub fn quantize(&self, grid: &str) -> Self {
        let grid_ticks = self.parse_grid(grid);

        let tracks = self.tracks.iter()
            .map(|t| MidiTrack {
                name: t.name.clone(),
                channel: t.channel,
                events: t.events.iter()
                    .map(|e| Event {
                        tick: ((e.tick as f64 / grid_ticks as f64).round() * grid_ticks as f64) as u64,
                        kind: e.kind.clone(),
                    })
                    .collect(),
            })
            .collect();

        Self { tracks, ..self.clone() }
    }

    fn parse_grid(&self, grid: &str) -> u64 {
        let ppq = self.ppq as u64;
        match grid {
            "1/4" => ppq,
            "1/8" => ppq / 2,
            "1/16" => ppq / 4,
            "1/32" => ppq / 8,
            "1/8T" => ppq / 3,      // Triplet eighth
            "1/16T" => ppq / 6,     // Triplet sixteenth
            _ => ppq / 4,           // Default to 1/16
        }
    }

    // ==================== SLICING ====================

    /// Slice into N-bar chunks
    pub fn slice_by_bars(&self, bars: u32) -> Vec<Self> {
        let ticks_per_bar = self.ppq as u64 * 4 * self.time_sig.0 as u64 / self.time_sig.1 as u64;
        let slice_ticks = ticks_per_bar * bars as u64;
        self.slice_by_ticks(slice_ticks)
    }

    /// Slice into chunks of N beats
    pub fn slice_by_beats(&self, beats: f64) -> Vec<Self> {
        let slice_ticks = (self.ppq as f64 * beats) as u64;
        self.slice_by_ticks(slice_ticks)
    }

    fn slice_by_ticks(&self, slice_ticks: u64) -> Vec<Self> {
        let total_ticks = self.duration_ticks();
        let num_slices = ((total_ticks as f64 / slice_ticks as f64).ceil() as u64).max(1);

        (0..num_slices)
            .map(|i| {
                let start = i * slice_ticks;
                let end = start + slice_ticks;
                self.trim_ticks(start, end)
            })
            .collect()
    }

    fn trim_ticks(&self, start: u64, end: u64) -> Self {
        let tracks = self.tracks.iter()
            .map(|t| MidiTrack {
                name: t.name.clone(),
                channel: t.channel,
                events: t.events.iter()
                    .filter(|e| e.tick >= start && e.tick < end)
                    .map(|e| Event {
                        tick: e.tick - start,  // Normalize to slice start
                        kind: e.kind.clone(),
                    })
                    .collect(),
            })
            .filter(|t| !t.events.is_empty())
            .collect();

        Self { tracks, ..self.clone() }
    }

    // ==================== ANALYSIS ====================

    pub fn duration_ticks(&self) -> u64 {
        self.tracks.iter()
            .flat_map(|t| t.events.iter())
            .map(|e| e.tick)
            .max()
            .unwrap_or(0)
    }

    pub fn duration_beats(&self) -> f64 {
        self.duration_ticks() as f64 / self.ppq as f64
    }

    /// Duration in seconds, accounting for tempo changes
    pub fn duration_seconds(&self) -> f64 {
        self.tempo_map.ticks_to_seconds(self.duration_ticks(), self.ppq)
    }

    /// Convert a tick position to seconds using tempo map
    pub fn tick_to_seconds(&self, tick: u64) -> f64 {
        self.tempo_map.ticks_to_seconds(tick, self.ppq)
    }

    /// Convert seconds to tick position using tempo map
    pub fn seconds_to_tick(&self, seconds: f64) -> u64 {
        self.tempo_map.seconds_to_ticks(seconds, self.ppq)
    }

    /// Get average tempo across the sequence
    pub fn average_tempo(&self) -> f64 {
        self.tempo_map.average_tempo(self.duration_ticks(), self.ppq)
    }

    pub fn note_range(&self) -> (u8, u8) {
        let pitches: Vec<u8> = self.tracks.iter()
            .flat_map(|t| t.events.iter())
            .filter_map(|e| match &e.kind {
                EventKind::NoteOn { pitch, .. } => Some(*pitch),
                _ => None,
            })
            .collect();

        if pitches.is_empty() {
            (0, 0)
        } else {
            (*pitches.iter().min().unwrap(), *pitches.iter().max().unwrap())
        }
    }

    /// Check if sequence has tempo changes
    pub fn has_tempo_changes(&self) -> bool {
        self.tempo_map.changes.len() > 1
    }

    // ==================== EXPORT ====================

    /// Export to SMF bytes
    pub fn to_smf(&self) -> Result<Vec<u8>> {
        // Convert absolute ticks back to delta and write SMF
        // TODO: Implement full export
        todo!("SMF export not yet implemented")
    }

    /// Generate content hash for deduplication
    pub fn content_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        // Hash normalized note data (pitch + relative timing, ignore velocity)
        for track in &self.tracks {
            for event in &track.events {
                if let EventKind::NoteOn { pitch, .. } = &event.kind {
                    event.tick.hash(&mut hasher);
                    pitch.hash(&mut hasher);
                }
            }
        }
        format!("{:016x}", hasher.finish())
    }
}
```

### Update `crates/flayer/src/lib.rs`

```rust
pub mod midi;
// ... existing modules ...

pub use midi::{Sequence, MidiTrack, Event, EventKind};
```

## Acceptance Criteria

- [ ] `Sequence::from_smf()` parses standard MIDI files
- [ ] `extract_channel(10)` returns only drum events
- [ ] `quantize("1/16")` snaps events to grid
- [ ] `slice_by_bars(4)` returns correct number of slices
- [ ] `transpose(12)` shifts all notes up an octave
- [ ] All transformations return new `Sequence` (immutable)
- [ ] `content_hash()` produces stable hash for deduplication

## Tests to Write

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_drums() {
        // Load test MIDI, extract channel 10, verify only drum events
    }

    #[test]
    fn test_slice_by_bars() {
        // Create sequence, slice by 4 bars, verify slice count and content
    }

    #[test]
    fn test_quantize() {
        // Create off-grid events, quantize, verify snapped positions
    }

    #[test]
    fn test_immutability() {
        let seq = Sequence::from_smf(/* test data */)?;
        let transposed = seq.transpose(12);
        // Original should be unchanged
        assert_ne!(seq.tracks[0].events[0], transposed.tracks[0].events[0]);
    }
}
```
