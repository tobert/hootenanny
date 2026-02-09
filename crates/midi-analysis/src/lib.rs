pub mod analyze;
pub mod classify;
pub mod gm;
pub mod midi_writer;
pub mod note;
pub mod voice_separate;

pub use analyze::{analyze, MidiAnalysis, MidiFileContext, TrackProfile};
pub use classify::{
    classify_heuristic, classify_voices, classify_voices_with_features, extract_features,
    ClassificationMethod, VoiceClassification, VoiceFeatures, VoiceRole,
};
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
