use crate::analyze::MidiFileContext;
use crate::note::SeparatedVoice;
use serde::{Deserialize, Serialize};

/// Options for MIDI export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    /// Include tempo map track. Default: true.
    pub include_tempo_map: bool,
    /// Assign unique GM program to each voice. Default: true.
    pub assign_programs: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_tempo_map: true,
            assign_programs: true,
        }
    }
}

/// Write separated voices to Standard MIDI File format 1 bytes.
///
/// Track 0: tempo map + time signatures (from context).
/// Tracks 1+: one per voice, with track name, program change, note events.
pub fn voices_to_midi(
    voices: &[SeparatedVoice],
    context: &MidiFileContext,
    options: &ExportOptions,
) -> Vec<u8> {
    let mut tracks: Vec<Vec<u8>> = Vec::new();

    // Track 0: tempo map
    if options.include_tempo_map {
        tracks.push(build_tempo_track(context));
    }

    // Assign channels (skip 9 for drums)
    let mut channel_alloc = 0u8;

    for voice in voices {
        let channel = if channel_alloc == 9 {
            channel_alloc += 1;
            channel_alloc - 1
        } else {
            let ch = channel_alloc;
            channel_alloc += 1;
            if channel_alloc == 9 {
                channel_alloc = 10; // skip percussion channel
            }
            ch
        };
        // Cap at 15 (MIDI has 16 channels)
        let channel = channel.min(15);

        tracks.push(build_voice_track(voice, channel, options));
    }

    build_midi_file(context.ppq, &tracks)
}

/// Build the tempo/time-signature track.
fn build_tempo_track(context: &MidiFileContext) -> Vec<u8> {
    let mut events: Vec<(u64, Vec<u8>)> = Vec::new();

    // Tempo changes
    for tc in &context.tempo_changes {
        let usec = tc.microseconds_per_beat;
        events.push((
            tc.tick,
            vec![
                0xFF,
                0x51,
                0x03,
                (usec >> 16) as u8,
                (usec >> 8) as u8,
                usec as u8,
            ],
        ));
    }

    // Time signatures
    for ts in &context.time_signatures {
        let denom_pow = (ts.denominator as f64).log2() as u8;
        events.push((
            ts.tick,
            vec![0xFF, 0x58, 0x04, ts.numerator, denom_pow, 0x18, 0x08],
        ));
    }

    // If no tempo was provided, emit default 120 BPM
    if context.tempo_changes.is_empty() {
        let usec: u32 = 500_000; // 120 BPM
        events.push((
            0,
            vec![
                0xFF,
                0x51,
                0x03,
                (usec >> 16) as u8,
                (usec >> 8) as u8,
                usec as u8,
            ],
        ));
    }

    events.sort_by_key(|(tick, _)| *tick);

    let mut track_data = Vec::new();
    let mut last_tick = 0u64;

    for (tick, data) in events {
        let delta = tick - last_tick;
        write_vlq(&mut track_data, delta as u32);
        track_data.extend_from_slice(&data);
        last_tick = tick;
    }

    // End of track
    write_vlq(&mut track_data, 0);
    track_data.extend_from_slice(&[0xFF, 0x2F, 0x00]);

    track_data
}

/// Build a track for a single separated voice.
fn build_voice_track(voice: &SeparatedVoice, channel: u8, options: &ExportOptions) -> Vec<u8> {
    let mut events: Vec<(u64, Vec<u8>)> = Vec::new();

    // Track name
    let name = format!("Voice {}", voice.voice_index);
    let name_bytes = name.as_bytes();
    let mut name_event = vec![0xFF, 0x03];
    write_vlq_to_vec(&mut name_event, name_bytes.len() as u32);
    name_event.extend_from_slice(name_bytes);
    events.push((0, name_event));

    // Program change (use original if available, else piano)
    if options.assign_programs {
        let program = voice
            .source_channel
            .map(|_| 0u8) // could be smarter in Phase 2
            .unwrap_or(0);
        events.push((0, vec![0xC0 | (channel & 0x0F), program]));
    }

    // Note events
    for note in &voice.notes {
        // Note On
        events.push((
            note.onset_tick,
            vec![0x90 | (channel & 0x0F), note.pitch, note.velocity],
        ));
        // Note Off
        events.push((
            note.offset_tick,
            vec![0x80 | (channel & 0x0F), note.pitch, 0],
        ));
    }

    // Sort by tick, with note-offs before note-ons at the same tick
    events.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| {
            let a_is_off = a.1.first().is_some_and(|b| b & 0xF0 == 0x80);
            let b_is_off = b.1.first().is_some_and(|b| b & 0xF0 == 0x80);
            b_is_off.cmp(&a_is_off) // note-offs first
        })
    });

    let mut track_data = Vec::new();
    let mut last_tick = 0u64;

    for (tick, data) in events {
        let delta = tick.saturating_sub(last_tick);
        write_vlq(&mut track_data, delta as u32);
        track_data.extend_from_slice(&data);
        last_tick = tick;
    }

    // End of track
    write_vlq(&mut track_data, 0);
    track_data.extend_from_slice(&[0xFF, 0x2F, 0x00]);

    track_data
}

/// Assemble a complete MIDI file from track data blobs.
fn build_midi_file(ppq: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
    let mut buf = Vec::new();

    // MThd header
    buf.extend_from_slice(b"MThd");
    buf.extend_from_slice(&6u32.to_be_bytes());
    buf.extend_from_slice(&1u16.to_be_bytes()); // format 1
    buf.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    buf.extend_from_slice(&ppq.to_be_bytes());

    // MTrk chunks
    for track_data in tracks {
        buf.extend_from_slice(b"MTrk");
        buf.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
        buf.extend_from_slice(track_data);
    }

    buf
}

/// Write a variable-length quantity to a byte buffer.
fn write_vlq(buf: &mut Vec<u8>, mut value: u32) {
    if value == 0 {
        buf.push(0);
        return;
    }

    let mut bytes = Vec::new();
    bytes.push((value & 0x7F) as u8);
    value >>= 7;

    while value > 0 {
        bytes.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }

    bytes.reverse();
    buf.extend_from_slice(&bytes);
}

/// Write VLQ without the extra vec allocation.
fn write_vlq_to_vec(buf: &mut Vec<u8>, value: u32) {
    write_vlq(buf, value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{TempoChange, TimeSignature};
    use crate::note::{SeparationMethod, TimedNote, VoiceStats};
    use midly::Smf;

    fn make_voice(notes: Vec<TimedNote>, index: usize) -> SeparatedVoice {
        SeparatedVoice {
            stats: VoiceStats::from_notes(&notes),
            notes,
            method: SeparationMethod::PitchContiguity,
            voice_index: index,
            source_channel: None,
            source_track: Some(0),
        }
    }

    fn make_context() -> MidiFileContext {
        MidiFileContext {
            ppq: 480,
            format: 1,
            track_count: 2,
            tempo_changes: vec![TempoChange {
                tick: 0,
                microseconds_per_beat: 500_000,
                bpm: 120.0,
            }],
            time_signatures: vec![TimeSignature {
                tick: 0,
                numerator: 4,
                denominator: 4,
            }],
            total_ticks: 1920,
        }
    }

    #[test]
    fn round_trip_single_voice() {
        let notes = vec![
            TimedNote {
                onset_tick: 0,
                offset_tick: 480,
                pitch: 60,
                velocity: 100,
                channel: 0,
                track_index: 0,
            },
            TimedNote {
                onset_tick: 480,
                offset_tick: 960,
                pitch: 64,
                velocity: 100,
                channel: 0,
                track_index: 0,
            },
        ];

        let voice = make_voice(notes, 0);
        let context = make_context();
        let midi_bytes = voices_to_midi(&[voice], &context, &ExportOptions::default());

        // Parse the output with midly
        let smf = Smf::parse(&midi_bytes).expect("Generated MIDI should be valid");
        assert_eq!(smf.header.format, midly::Format::Parallel); // format 1
        assert_eq!(smf.tracks.len(), 2); // tempo + 1 voice
    }

    #[test]
    fn round_trip_preserves_notes() {
        let notes = vec![
            TimedNote {
                onset_tick: 0,
                offset_tick: 480,
                pitch: 60,
                velocity: 100,
                channel: 0,
                track_index: 0,
            },
            TimedNote {
                onset_tick: 480,
                offset_tick: 960,
                pitch: 67,
                velocity: 80,
                channel: 0,
                track_index: 0,
            },
        ];

        let voice = make_voice(notes.clone(), 0);
        let context = make_context();
        let midi_bytes = voices_to_midi(&[voice], &context, &ExportOptions::default());

        let smf = Smf::parse(&midi_bytes).unwrap();

        // Count note-on events in the voice track (track 1)
        let mut note_ons = 0;
        for event in &smf.tracks[1] {
            if let midly::TrackEventKind::Midi {
                message: midly::MidiMessage::NoteOn { vel, .. },
                ..
            } = event.kind
            {
                if vel.as_int() > 0 {
                    note_ons += 1;
                }
            }
        }
        assert_eq!(note_ons, 2);
    }

    #[test]
    fn multi_voice_output() {
        let voice0 = make_voice(
            vec![TimedNote {
                onset_tick: 0,
                offset_tick: 960,
                pitch: 72,
                velocity: 100,
                channel: 0,
                track_index: 0,
            }],
            0,
        );
        let voice1 = make_voice(
            vec![TimedNote {
                onset_tick: 0,
                offset_tick: 960,
                pitch: 48,
                velocity: 80,
                channel: 0,
                track_index: 0,
            }],
            1,
        );

        let context = make_context();
        let midi_bytes = voices_to_midi(&[voice0, voice1], &context, &ExportOptions::default());

        let smf = Smf::parse(&midi_bytes).unwrap();
        assert_eq!(smf.tracks.len(), 3); // tempo + 2 voices
    }

    #[test]
    fn vlq_encoding() {
        let mut buf = Vec::new();
        write_vlq(&mut buf, 0);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        write_vlq(&mut buf, 127);
        assert_eq!(buf, vec![0x7F]);

        buf.clear();
        write_vlq(&mut buf, 128);
        assert_eq!(buf, vec![0x81, 0x00]);

        buf.clear();
        write_vlq(&mut buf, 480);
        assert_eq!(buf, vec![0x83, 0x60]);
    }
}
