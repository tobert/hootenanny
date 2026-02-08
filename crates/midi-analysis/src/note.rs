use serde::{Deserialize, Serialize};

/// A single MIDI note with absolute tick timing and source metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedNote {
    pub onset_tick: u64,
    pub offset_tick: u64,
    pub pitch: u8,
    pub velocity: u8,
    pub channel: u8,
    pub track_index: usize,
}

impl TimedNote {
    pub fn duration_ticks(&self) -> u64 {
        self.offset_tick.saturating_sub(self.onset_tick)
    }
}

/// How a voice was separated from its source material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeparationMethod {
    /// Track was already monophonic, passed through unchanged
    AlreadyMonophonic,
    /// Separated by MIDI channel (format-0 multi-channel tracks)
    ChannelSplit,
    /// Chew & Wu nearest-neighbor pitch contiguity
    PitchContiguity,
    /// Highest note at each onset (melody extraction)
    Skyline,
    /// Lowest note at each onset (bass extraction)
    Bassline,
}

/// Statistics about a separated voice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceStats {
    pub note_count: usize,
    pub pitch_min: u8,
    pub pitch_max: u8,
    pub mean_pitch: f64,
    /// Fraction of the voice's time span covered by notes (0.0–1.0)
    pub coverage: f64,
}

impl VoiceStats {
    pub fn from_notes(notes: &[TimedNote]) -> Self {
        if notes.is_empty() {
            return Self {
                note_count: 0,
                pitch_min: 0,
                pitch_max: 0,
                mean_pitch: 0.0,
                coverage: 0.0,
            };
        }

        let pitch_min = notes.iter().map(|n| n.pitch).min().unwrap_or(0);
        let pitch_max = notes.iter().map(|n| n.pitch).max().unwrap_or(0);
        let mean_pitch =
            notes.iter().map(|n| n.pitch as f64).sum::<f64>() / notes.len() as f64;

        let first_onset = notes.iter().map(|n| n.onset_tick).min().unwrap_or(0);
        let last_offset = notes.iter().map(|n| n.offset_tick).max().unwrap_or(0);
        let span = last_offset.saturating_sub(first_onset);

        let sounding_ticks: u64 = notes.iter().map(|n| n.duration_ticks()).sum();
        let coverage = if span > 0 {
            (sounding_ticks as f64 / span as f64).min(1.0)
        } else {
            0.0
        };

        Self {
            note_count: notes.len(),
            pitch_min,
            pitch_max,
            mean_pitch,
            coverage,
        }
    }
}

/// A separated musical voice with its notes and provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeparatedVoice {
    pub notes: Vec<TimedNote>,
    pub method: SeparationMethod,
    pub voice_index: usize,
    pub stats: VoiceStats,
    pub source_channel: Option<u8>,
    pub source_track: Option<usize>,
}
