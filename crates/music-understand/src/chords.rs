use midi_analysis::{MidiFileContext, TimedNote};

use crate::chord_templates::{match_chord, FLAT_KEY_ROOTS};
use crate::types::{ChordEvent, KeyDetection};

/// Extract chord symbols at each beat position from harmony and bass voices.
///
/// Walks beat-by-beat through the MIDI file, collects pitch classes of all
/// notes sounding at each beat boundary, and matches against chord templates.
/// Consecutive identical chords are deduplicated.
pub fn extract_chords(
    harmony_notes: &[TimedNote],
    bass_notes: &[TimedNote],
    context: &MidiFileContext,
    key: &KeyDetection,
) -> Vec<ChordEvent> {
    let ppq = context.ppq as f64;
    let use_flats = FLAT_KEY_ROOTS.contains(&key.root_pitch_class);

    if harmony_notes.is_empty() && bass_notes.is_empty() {
        return Vec::new();
    }

    let total_beats = context.total_ticks as f64 / ppq;
    let mut chords = Vec::new();
    let mut prev_symbol: Option<String> = None;

    let mut beat = 0.0;
    while beat < total_beats {
        let beat_tick = (beat * ppq) as u64;

        // Collect pitch classes of all notes sounding at this beat
        let mut pitch_classes = Vec::new();
        let mut bass_pitch: Option<u8> = None;

        // Harmony notes
        for note in harmony_notes {
            if note.onset_tick <= beat_tick && beat_tick < note.offset_tick {
                let pc = note.pitch % 12;
                if !pitch_classes.contains(&pc) {
                    pitch_classes.push(pc);
                }
            }
        }

        // Bass notes — also collect as pitch classes, track lowest for bass hint
        for note in bass_notes {
            if note.onset_tick <= beat_tick && beat_tick < note.offset_tick {
                let pc = note.pitch % 12;
                if !pitch_classes.contains(&pc) {
                    pitch_classes.push(pc);
                }
                if bass_pitch.is_none() {
                    bass_pitch = Some(pc);
                }
            }
        }

        if pitch_classes.len() >= 2 {
            if let Some((root_pc, symbol, quality, confidence)) =
                match_chord(&pitch_classes, bass_pitch, use_flats)
            {
                if prev_symbol.as_ref() != Some(&symbol) {
                    prev_symbol = Some(symbol.clone());
                    chords.push(ChordEvent {
                        beat,
                        symbol,
                        root_pitch_class: root_pc,
                        quality,
                        confidence,
                    });
                }
            }
        } else if pitch_classes.len() == 1 && bass_pitch.is_some() {
            // Single harmony note + bass: try to infer
            let bass_pc = bass_pitch.unwrap();
            if !pitch_classes.contains(&bass_pc) {
                pitch_classes.push(bass_pc);
            }
            if pitch_classes.len() >= 2 {
                if let Some((root_pc, symbol, quality, confidence)) =
                    match_chord(&pitch_classes, bass_pitch, use_flats)
                {
                    let confidence = confidence * 0.7; // lower confidence for sparse evidence
                    if prev_symbol.as_ref() != Some(&symbol) {
                        prev_symbol = Some(symbol.clone());
                        chords.push(ChordEvent {
                            beat,
                            symbol,
                            root_pitch_class: root_pc,
                            quality,
                            confidence,
                        });
                    }
                }
            }
        }

        beat += 1.0;
    }

    chords
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::KeyMode;

    fn make_note(pitch: u8, onset: u64, offset: u64) -> TimedNote {
        TimedNote {
            pitch,
            onset_tick: onset,
            offset_tick: offset,
            velocity: 80,
            channel: 0,
            track_index: 0,
        }
    }

    fn c_major_key() -> KeyDetection {
        KeyDetection {
            root: "C".into(),
            root_pitch_class: 0,
            mode: KeyMode::Major,
            confidence: 0.9,
        }
    }

    fn context(ppq: u16, total_ticks: u64) -> MidiFileContext {
        MidiFileContext {
            ppq,
            format: 1,
            track_count: 1,
            tempo_changes: vec![],
            time_signatures: vec![],
            total_ticks,
        }
    }

    #[test]
    fn empty_notes_empty_chords() {
        let ctx = context(480, 1920);
        let result = extract_chords(&[], &[], &ctx, &c_major_key());
        assert!(result.is_empty());
    }

    #[test]
    fn simple_c_major_chord() {
        let ppq = 480;
        // C E G held for 4 beats
        let harmony = vec![
            make_note(60, 0, 4 * ppq), // C
            make_note(64, 0, 4 * ppq), // E
            make_note(67, 0, 4 * ppq), // G
        ];
        let ctx = context(ppq as u16, 4 * ppq);
        let chords = extract_chords(&harmony, &[], &ctx, &c_major_key());

        assert!(!chords.is_empty());
        assert_eq!(chords[0].symbol, "C");
        assert_eq!(chords[0].beat, 0.0);
    }

    #[test]
    fn consecutive_same_chord_deduplicated() {
        let ppq = 480u64;
        // Same C major chord held for 4 beats
        let harmony = vec![
            make_note(60, 0, 4 * ppq),
            make_note(64, 0, 4 * ppq),
            make_note(67, 0, 4 * ppq),
        ];
        let ctx = context(ppq as u16, 4 * ppq);
        let chords = extract_chords(&harmony, &[], &ctx, &c_major_key());

        // Should only emit once, not once per beat
        assert_eq!(chords.len(), 1);
    }

    #[test]
    fn chord_change_emits_new_event() {
        let ppq = 480u64;
        // C major for beats 0-1, then F major for beats 2-3
        let harmony = vec![
            make_note(60, 0, 2 * ppq),         // C
            make_note(64, 0, 2 * ppq),         // E
            make_note(67, 0, 2 * ppq),         // G
            make_note(65, 2 * ppq, 4 * ppq),   // F
            make_note(69, 2 * ppq, 4 * ppq),   // A
            make_note(60, 2 * ppq, 4 * ppq),   // C
        ];
        let ctx = context(ppq as u16, 4 * ppq);
        let chords = extract_chords(&harmony, &[], &ctx, &c_major_key());

        assert_eq!(chords.len(), 2);
        assert_eq!(chords[0].symbol, "C");
        assert_eq!(chords[1].symbol, "F");
    }

    #[test]
    fn bass_hint_influences_root() {
        let ppq = 480u64;
        let harmony = vec![
            make_note(64, 0, 2 * ppq), // E
            make_note(67, 0, 2 * ppq), // G
        ];
        let bass = vec![make_note(36, 0, 2 * ppq)]; // C in bass

        let ctx = context(ppq as u16, 2 * ppq);
        let chords = extract_chords(&harmony, &bass, &ctx, &c_major_key());

        assert!(!chords.is_empty());
        // With C in bass, should detect C major
        assert_eq!(chords[0].symbol, "C");
    }
}
