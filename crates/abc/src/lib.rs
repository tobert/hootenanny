//! ABC notation parser and MIDI generator.
//!
//! This crate provides tools for parsing ABC music notation into a structured
//! AST, and converting that AST to MIDI.
//!
//! # Example
//!
//! ```
//! use abc::{parse, to_midi, MidiParams};
//!
//! let abc = r#"
//! X:1
//! T:Test Tune
//! M:4/4
//! L:1/8
//! K:G
//! GABc dedB|cBAG D2D2|
//! "#;
//!
//! let result = parse(abc);
//! if !result.has_errors() {
//!     let midi_bytes = to_midi(&result.value, &MidiParams::default());
//!     // midi_bytes is a valid SMF format 0 MIDI file
//! }
//! ```

pub mod ast;
pub mod feedback;
pub mod midi;
pub mod parser;

pub use ast::*;
pub use feedback::{Feedback, FeedbackLevel, ParseResult};

/// Parse ABC notation into a Tune AST.
///
/// This is a generous parser that will attempt to continue parsing
/// even when encountering issues, collecting feedback along the way.
pub fn parse(input: &str) -> ParseResult<Tune> {
    parser::parse(input)
}

/// Parameters for MIDI generation
#[derive(Debug, Clone)]
pub struct MidiParams {
    /// MIDI velocity for notes (1-127)
    pub velocity: u8,
    /// Ticks per quarter note (typically 480)
    pub ticks_per_beat: u16,
    /// MIDI channel (0-15, default 0). Use 9 for GM drums.
    pub channel: u8,
}

impl Default for MidiParams {
    fn default() -> Self {
        MidiParams {
            velocity: 80,
            ticks_per_beat: 480,
            channel: 0,
        }
    }
}

/// Convert a parsed Tune to MIDI bytes (SMF format 0)
pub fn to_midi(tune: &Tune, params: &MidiParams) -> Vec<u8> {
    midi::generate(tune, params)
}

/// Transpose a tune by the given number of semitones
pub fn transpose(_tune: &Tune, _semitones: i8) -> Tune {
    // TODO: Implement
    Tune::default()
}

/// Convert a Tune back to ABC notation string
pub fn to_abc(_tune: &Tune) -> String {
    // TODO: Implement
    String::new()
}

/// Calculate semitones needed to transpose from source key to target key
pub fn semitones_to_key(_source: &Key, _target: &str) -> Result<i8, String> {
    // TODO: Implement
    Err("Not implemented".to_string())
}
