// crates/resonode/src/lib.rs

use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::fmt;

// --- 1. Basic musical types ---

/// Represents a musical note with MIDI 2.0 fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Note {
    pub pitch: Pitch,
    pub velocity: Velocity,
    pub articulation: Articulation,
}

impl Note {
    pub fn new(pitch: Pitch, velocity: Velocity, articulation: Articulation) -> Self {
        Self {
            pitch,
            velocity,
            articulation,
        }
    }
}

/// Represents the pitch of a note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Pitch {
    pub frequency: f32, // Hz
    pub midi_note_number: u8, // 0-127
}

impl Pitch {
    pub fn new(midi_note_number: u8) -> Self {
        // Simple calculation for frequency, assuming A4 = 440Hz (MIDI note 69)
        let frequency = 440.0 * (2.0f32).powf((midi_note_number as f32 - 69.0) / 12.0);
        Self {
            frequency,
            midi_note_number,
        }
    }
}

/// Represents the duration of a musical event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Duration {
    Musical(MusicalDuration),
    Absolute(AbsoluteTime),
}

/// Represents musical duration (e.g., quarter note, eighth note).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum MusicalDuration {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    // Add more as needed
    Custom(f32), // e.g., 1.5 for dotted quarter
}

/// Represents velocity (loudness) using a 16-bit value for MIDI 2.0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Velocity(pub u16); // 0-65535

/// Represents the articulation of a note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Articulation {
    Legato,
    Staccato,
    Tenuto,
    Accent,
    Marcato,
    Sforzando,
    Fermata,
    // Add more as needed
    Custom(String),
}

// --- 2. Harmonic types ---

/// Represents a musical key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum Key {
    CMajor,
    CSharpMajor,
    DMajor,
    DSharpMajor,
    EMajor,
    FMajor,
    FSharpMajor,
    GMajor,
    GSharpMajor,
    AMajor,
    ASharpMajor,
    BMajor,
    AMinor,
    ASharpMinor,
    BMinor,
    CMinor,
    CSharpMinor,
    DMinor,
    DSharpMinor,
    EMinor,
    FMinor,
    FSharpMinor,
    GMinor,
    GSharpMinor,
    // Add more as needed
}

/// Represents a musical scale with interval patterns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Scale {
    pub root: Pitch,
    pub intervals: Vec<u8>, // Semitone intervals from the root
    pub name: String,
}

impl Scale {
    pub fn major(root_midi_note: u8) -> Self {
        Self {
            root: Pitch::new(root_midi_note),
            intervals: vec![0, 2, 4, 5, 7, 9, 11], // Major scale intervals
            name: "Major".to_string(),
        }
    }

    pub fn minor(root_midi_note: u8) -> Self {
        Self {
            root: Pitch::new(root_midi_note),
            intervals: vec![0, 2, 3, 5, 7, 8, 10], // Natural Minor scale intervals
            name: "Minor".to_string(),
        }
    }
}

/// Represents a musical chord.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Chord {
    pub root: Pitch,
    pub quality: ChordQuality,
    pub voicing: Vec<Pitch>, // Specific pitches in the chord
}

impl Chord {
    pub fn builder() -> ChordBuilder {
        ChordBuilder::new()
    }
}

/// Builder for `Chord`.
pub struct ChordBuilder {
    root: Option<Pitch>,
    quality: Option<ChordQuality>,
    voicing: Vec<Pitch>,
}

impl ChordBuilder {
    pub fn new() -> Self {
        Self {
            root: None,
            quality: None,
            voicing: Vec::new(),
        }
    }

    pub fn root(mut self, pitch: Pitch) -> Self {
        self.root = Some(pitch);
        self
    }

    pub fn quality(mut self, quality: ChordQuality) -> Self {
        self.quality = Some(quality);
        self
    }

    pub fn add_voicing_pitch(mut self, pitch: Pitch) -> Self {
        self.voicing.push(pitch);
        self
    }

    pub fn build(self) -> anyhow::Result<Chord> {
        let root = self.root.ok_or_else(|| anyhow::anyhow!("Chord root is required"))?;
        let quality = self.quality.ok_or_else(|| anyhow::anyhow!("Chord quality is required"))?;

        // If no specific voicing is provided, generate a default based on quality
        let voicing = if self.voicing.is_empty() {
            match quality {
                ChordQuality::Major => vec![
                    root.clone(),
                    Pitch::new(root.midi_note_number + 4),
                    Pitch::new(root.midi_note_number + 7),
                ],
                ChordQuality::Minor => vec![
                    root.clone(),
                    Pitch::new(root.midi_note_number + 3),
                    Pitch::new(root.midi_note_number + 7),
                ],
                ChordQuality::Diminished => vec![
                    root.clone(),
                    Pitch::new(root.midi_note_number + 3),
                    Pitch::new(root.midi_note_number + 6),
                ],
                ChordQuality::Augmented => vec![
                    root.clone(),
                    Pitch::new(root.midi_note_number + 4),
                    Pitch::new(root.midi_note_number + 8),
                ],
                ChordQuality::Dominant7 => vec![
                    root.clone(),
                    Pitch::new(root.midi_note_number + 4),
                    Pitch::new(root.midi_note_number + 7),
                    Pitch::new(root.midi_note_number + 10),
                ],
                // Add more default voicings as needed
                _ => vec![root.clone()], // Fallback
            }
        } else {
            self.voicing
        };

        Ok(Chord { root, quality, voicing })
    }
}

/// Represents the quality of a chord.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ChordQuality {
    Major,
    Minor,
    Diminished,
    Augmented,
    Dominant7,
    Major7,
    Minor7,
    Suspended2,
    Suspended4,
    // Add more as needed
    Custom(String),
}

// --- 3. Temporal types ---

/// Represents a time signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TimeSignature {
    pub numerator: u8,   // e.g., 4 in 4/4
    pub denominator: u8, // e.g., 4 in 4/4
}

/// Represents tempo in Beats Per Minute (BPM).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Tempo(pub f32); // e.g., 120.0 BPM

/// Represents musical time in bars:beats:ticks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, Eq, PartialOrd, Ord)]
pub struct MusicalTime {
    pub bar: u32,
    pub beat: u8,  // 1-based
    pub tick: u16, // 0-999, for sub-beat precision
}

impl MusicalTime {
    pub fn new(bar: u32, beat: u8, tick: u16) -> Self {
        Self { bar, beat, tick }
    }
}

/// Represents absolute time in milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbsoluteTime(pub u64); // Milliseconds

// --- Conversions between MusicalTime and AbsoluteTime ---
impl MusicalTime {
    /// Converts MusicalTime to AbsoluteTime given tempo and time signature.
    pub fn to_absolute_time(&self, tempo: &Tempo, time_signature: &TimeSignature) -> AbsoluteTime {
        // Assuming 1 beat = 1 quarter note for BPM calculation
        // Milliseconds per beat = 60,000 / BPM
        let ms_per_beat = 60_000.0 / tempo.0;

        // Number of beats per bar based on time signature (e.g., 4/4 has 4 quarter notes per bar)
        // This assumes the denominator represents quarter notes.
        let beats_per_bar = time_signature.numerator as f32 * (4.0 / time_signature.denominator as f32);

        let total_beats = (self.bar as f32 * beats_per_bar) + (self.beat as f32 - 1.0) + (self.tick as f32 / 1000.0);
        AbsoluteTime((total_beats * ms_per_beat) as u64)
    }
}

impl AbsoluteTime {
    /// Converts AbsoluteTime to MusicalTime given tempo and time signature.
    pub fn to_musical_time(&self, tempo: &Tempo, time_signature: &TimeSignature) -> MusicalTime {
        let ms_per_beat = 60_000.0 / tempo.0;
        let total_beats = self.0 as f32 / ms_per_beat;

        let beats_per_bar = time_signature.numerator as f32 * (4.0 / time_signature.denominator as f32);

        let bar = (total_beats / beats_per_bar).floor() as u32;
        let remaining_beats = total_beats % beats_per_bar;
        let beat = (remaining_beats.floor() + 1.0) as u8; // 1-based
        let tick = ((remaining_beats - (beat as f32 - 1.0)) * 1000.0).round() as u16;

        MusicalTime::new(bar, beat, tick)
    }
}

// --- Display implementations ---
impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Note(MIDI: {}, Freq: {:.2}Hz, Vel: {}, Art: {:?})",
            self.pitch.midi_note_number, self.pitch.frequency, self.velocity.0, self.articulation
        )
    }
}

impl fmt::Display for Pitch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pitch(MIDI: {}, Freq: {:.2}Hz)", self.midi_note_number, self.frequency)
    }
}

impl fmt::Display for Velocity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Velocity({})", self.0)
    }
}

impl fmt::Display for MusicalTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.bar, self.beat, self.tick)
    }
}

impl fmt::Display for AbsoluteTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

impl fmt::Display for Tempo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} BPM", self.0)
    }
}

impl fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let voicing_str: Vec<String> = self.voicing.iter().map(|p| p.to_string()).collect();
        write!(
            f,
            "Chord(Root: {}, Quality: {:?}, Voicing: [{}])",
            self.root,
            self.quality,
            voicing_str.join(", ")
        )
    }
}

impl fmt::Display for Scale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let intervals_str: Vec<String> = self.intervals.iter().map(|i| i.to_string()).collect();
        write!(
            f,
            "Scale(Name: {}, Root: {}, Intervals: [{}])",
            self.name,
            self.root,
            intervals_str.join(", ")
        )
    }
}

// Default implementation for ChordBuilder
impl Default for ChordBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pitch_frequency() {
        let a4 = Pitch::new(69);
        assert!((a4.frequency - 440.0).abs() < 0.01);

        let c5 = Pitch::new(72); // 3 semitones above A4
        assert!((c5.frequency - 523.25).abs() < 0.01);
    }

    #[test]
    fn test_note_creation() {
        let pitch = Pitch::new(60); // C4
        let velocity = Velocity(10000);
        let articulation = Articulation::Staccato;
        let note = Note::new(pitch.clone(), velocity.clone(), articulation.clone());

        assert_eq!(note.pitch, pitch);
        assert_eq!(note.velocity, velocity);
        assert_eq!(note.articulation, articulation);
    }

    #[test]
    fn test_scale_creation() {
        let c_major = Scale::major(60); // C4
        assert_eq!(c_major.name, "Major");
        assert_eq!(c_major.root, Pitch::new(60));
        assert_eq!(c_major.intervals, vec![0, 2, 4, 5, 7, 9, 11]);

        let a_minor = Scale::minor(57); // A3
        assert_eq!(a_minor.name, "Minor");
        assert_eq!(a_minor.root, Pitch::new(57));
        assert_eq!(a_minor.intervals, vec![0, 2, 3, 5, 7, 8, 10]);
    }

    #[test]
    fn test_chord_builder_major() {
        let c4 = Pitch::new(60);
        let chord = Chord::builder()
            .root(c4.clone())
            .quality(ChordQuality::Major)
            .build()
            .unwrap();

        assert_eq!(chord.root, c4);
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.voicing.len(), 3);
        assert_eq!(chord.voicing[0], Pitch::new(60)); // C4
        assert_eq!(chord.voicing[1], Pitch::new(64)); // E4
        assert_eq!(chord.voicing[2], Pitch::new(67)); // G4
    }

    #[test]
    fn test_chord_builder_custom_voicing() {
        let c4 = Pitch::new(60);
        let e4 = Pitch::new(64);
        let g4 = Pitch::new(67);
        let c5 = Pitch::new(72);

        let chord = Chord::builder()
            .root(c4.clone())
            .quality(ChordQuality::Major)
            .add_voicing_pitch(c4.clone())
            .add_voicing_pitch(e4.clone())
            .add_voicing_pitch(g4.clone())
            .add_voicing_pitch(c5.clone())
            .build()
            .unwrap();

        assert_eq!(chord.root, c4);
        assert_eq!(chord.quality, ChordQuality::Major);
        assert_eq!(chord.voicing.len(), 4);
        assert_eq!(chord.voicing[0], c4);
        assert_eq!(chord.voicing[1], e4);
        assert_eq!(chord.voicing[2], g4);
        assert_eq!(chord.voicing[3], c5);
    }

    #[test]
    fn test_musical_absolute_time_conversion() {
        let tempo = Tempo(120.0); // 120 BPM
        let time_signature = TimeSignature { numerator: 4, denominator: 4 }; // 4/4

        // 1 bar, 1 beat, 0 ticks (start of the first beat of the first bar)
        let mt1 = MusicalTime::new(0, 1, 0);
        let at1 = mt1.to_absolute_time(&tempo, &time_signature);
        // At 120 BPM, a quarter note (1 beat in 4/4) is 500ms.
        // Bar 0, Beat 1, Tick 0 should be 0ms.
        assert_eq!(at1, AbsoluteTime(0));

        // 0 bars, 2 beats, 0 ticks (start of the second beat of the first bar)
        let mt2 = MusicalTime::new(0, 2, 0);
        let at2 = mt2.to_absolute_time(&tempo, &time_signature);
        // 1 beat = 500ms
        assert_eq!(at2, AbsoluteTime(500));

        // 1 bar, 1 beat, 0 ticks (start of the first beat of the second bar)
        let mt3 = MusicalTime::new(1, 1, 0);
        let at3 = mt3.to_absolute_time(&tempo, &time_signature);
        // 1 bar (4 beats) = 2000ms
        assert_eq!(at3, AbsoluteTime(2000));

        // Test conversion back
        let converted_mt1 = at1.to_musical_time(&tempo, &time_signature);
        assert_eq!(converted_mt1, mt1);

        let converted_mt2 = at2.to_musical_time(&tempo, &time_signature);
        assert_eq!(converted_mt2, mt2);

        let converted_mt3 = at3.to_musical_time(&tempo, &time_signature);
        assert_eq!(converted_mt3, mt3);

        // Test with ticks
        let mt4 = MusicalTime::new(0, 1, 250); // 1/4 of a beat
        let at4 = mt4.to_absolute_time(&tempo, &time_signature);
        assert_eq!(at4, AbsoluteTime(125)); // 500ms * 0.25 = 125ms

        let converted_mt4 = at4.to_musical_time(&tempo, &time_signature);
        assert_eq!(converted_mt4, mt4);

        // Test with different time signature (3/4)
        let ts_3_4 = TimeSignature { numerator: 3, denominator: 4 };
        let mt_3_4_bar1_beat1 = MusicalTime::new(0, 1, 0);
        let at_3_4_bar1_beat1 = mt_3_4_bar1_beat1.to_absolute_time(&tempo, &ts_3_4);
        assert_eq!(at_3_4_bar1_beat1, AbsoluteTime(0));

        let mt_3_4_bar1_beat2 = MusicalTime::new(0, 2, 0);
        let at_3_4_bar1_beat2 = mt_3_4_bar1_beat2.to_absolute_time(&tempo, &ts_3_4);
        assert_eq!(at_3_4_bar1_beat2, AbsoluteTime(500));

        let mt_3_4_bar2_beat1 = MusicalTime::new(1, 1, 0);
        let at_3_4_bar2_beat1 = mt_3_4_bar2_beat1.to_absolute_time(&tempo, &ts_3_4);
        // 1 bar (3 beats) = 1500ms
        assert_eq!(at_3_4_bar2_beat1, AbsoluteTime(1500));

        let converted_mt_3_4_bar2_beat1 = at_3_4_bar2_beat1.to_musical_time(&tempo, &ts_3_4);
        assert_eq!(converted_mt_3_4_bar2_beat1, mt_3_4_bar2_beat1);
    }
}