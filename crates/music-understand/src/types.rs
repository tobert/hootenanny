use midi_analysis::{MidiFileContext, VoiceFeatures, VoiceRole};
use serde::{Deserialize, Serialize};

/// Complete music understanding for a MIDI file.
///
/// Composes voice separation/classification (from midi-analysis) with
/// key detection, meter detection, and chord extraction into a single
/// cached result addressed by content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicUnderstanding {
    pub content_hash: String,
    /// Algorithm version — cache invalidation on bump
    pub version: u32,
    pub context: MidiFileContext,
    pub key: KeyDetection,
    pub meter: MeterDetection,
    pub voices: Vec<ClassifiedVoice>,
    pub chords: Vec<ChordEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyMode {
    Major,
    Minor,
}

impl std::fmt::Display for KeyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyMode::Major => write!(f, "major"),
            KeyMode::Minor => write!(f, "minor"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDetection {
    /// Root note name: "C", "Db", "F#", etc.
    pub root: String,
    /// Pitch class 0–11 (C=0, C#=1, ...)
    pub root_pitch_class: u8,
    pub mode: KeyMode,
    /// Pearson correlation with best-matching key profile
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeterDetection {
    pub numerator: u8,
    pub denominator: u8,
    pub confidence: f64,
    /// 0.0 = straight feel, 1.0 = compound/triplet feel
    pub triplet_feel: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChordQuality {
    Major,
    Minor,
    Diminished,
    Augmented,
    Suspended4,
    Suspended2,
    Dominant7,
    Major7,
    Minor7,
    MinorMajor7,
    Diminished7,
    HalfDiminished7,
    Major6,
    Minor6,
    Add9,
    Power,
}

impl ChordQuality {
    /// Suffix for chord symbol display
    pub fn suffix(&self) -> &'static str {
        match self {
            ChordQuality::Major => "",
            ChordQuality::Minor => "m",
            ChordQuality::Diminished => "dim",
            ChordQuality::Augmented => "aug",
            ChordQuality::Suspended4 => "sus4",
            ChordQuality::Suspended2 => "sus2",
            ChordQuality::Dominant7 => "7",
            ChordQuality::Major7 => "maj7",
            ChordQuality::Minor7 => "m7",
            ChordQuality::MinorMajor7 => "m(maj7)",
            ChordQuality::Diminished7 => "dim7",
            ChordQuality::HalfDiminished7 => "m7b5",
            ChordQuality::Major6 => "6",
            ChordQuality::Minor6 => "m6",
            ChordQuality::Add9 => "add9",
            ChordQuality::Power => "5",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChordEvent {
    /// Beat position where this chord begins
    pub beat: f64,
    /// Full chord symbol: "Cmaj7", "Dm", "G7"
    pub symbol: String,
    pub root_pitch_class: u8,
    pub quality: ChordQuality,
    pub confidence: f64,
}

/// A voice with its classification, features, and constituent notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedVoice {
    pub voice_index: usize,
    pub role: VoiceRole,
    pub confidence: f64,
    pub notes: Vec<midi_analysis::TimedNote>,
    pub features: VoiceFeatures,
}
