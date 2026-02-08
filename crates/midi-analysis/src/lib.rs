pub mod analyze;
pub mod gm;
pub mod midi_writer;
pub mod note;
pub mod voice_separate;

pub use analyze::{analyze, MidiAnalysis, MidiFileContext, TrackProfile};
pub use midi_writer::{voices_to_midi, ExportOptions};
pub use note::{SeparatedVoice, SeparationMethod, TimedNote, VoiceStats};
pub use voice_separate::{separate_voices, SeparationParams};

/// Errors from MIDI analysis operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("MIDI parse error: {0}")]
    MidiParse(String),
}

pub type Result<T> = std::result::Result<T, Error>;
