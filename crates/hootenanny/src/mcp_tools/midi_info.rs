//! MIDI file information extraction using midly
//!
//! Extracts tempo, time signature, duration, and other metadata from MIDI files.

use anyhow::{Context, Result};
use midly::{MetaMessage, Smf, TrackEventKind};
use serde::Serialize;

/// Tempo change event at a specific tick position
#[derive(Debug, Clone, Serialize)]
pub struct TempoChange {
    /// Tick position where tempo changes
    pub tick: u32,
    /// Tempo in BPM
    pub bpm: f64,
}

/// MIDI file information
#[derive(Debug, Clone, Serialize)]
pub struct MidiInfo {
    /// Primary tempo in BPM (from first SetTempo event, if any)
    pub tempo_bpm: Option<f64>,

    /// All tempo changes in the file
    pub tempo_changes: Vec<TempoChange>,

    /// Time signature as (numerator, denominator), e.g. (4, 4)
    pub time_signature: Option<(u8, u8)>,

    /// Duration in seconds (calculated from ticks and tempo)
    pub duration_seconds: f64,

    /// Number of tracks
    pub track_count: usize,

    /// Pulses per quarter note (ticks per beat)
    pub ppq: u16,

    /// Total number of note-on events
    pub note_count: usize,

    /// MIDI format (0, 1, or 2)
    pub format: u8,
}

/// Extract information from MIDI file bytes
pub fn extract_midi_info(midi_bytes: &[u8]) -> Result<MidiInfo> {
    let smf = Smf::parse(midi_bytes).context("Failed to parse MIDI file")?;

    let ppq = match smf.header.timing {
        midly::Timing::Metrical(ticks) => ticks.as_int(),
        midly::Timing::Timecode(_fps, _tpf) => {
            // For SMPTE timing, use a reasonable default
            // This is less common in music MIDI files
            480
        }
    };

    let format = match smf.header.format {
        midly::Format::SingleTrack => 0,
        midly::Format::Parallel => 1,
        midly::Format::Sequential => 2,
    };

    let mut tempo_changes: Vec<TempoChange> = Vec::new();
    let mut time_signature: Option<(u8, u8)> = None;
    let mut note_count: usize = 0;
    let mut max_tick: u32 = 0;

    // Process all tracks
    for track in &smf.tracks {
        let mut current_tick: u32 = 0;

        for event in track {
            current_tick += event.delta.as_int();

            match event.kind {
                TrackEventKind::Meta(MetaMessage::Tempo(tempo)) => {
                    // Convert microseconds per beat to BPM
                    let microseconds_per_beat = tempo.as_int();
                    let bpm = 60_000_000.0 / microseconds_per_beat as f64;
                    tempo_changes.push(TempoChange {
                        tick: current_tick,
                        bpm,
                    });
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(num, denom_pow, _, _)) => {
                    // denom_pow is log2 of denominator (e.g., 2 = quarter note)
                    let denominator = 1u8 << denom_pow;
                    if time_signature.is_none() {
                        time_signature = Some((num, denominator));
                    }
                }
                TrackEventKind::Midi {
                    message: midly::MidiMessage::NoteOn { .. },
                    ..
                } => {
                    note_count += 1;
                }
                _ => {}
            }

            max_tick = max_tick.max(current_tick);
        }
    }

    // Sort tempo changes by tick (they should already be, but let's be safe)
    tempo_changes.sort_by_key(|t| t.tick);

    // Calculate duration in seconds
    let duration_seconds = ticks_to_seconds(max_tick, ppq, &tempo_changes);

    // Get primary tempo (first tempo event, or None if no tempo events)
    let tempo_bpm = tempo_changes.first().map(|t| t.bpm);

    Ok(MidiInfo {
        tempo_bpm,
        tempo_changes,
        time_signature,
        duration_seconds,
        track_count: smf.tracks.len(),
        ppq,
        note_count,
        format,
    })
}

/// Convert ticks to seconds, accounting for tempo changes
fn ticks_to_seconds(total_ticks: u32, ppq: u16, tempo_changes: &[TempoChange]) -> f64 {
    if tempo_changes.is_empty() {
        // Default 120 BPM if no tempo specified
        let default_bpm = 120.0;
        let seconds_per_tick = 60.0 / (default_bpm * ppq as f64);
        return total_ticks as f64 * seconds_per_tick;
    }

    let mut seconds = 0.0;
    let mut last_tick = 0u32;
    let mut current_bpm = tempo_changes[0].bpm;

    for tempo in tempo_changes {
        if tempo.tick > last_tick {
            // Add time for the segment at current tempo
            let delta_ticks = tempo.tick - last_tick;
            let seconds_per_tick = 60.0 / (current_bpm * ppq as f64);
            seconds += delta_ticks as f64 * seconds_per_tick;
        }
        last_tick = tempo.tick;
        current_bpm = tempo.bpm;
    }

    // Add remaining ticks after last tempo change
    if total_ticks > last_tick {
        let delta_ticks = total_ticks - last_tick;
        let seconds_per_tick = 60.0 / (current_bpm * ppq as f64);
        seconds += delta_ticks as f64 * seconds_per_tick;
    }

    seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ticks_to_seconds_default_tempo() {
        // 480 PPQ, 480 ticks = 1 beat at 120 BPM = 0.5 seconds
        let seconds = ticks_to_seconds(480, 480, &[]);
        assert!((seconds - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_ticks_to_seconds_with_tempo() {
        // 480 PPQ, 480 ticks = 1 beat at 60 BPM = 1.0 second
        let tempo = vec![TempoChange { tick: 0, bpm: 60.0 }];
        let seconds = ticks_to_seconds(480, 480, &tempo);
        assert!((seconds - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_ticks_to_seconds_tempo_change() {
        // 480 PPQ
        // First 480 ticks at 120 BPM = 0.5 seconds
        // Next 480 ticks at 60 BPM = 1.0 second
        // Total = 1.5 seconds
        let tempo = vec![
            TempoChange { tick: 0, bpm: 120.0 },
            TempoChange {
                tick: 480,
                bpm: 60.0,
            },
        ];
        let seconds = ticks_to_seconds(960, 480, &tempo);
        assert!((seconds - 1.5).abs() < 0.001);
    }
}
