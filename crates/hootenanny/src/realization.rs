use crate::domain::{EmotionalVector, AbstractEvent, ConcreteEvent, NoteEvent};

impl AbstractEvent {
    /// Transform abstract intention into concrete sound through the Alchemical Codex.
    /// Uses both the legacy "how" field and the EmotionalVector for realization.
    pub fn realize(&self) -> ConcreteEvent {
        match self {
            AbstractEvent::Intention(intention) => {
                let pitch = note_to_midi(&intention.what);

                // Use EmotionalVector to determine velocity and duration
                let velocity = emotion_to_velocity(&intention.emotion, &intention.how);
                let duration_ms = emotion_to_duration(&intention.emotion);

                tracing::info!(
                    "🎵 {} {} (v:{:.2}, a:{:.2}, ag:{:.2}) → pitch:{}, vel:{}, dur:{}ms",
                    intention.how,
                    intention.what,
                    intention.emotion.valence,
                    intention.emotion.arousal,
                    intention.emotion.agency,
                    pitch,
                    velocity,
                    duration_ms
                );

                ConcreteEvent::Note(NoteEvent {
                    note: resonode::Note {
                        pitch: resonode::Pitch::new(pitch),
                        velocity: resonode::Velocity(velocity as u16),
                        articulation: resonode::Articulation::Custom("".to_string()),
                    },
                    start_time: resonode::AbsoluteTime(0),
                    duration: resonode::Duration::Absolute(resonode::AbsoluteTime(duration_ms)),
                })
            }
            _ => {
                // Handle other AbstractEvent variants later
                unimplemented!()
            }
        }
    }
}

fn note_to_midi(note: &str) -> u8 {
    match note {
        "C" => 60,
        "D" => 62,
        "E" => 64,
        "F" => 65,
        "G" => 67,
        "A" => 69,
        "B" => 71,
        _ => 60, // Default to C
    }
}

/// Map EmotionalVector to MIDI velocity using the Alchemical Codex.
/// Base dynamics = 0.3 + (arousal * 0.4) + (agency * 0.2) + (valence * 0.1)
fn emotion_to_velocity(emotion: &EmotionalVector, legacy_how: &str) -> u8 {
    // Calculate base dynamics from emotion (normalized 0-1)
    let base_dynamics = 0.3
        + (emotion.arousal * 0.4)
        + (emotion.agency * 0.2)
        + (emotion.valence * 0.1);

    // Clamp to valid range and convert to MIDI (0-127)
    let velocity = (base_dynamics.clamp(0.0, 1.0) * 127.0) as u8;

    // Legacy fallback if emotion results in middle range
    if velocity >= 55 && velocity <= 75 {
        feeling_to_velocity(legacy_how)
    } else {
        velocity
    }
}

fn feeling_to_velocity(feeling: &str) -> u8 {
    match feeling {
        "softly" => 40,
        "normally" => 64,
        "boldly" => 90,
        "questioning" => 50,
        _ => 64,
    }
}

/// Map arousal to note duration.
/// High arousal = shorter, more energetic notes
/// Low arousal = longer, sustained notes
fn emotion_to_duration(emotion: &EmotionalVector) -> u64 {
    let base_duration = 500.0; // milliseconds

    // Arousal inversely affects duration
    // arousal 0.0 → 2x longer, arousal 1.0 → 0.5x shorter
    let arousal_factor = 2.0 - (emotion.arousal * 1.5);

    (base_duration * arousal_factor).round() as u64
}

#[cfg(test)]
mod tests {
    use crate::domain::{EmotionalVector, AbstractEvent, IntentionEvent, ConcreteEvent, NoteEvent};

    #[test]
    fn intention_becomes_sound() {
        let intention = AbstractEvent::Intention(IntentionEvent {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion: EmotionalVector {
                valence: -0.3,
                arousal: 0.2,
                agency: -0.2,
            },
        });

        let sound = intention.realize();

        if let ConcreteEvent::Note(note_event) = sound {
            assert_eq!(note_event.note.pitch.midi_note_number, 60);
            assert!(note_event.note.velocity.0 < 5000);
        } else {
            panic!("Expected a NoteEvent");
        }
    }

    #[test]
    fn different_intentions_different_sounds() {
        let soft_c = AbstractEvent::Intention(IntentionEvent {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion: EmotionalVector {
                valence: -0.3,
                arousal: 0.2,
                agency: -0.2,
            },
        })
        .realize();

        let bold_g = AbstractEvent::Intention(IntentionEvent {
            what: "G".to_string(),
            how: "boldly".to_string(),
            emotion: EmotionalVector {
                valence: 0.7,
                arousal: 0.8,
                agency: 0.7,
            },
        })
        .realize();

        if let (ConcreteEvent::Note(soft_c_note), ConcreteEvent::Note(bold_g_note)) = (soft_c, bold_g) {
            assert_ne!(soft_c_note.note.pitch.midi_note_number, bold_g_note.note.pitch.midi_note_number);
            assert!(soft_c_note.note.velocity.0 < bold_g_note.note.velocity.0);
        } else {
            panic!("Expected NoteEvents");
        }
    }

    #[test]
    fn high_arousal_creates_high_velocity() {
        let intense = AbstractEvent::Intention(IntentionEvent {
            what: "C".to_string(),
            how: "normally".to_string(),
            emotion: EmotionalVector {
                valence: 0.5,
                arousal: 0.9, // Very high arousal
                agency: 0.8,
            },
        })
        .realize();

        if let ConcreteEvent::Note(intense_note) = intense {
        // High arousal should result in high velocity
        assert!(intense_note.note.velocity.0 > 100);
        } else {
            panic!("Expected a NoteEvent");
        }
    }

    #[test]
    fn low_arousal_creates_longer_notes() {
        let calm = AbstractEvent::Intention(IntentionEvent {
            what: "C".to_string(),
            how: "normally".to_string(),
            emotion: EmotionalVector {
                valence: 0.3,
                arousal: 0.1, // Very low arousal
                agency: 0.0,
            },
        })
        .realize();

        if let ConcreteEvent::Note(calm_note) = calm {
            if let resonode::Duration::Absolute(duration) = calm_note.duration {
                assert!(duration.0 > 800);
            } else {
                panic!("Expected Absolute Duration");
            }
        } else {
            panic!("Expected a NoteEvent");
        }
    }
}
