//! ABC notation parser using winnow.
//!
//! The parser is designed to be generous - it will attempt to continue
//! parsing even when encountering issues, collecting feedback for the user.

mod header;
mod body;
mod note;
mod key;

use std::collections::HashMap;
use crate::ast::{Element, Tune, Voice};
use crate::feedback::{FeedbackCollector, ParseResult};

/// Parse ABC notation into a Tune AST.
pub fn parse(input: &str) -> ParseResult<Tune> {
    let mut collector = FeedbackCollector::new();

    // Parse header
    let (remaining, header) = header::parse_header(input, &mut collector);

    // Parse body
    let elements = body::parse_body(remaining, &mut collector);

    // Route elements to voices based on VoiceSwitch elements
    let voices = route_elements_to_voices(&header.voice_defs, elements);

    let tune = Tune {
        header,
        voices,
    };

    ParseResult::new(tune, collector.into_feedback())
}

/// Route parsed elements to their respective voices based on VoiceSwitch markers.
fn route_elements_to_voices(voice_defs: &[crate::ast::VoiceDef], elements: Vec<Element>) -> Vec<Voice> {
    // If no voice definitions, put everything in a single default voice
    if voice_defs.is_empty() {
        // Filter out VoiceSwitch elements if any (shouldn't exist without defs, but be safe)
        let filtered: Vec<_> = elements.into_iter()
            .filter(|e| !matches!(e, Element::VoiceSwitch(_)))
            .collect();
        return vec![Voice {
            id: None,
            name: None,
            elements: filtered,
        }];
    }

    // Create a voice for each definition
    let mut voice_map: HashMap<String, Vec<Element>> = HashMap::new();
    for def in voice_defs {
        voice_map.insert(def.id.clone(), Vec::new());
    }

    // Start with the first defined voice
    let mut current_voice_id = voice_defs[0].id.clone();

    for element in elements {
        match element {
            Element::VoiceSwitch(ref voice_id) => {
                // Switch to the specified voice (create if not exists)
                current_voice_id = voice_id.clone();
                if !voice_map.contains_key(&current_voice_id) {
                    voice_map.insert(current_voice_id.clone(), Vec::new());
                }
            }
            _ => {
                // Add element to current voice
                if let Some(voice_elements) = voice_map.get_mut(&current_voice_id) {
                    voice_elements.push(element);
                }
            }
        }
    }

    // Build voices in order of definitions, plus any extras
    let mut voices = Vec::new();
    for def in voice_defs {
        let elements = voice_map.remove(&def.id).unwrap_or_default();
        voices.push(Voice {
            id: Some(def.id.clone()),
            name: def.name.clone(),
            elements,
        });
    }

    // Add any voices that were created by VoiceSwitch but not in defs
    for (id, elements) in voice_map {
        if !elements.is_empty() {
            voices.push(Voice {
                id: Some(id),
                name: None,
                elements,
            });
        }
    }

    voices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    #[test]
    fn test_parse_minimal() {
        let abc = "X:1\nT:Test\nK:C\n";
        let result = parse(abc);

        assert!(!result.has_errors());
        assert_eq!(result.value.header.reference, 1);
        assert_eq!(result.value.header.title, "Test");
        assert_eq!(result.value.header.key.root, NoteName::C);
    }

    #[test]
    fn test_parse_with_meter() {
        let abc = "X:1\nT:Test\nM:6/8\nK:G\n";
        let result = parse(abc);

        assert!(!result.has_errors());
        assert_eq!(
            result.value.header.meter,
            Some(Meter::Simple {
                numerator: 6,
                denominator: 8
            })
        );
    }

    #[test]
    fn test_parse_common_time() {
        let abc = "X:1\nT:Test\nM:C\nK:D\n";
        let result = parse(abc);

        assert!(!result.has_errors());
        assert_eq!(result.value.header.meter, Some(Meter::Common));
    }

    #[test]
    fn test_parse_cut_time() {
        let abc = "X:1\nT:Test\nM:C|\nK:D\n";
        let result = parse(abc);

        assert!(!result.has_errors());
        assert_eq!(result.value.header.meter, Some(Meter::Cut));
    }

    #[test]
    fn test_parse_unit_length() {
        let abc = "X:1\nT:Test\nL:1/16\nK:C\n";
        let result = parse(abc);

        assert!(!result.has_errors());
        assert_eq!(
            result.value.header.unit_length,
            Some(UnitLength {
                numerator: 1,
                denominator: 16
            })
        );
    }

    #[test]
    fn test_parse_tempo() {
        let abc = "X:1\nT:Test\nQ:1/4=120\nK:C\n";
        let result = parse(abc);

        assert!(!result.has_errors());
        let tempo = result.value.header.tempo.as_ref().unwrap();
        assert_eq!(tempo.bpm, 120);
        assert_eq!(tempo.beat_unit, (1, 4));
    }

    #[test]
    fn test_parse_simple_notes() {
        let abc = "X:1\nT:Test\nK:C\nCDEF|";
        let result = parse(abc);

        assert!(!result.has_errors());

        let notes: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        assert_eq!(notes.len(), 4);
        assert_eq!(notes[0].pitch, NoteName::C);
        assert_eq!(notes[0].octave, 0);
        assert_eq!(notes[1].pitch, NoteName::D);
        assert_eq!(notes[2].pitch, NoteName::E);
        assert_eq!(notes[3].pitch, NoteName::F);
    }

    #[test]
    fn test_parse_lowercase_notes() {
        let abc = "X:1\nT:Test\nK:C\ncdef|";
        let result = parse(abc);

        let notes: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        assert_eq!(notes.len(), 4);
        assert_eq!(notes[0].pitch, NoteName::C);
        assert_eq!(notes[0].octave, 1); // Lowercase = octave 1
    }

    #[test]
    fn test_parse_octave_modifiers() {
        let abc = "X:1\nT:Test\nK:C\nC,Cc'|";
        let result = parse(abc);

        let notes: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].octave, -1); // C,
        assert_eq!(notes[1].octave, 0);  // C
        assert_eq!(notes[2].octave, 2);  // c'
    }

    #[test]
    fn test_parse_accidentals() {
        let abc = "X:1\nT:Test\nK:C\n^C_D=E^^F__|";
        let result = parse(abc);

        let notes: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        // Note: this depends on how we parse ^^F__
        // It could be ^^F followed by __, or it could fail
        // For now, let's check what we get
        assert!(notes.len() >= 4);
        assert_eq!(notes[0].accidental, Some(Accidental::Sharp));
        assert_eq!(notes[1].accidental, Some(Accidental::Flat));
        assert_eq!(notes[2].accidental, Some(Accidental::Natural));
    }

    #[test]
    fn test_parse_durations() {
        let abc = "X:1\nT:Test\nK:C\nA A2 A/2 A3/2|";
        let result = parse(abc);

        let notes: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        assert_eq!(notes.len(), 4);
        assert_eq!(notes[0].duration, Duration::new(1, 1));
        assert_eq!(notes[1].duration, Duration::new(2, 1));
        assert_eq!(notes[2].duration, Duration::new(1, 2));
        assert_eq!(notes[3].duration, Duration::new(3, 2));
    }

    #[test]
    fn test_parse_rest() {
        let abc = "X:1\nT:Test\nK:C\nz z2|";
        let result = parse(abc);

        let rests: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Rest(r) => Some(r),
                _ => None,
            })
            .collect();

        assert_eq!(rests.len(), 2);
        assert!(rests[0].visible);
        assert_eq!(rests[1].duration, Duration::new(2, 1));
    }

    #[test]
    fn test_parse_chord() {
        let abc = "X:1\nT:Test\nK:C\n[CEG]2|";
        let result = parse(abc);

        let chords: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Chord(c) => Some(c),
                _ => None,
            })
            .collect();

        assert_eq!(chords.len(), 1);
        assert_eq!(chords[0].notes.len(), 3);
        assert_eq!(chords[0].duration, Duration::new(2, 1));
    }

    #[test]
    fn test_parse_bar_types() {
        let abc = "X:1\nT:Test\nK:C\nC|D||E|]";
        let result = parse(abc);

        let bars: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Bar(b) => Some(b),
                _ => None,
            })
            .collect();

        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0], &Bar::Single);
        assert_eq!(bars[1], &Bar::Double);
        assert_eq!(bars[2], &Bar::End);
    }

    #[test]
    fn test_parse_repeat_bars() {
        let abc = "X:1\nT:Test\nK:C\n|:C:|D::|";
        let result = parse(abc);

        let bars: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Bar(b) => Some(b),
                _ => None,
            })
            .collect();

        assert!(bars.contains(&&Bar::RepeatStart));
        assert!(bars.contains(&&Bar::RepeatEnd));
    }

    #[test]
    fn test_parse_tie() {
        let abc = "X:1\nT:Test\nK:C\nC-C|";
        let result = parse(abc);

        let notes: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::Note(n) => Some(n),
                _ => None,
            })
            .collect();

        assert_eq!(notes.len(), 2);
        assert!(notes[0].tie);
        assert!(!notes[1].tie);
    }

    #[test]
    fn test_parse_missing_x_field_warns() {
        let abc = "T:Test\nK:C\nCDE|";
        let result = parse(abc);

        // Should warn but not error
        assert!(result.feedback.iter().any(|f| f.message.contains("X:")));
        // Should still parse
        assert_eq!(result.value.header.title, "Test");
    }

    #[test]
    fn test_parse_key_modes() {
        let abc = "X:1\nT:Test\nK:D dorian\n";
        let result = parse(abc);

        assert_eq!(result.value.header.key.root, NoteName::D);
        assert_eq!(result.value.header.key.mode, Mode::Dorian);
    }

    #[test]
    fn test_parse_key_with_accidental() {
        let abc = "X:1\nT:Test\nK:F#m\n";
        let result = parse(abc);

        assert_eq!(result.value.header.key.root, NoteName::F);
        assert_eq!(result.value.header.key.accidental, Some(Accidental::Sharp));
        assert_eq!(result.value.header.key.mode, Mode::Minor);
    }

    #[test]
    fn test_parse_chord_symbol() {
        let abc = "X:1\nT:Test\nK:C\n\"G\"GAB|";
        let result = parse(abc);

        let symbols: Vec<_> = result.value.voices[0]
            .elements
            .iter()
            .filter_map(|e| match e {
                Element::ChordSymbol(s) => Some(s),
                _ => None,
            })
            .collect();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0], "G");
    }
}
