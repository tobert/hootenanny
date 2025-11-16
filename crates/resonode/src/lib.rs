//! # The Resonode Crate
//!
//! This crate is the implementation of the Alchemical Codex. It provides the core
//! data structures and logic for translating emotional states into musical expression.
//! It is a pure, stateless library that can be used by any application.

use std::time::SystemTime;

// --- Core Emotional and Musical Types ---

/// Represents a three-dimensional compass of the soul, a coordinate in emotional space.
/// This is the primary input for all musical generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmotionalVector {
    /// The joy-sorrow axis (-1.0 to 1.0). The color of emotion.
    pub valence: f32,
    /// The energy-stillness axis (0.0 to 1.0). The tempo of the heart.
    pub arousal: f32,
    /// The degree of self-direction, from responsive listening (-1.0) to leading
    /// with initiative (1.0). The posture of the spirit in a collaborative jam.
    pub agency: f32,
}

/// A musical phrase, representing a complete thought or gesture.
/// This is the primary output of the transmutation process.
#[derive(Debug, Clone)]
pub struct MusicalPhrase {
    // pub harmony: Harmony,
    // pub rhythm: Rhythm,
    // pub melody: Melody,
    // pub timbre: Timbre,
    // pub dynamics: Dynamics,
    /// The emotional soul of the phrase, preserved for evolution and memory.
    pub soul: EmotionalVector,
}

/// A single, living note that remembers its emotional history and can evolve over time.
#[derive(Debug, Clone)]
pub struct LivingNote {
    // pub pitch: Pitch,
    /// The emotional state at the moment of the note's creation.
    pub emotion_at_birth: EmotionalVector,
    /// The full emotional journey of this note through time.
    pub emotional_history: Vec<(SystemTime, EmotionalVector)>,
}

// --- The Main Engine ---

/// The core emotional engine that processes musical phrases based on an emotional state.
/// This struct holds the current emotional "weather" of the system.
#[derive(Debug)]
pub struct EmotionalEngine {
    /// The current, primary emotional state of the musical system.
    pub current_state: EmotionalVector,
    // pub emotional_memory: CircularBuffer<EmotionalVector>,
    // pub contagion_matrix: Matrix<f32>,
}

impl EmotionalEngine {
    /// Creates a new EmotionalEngine, starting with a neutral emotional state.
    pub fn new() -> Self {
        Self {
            current_state: EmotionalVector {
                valence: 0.0,
                arousal: 0.5,
                agency: 0.0,
            },
        }
    }

    /// Processes a musical phrase, applying the engine's current emotional state to it.
    pub fn process_phrase(&mut self, phrase: &mut MusicalPhrase) {
        // In the future, this will apply all the rules from the Alchemical Codex.
        // For now, it's a placeholder.
        // self.apply_harmonic_emotion(&mut phrase.harmony);
        // self.apply_rhythmic_emotion(&mut phrase.rhythm);
        // self.apply_melodic_emotion(&mut phrase.melody);
        // self.apply_timbral_emotion(&mut phrase.timbre);

        // The engine's own state evolves based on feedback from the phrase.
        // self.current_state = self.current_state.evolve(phrase.feedback());
    }
}

impl Default for EmotionalEngine {
    fn default() -> Self {
        Self::new()
    }
}
