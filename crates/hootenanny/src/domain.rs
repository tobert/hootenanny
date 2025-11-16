use rmcp::schemars;
use serde::{Deserialize, Serialize};

/// The fundamental duality: Abstract intentions and Concrete sounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Abstract(Intention),
    Concrete(Sound),
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
pub struct Intention {
    #[schemars(description = "The note to play (C, D, E, F, G, A, B)")]
    pub what: String,

    #[schemars(description = "How to play it (softly, normally, boldly, questioning)")]
    pub how: String,

    #[schemars(description = "The emotional vector shaping this intention")]
    pub emotion: EmotionalVector,
}

/// A concrete sound event in the physical world.
/// The "what actually happens" after intention becomes reality.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Sound {
    #[schemars(description = "MIDI note number (0-127)")]
    pub pitch: u8,

    #[schemars(description = "MIDI velocity (0-127)")]
    pub velocity: u8,

    #[schemars(description = "The emotional vector that birthed this sound")]
    pub emotion: EmotionalVector,

    #[schemars(description = "Duration in milliseconds")]
    pub duration_ms: u64,
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

        let intention = Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion,
        };

        let abstract_event = Event::Abstract(intention);
        let concrete_event = Event::Concrete(Sound {
            pitch: 60,
            velocity: 40,
            emotion,
            duration_ms: 500,
        });

        // They coexist
        assert!(matches!(abstract_event, Event::Abstract(_)));
        assert!(matches!(concrete_event, Event::Concrete(_)));
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
