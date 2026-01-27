//! MIDI file parsing for playback
//!
//! Extracts MIDI events from files for real-time playback to external hardware.
//! Converts from midly's event types to our MidiMessage format.

use anyhow::{Context, Result};
use midly::{MetaMessage, MidiMessage as MidlyMessage, Smf, TrackEventKind};

use crate::primitives::MidiMessage;

/// A MIDI event at a specific tick position
#[derive(Debug, Clone)]
pub struct MidiFileEvent {
    /// Absolute tick position in the file
    pub tick: u64,
    /// MIDI channel (0-15)
    pub channel: u8,
    /// The MIDI message
    pub message: MidiMessage,
}

/// Tempo change at a specific tick
#[derive(Debug, Clone)]
pub struct TempoChange {
    /// Tick position where tempo changes
    pub tick: u64,
    /// Tempo in BPM
    pub bpm: f64,
}

/// Parsed MIDI file ready for playback
#[derive(Debug, Clone)]
pub struct ParsedMidiFile {
    /// Pulses per quarter note (ticks per beat)
    pub ppq: u16,
    /// Tempo changes throughout the file
    pub tempo_changes: Vec<TempoChange>,
    /// All MIDI events, sorted by tick
    pub events: Vec<MidiFileEvent>,
    /// Duration in ticks
    pub duration_ticks: u64,
}

impl ParsedMidiFile {
    /// Get duration in beats
    pub fn duration_beats(&self) -> f64 {
        self.duration_ticks as f64 / self.ppq as f64
    }

    /// Convert a tick position to beats
    pub fn tick_to_beat(&self, tick: u64) -> f64 {
        tick as f64 / self.ppq as f64
    }

    /// Convert a beat position to ticks
    pub fn beat_to_tick(&self, beat: f64) -> u64 {
        (beat * self.ppq as f64).round() as u64
    }

    /// Get events in a tick range (inclusive start, exclusive end)
    pub fn events_in_range(&self, start_tick: u64, end_tick: u64) -> impl Iterator<Item = &MidiFileEvent> {
        self.events
            .iter()
            .filter(move |e| e.tick >= start_tick && e.tick < end_tick)
    }

    /// Get the tempo at a specific tick
    pub fn tempo_at(&self, tick: u64) -> f64 {
        self.tempo_changes
            .iter()
            .rev()
            .find(|tc| tc.tick <= tick)
            .map(|tc| tc.bpm)
            .unwrap_or(120.0)
    }
}

/// Convert midly MidiMessage to our MidiMessage format
fn convert_midly_message(channel: u8, message: MidlyMessage) -> Option<MidiMessage> {
    match message {
        MidlyMessage::NoteOn { key, vel } => {
            if vel.as_int() == 0 {
                // Note On with velocity 0 is Note Off
                Some(MidiMessage::NoteOff {
                    channel,
                    pitch: key.as_int(),
                })
            } else {
                Some(MidiMessage::NoteOn {
                    channel,
                    pitch: key.as_int(),
                    velocity: vel.as_int(),
                })
            }
        }
        MidlyMessage::NoteOff { key, .. } => Some(MidiMessage::NoteOff {
            channel,
            pitch: key.as_int(),
        }),
        MidlyMessage::Aftertouch { key: _, vel } => {
            // Polyphonic aftertouch - map to control change for now
            // Could add proper Aftertouch variant to MidiMessage if needed
            Some(MidiMessage::ControlChange {
                channel,
                controller: 102, // NRPN-ish placeholder
                value: vel.as_int(),
            })
        }
        MidlyMessage::Controller { controller, value } => Some(MidiMessage::ControlChange {
            channel,
            controller: controller.as_int(),
            value: value.as_int(),
        }),
        MidlyMessage::ProgramChange { program } => Some(MidiMessage::ProgramChange {
            channel,
            program: program.as_int(),
        }),
        MidlyMessage::ChannelAftertouch { vel } => {
            // Channel aftertouch - map to control change for now
            Some(MidiMessage::ControlChange {
                channel,
                controller: 103, // NRPN-ish placeholder
                value: vel.as_int(),
            })
        }
        MidlyMessage::PitchBend { bend } => {
            // midly's PitchBend is 14-bit centered at 8192
            let value = bend.as_int() as i16 - 8192;
            Some(MidiMessage::PitchBend { channel, value })
        }
    }
}

/// Parse a MIDI file from bytes
///
/// Extracts all note events and tempo changes, sorted by tick position.
/// Returns a ParsedMidiFile ready for playback.
pub fn parse_midi_file(bytes: &[u8]) -> Result<ParsedMidiFile> {
    let smf = Smf::parse(bytes).context("Failed to parse MIDI file")?;

    let ppq = match smf.header.timing {
        midly::Timing::Metrical(t) => t.as_int(),
        midly::Timing::Timecode(_fps, _tpf) => {
            // SMPTE timing - use reasonable default
            480
        }
    };

    let mut events = Vec::new();
    let mut tempo_changes = vec![TempoChange { tick: 0, bpm: 120.0 }];
    let mut max_tick = 0u64;

    // Process all tracks
    for track in &smf.tracks {
        let mut tick = 0u64;

        for event in track {
            tick += event.delta.as_int() as u64;

            match event.kind {
                TrackEventKind::Midi { channel, message } => {
                    if let Some(msg) = convert_midly_message(channel.as_int(), message) {
                        events.push(MidiFileEvent {
                            tick,
                            channel: channel.as_int(),
                            message: msg,
                        });
                    }
                }
                TrackEventKind::Meta(MetaMessage::Tempo(tempo)) => {
                    // Convert microseconds per beat to BPM
                    let bpm = 60_000_000.0 / tempo.as_int() as f64;
                    tempo_changes.push(TempoChange { tick, bpm });
                }
                _ => {}
            }

            max_tick = max_tick.max(tick);
        }
    }

    // Sort events by tick (tracks may interleave)
    events.sort_by_key(|e| e.tick);

    // Sort tempo changes by tick
    tempo_changes.sort_by_key(|t| t.tick);

    Ok(ParsedMidiFile {
        ppq,
        tempo_changes,
        events,
        duration_ticks: max_tick,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_beat_conversion() {
        let parsed = ParsedMidiFile {
            ppq: 480,
            tempo_changes: vec![TempoChange { tick: 0, bpm: 120.0 }],
            events: vec![],
            duration_ticks: 1920,
        };

        assert_eq!(parsed.tick_to_beat(480), 1.0);
        assert_eq!(parsed.tick_to_beat(960), 2.0);
        assert_eq!(parsed.beat_to_tick(1.0), 480);
        assert_eq!(parsed.beat_to_tick(2.5), 1200);
        assert_eq!(parsed.duration_beats(), 4.0);
    }

    #[test]
    fn test_tempo_at() {
        let parsed = ParsedMidiFile {
            ppq: 480,
            tempo_changes: vec![
                TempoChange { tick: 0, bpm: 120.0 },
                TempoChange { tick: 960, bpm: 140.0 },
                TempoChange { tick: 1920, bpm: 100.0 },
            ],
            events: vec![],
            duration_ticks: 2880,
        };

        assert_eq!(parsed.tempo_at(0), 120.0);
        assert_eq!(parsed.tempo_at(480), 120.0);
        assert_eq!(parsed.tempo_at(960), 140.0);
        assert_eq!(parsed.tempo_at(1440), 140.0);
        assert_eq!(parsed.tempo_at(1920), 100.0);
        assert_eq!(parsed.tempo_at(2400), 100.0);
    }

    #[test]
    fn test_events_in_range() {
        let parsed = ParsedMidiFile {
            ppq: 480,
            tempo_changes: vec![TempoChange { tick: 0, bpm: 120.0 }],
            events: vec![
                MidiFileEvent {
                    tick: 0,
                    channel: 0,
                    message: MidiMessage::NoteOn { channel: 0, pitch: 60, velocity: 100 },
                },
                MidiFileEvent {
                    tick: 480,
                    channel: 0,
                    message: MidiMessage::NoteOff { channel: 0, pitch: 60 },
                },
                MidiFileEvent {
                    tick: 480,
                    channel: 0,
                    message: MidiMessage::NoteOn { channel: 0, pitch: 64, velocity: 100 },
                },
                MidiFileEvent {
                    tick: 960,
                    channel: 0,
                    message: MidiMessage::NoteOff { channel: 0, pitch: 64 },
                },
            ],
            duration_ticks: 960,
        };

        let events: Vec<_> = parsed.events_in_range(0, 480).collect();
        assert_eq!(events.len(), 1);

        let events: Vec<_> = parsed.events_in_range(480, 960).collect();
        assert_eq!(events.len(), 2);

        let events: Vec<_> = parsed.events_in_range(0, 1000).collect();
        assert_eq!(events.len(), 4);
    }
}
