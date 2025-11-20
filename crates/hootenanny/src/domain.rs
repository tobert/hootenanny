use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// The fundamental duality: Abstract intentions and Concrete sounds.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Event {
    Abstract(AbstractEvent),
    Concrete(ConcreteEvent),
}

impl Event {
    pub fn is_concrete(&self) -> bool {
        matches!(self, Event::Concrete(_))
    }

    pub fn is_abstract(&self) -> bool {
        matches!(self, Event::Abstract(_))
    }
}

/// The three-dimensional compass of the soul.
/// Maps emotional state to musical parameters via the Alchemical Codex.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EmotionalVector {
    #[schemars(
        description = "Joy-sorrow axis: -1.0 (deepest melancholy) to +1.0 (golden euphoria)"
    )]
    pub valence: f32,

    #[schemars(description = "Energy-stillness axis: 0.0 (meditative calm) to 1.0 (ecstatic frenzy)")]
    pub arousal: f32,

    #[schemars(
        description = "Initiative-responsiveness axis: -1.0 (yielding listener) to +1.0 (leading voice)"
    )]
    pub agency: f32,
}

impl EmotionalVector {
    /// Create a neutral emotional state (balanced, calm, centered).
    pub fn neutral() -> Self {
        Self {
            valence: 0.0,
            arousal: 0.5,
            agency: 0.0,
        }
    }

    /// Interpolate smoothly between two emotional states.
    pub fn interpolate(&self, target: &EmotionalVector, t: f32) -> EmotionalVector {
        EmotionalVector {
            valence: self.valence + (target.valence - self.valence) * t,
            arousal: self.arousal + (target.arousal - self.arousal) * t,
            agency: self.agency + (target.agency - self.agency) * t,
        }
    }

    /// Blend this emotion with another using a weighted average.
    pub fn blend_with(&self, other: &EmotionalVector, weight: f32) -> EmotionalVector {
        self.interpolate(other, weight)
    }
}

/// An abstract musical intention carrying emotional weight.
/// The "what we want to express" before it becomes sound.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub enum AbstractEvent {
    Prompt(PromptEvent),
    Constraint(ConstraintEvent),
    Orchestration(OrchestrationEvent),
    Intention(IntentionEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TimeRange {
    pub start: AbsoluteTime,
    pub end: AbsoluteTime,
}

impl AbstractEvent {
    pub fn to_concrete(&self) -> ConcreteEvent {
        self.realize()
    }

    pub fn applies_to(&self, _time_range: &TimeRange) -> bool {
        // Placeholder implementation
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PromptEvent {
    pub prompt: String,
    pub emotion: EmotionalVector,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConstraintEvent {
    pub constraint: String, // e.g., "stay in C minor"
    pub emotion: EmotionalVector,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrchestrationEvent {
    pub command: String, // e.g., "agent2, play a bassline"
    pub emotion: EmotionalVector,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IntentionEvent {
    pub what: String,
    pub how: String,
    pub emotion: EmotionalVector,
}


pub mod context;
pub mod messages;

use resonode::*;

/// A concrete sound event in the physical world.
/// The "what actually happens" after intention becomes reality.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ConcreteEvent {
    Note(NoteEvent),
    Chord(ChordEvent),
    Control(ControlEvent),
    Pattern(PatternInstance),
    MidiClip(CasReference),
}

/// A reference to an immutable artifact in CAS.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CasReference {
    /// The BLAKE3 hash of the content (32 hex chars).
    pub hash: String,
    /// MIME type of the content (e.g., "audio/midi").
    pub mime_type: String,
    /// Size in bytes (for quick introspection).
    pub size_bytes: u64,
    /// Optional path for debugging/direct access if locally available.
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NoteEvent {
    pub note: Note,
    pub start_time: AbsoluteTime,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChordEvent {
    pub chord: Chord,
    pub start_time: AbsoluteTime,
    pub duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ControlEvent {
    pub parameter: String,
    pub value: f32,
    pub time: AbsoluteTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PatternInstance {
    pub pattern_id: String,
    pub start_time: AbsoluteTime,
    pub events: Vec<ConcreteEvent>,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_duality_exists() {
        let emotion = EmotionalVector {
            valence: -0.3,  // Gentle sadness
            arousal: 0.2,   // Low energy
            agency: -0.2,   // Yielding
        };

        let intention = AbstractEvent::Intention(IntentionEvent {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion,
        });

        let abstract_event = Event::Abstract(intention);
        let concrete_event = Event::Concrete(ConcreteEvent::Note(NoteEvent {
            note: resonode::Note {
                pitch: resonode::Pitch::new(60),
                velocity: resonode::Velocity(40),
                articulation: resonode::Articulation::Custom("".to_string()),
            },
            start_time: resonode::AbsoluteTime(0),
            duration: resonode::Duration::Absolute(resonode::AbsoluteTime(500)),
        }));

        // They coexist
        assert!(abstract_event.is_abstract());
        assert!(concrete_event.is_concrete());
    }

    #[test]
    fn emotional_vector_interpolation() {
        let sad = EmotionalVector {
            valence: -0.7,
            arousal: 0.2,
            agency: -0.4,
        };

        let joyful = EmotionalVector {
            valence: 0.8,
            arousal: 0.7,
            agency: 0.5,
        };

        // Halfway between sad and joyful
        let middle = sad.interpolate(&joyful, 0.5);

        assert!((middle.valence - 0.05).abs() < 0.01);
        assert!((middle.arousal - 0.45).abs() < 0.01);
        assert!((middle.agency - 0.05).abs() < 0.01);
    }

    #[test]
    fn neutral_emotion_is_balanced() {
        let neutral = EmotionalVector::neutral();

        assert_eq!(neutral.valence, 0.0);
        assert_eq!(neutral.arousal, 0.5);
        assert_eq!(neutral.agency, 0.0);
    }
}
