use midi_analysis::{MidiFileContext, SeparatedVoice, TimedNote, TrackProfile, VoiceRole};

use crate::chords::extract_chords;
use crate::key::detect_key;
use crate::meter::detect_meter;
use crate::types::{ChordEvent, ClassifiedVoice, KeyDetection, MeterDetection};

/// Trait for music analysis backends.
///
/// MVP: `HeuristicAnalyzer` using ported Python algorithms.
/// Future: `LearnedAnalyzer` calling GNN/transformer via ZMQ,
/// falling back to heuristic on timeout.
pub trait MusicAnalyzer: Send + Sync {
    fn analyze_key(&self, notes: &[TimedNote], context: &MidiFileContext) -> KeyDetection;

    fn analyze_meter(&self, notes: &[TimedNote], context: &MidiFileContext) -> MeterDetection;

    fn extract_chords(
        &self,
        harmony_notes: &[TimedNote],
        bass_notes: &[TimedNote],
        context: &MidiFileContext,
        key: &KeyDetection,
    ) -> Vec<ChordEvent>;

    fn classify_voices(
        &self,
        voices: &[SeparatedVoice],
        context: &MidiFileContext,
        track_profiles: &[TrackProfile],
    ) -> Vec<ClassifiedVoice>;
}

/// Heuristic analyzer using Krumhansl-Schmuckler key detection,
/// onset histogram meter detection, and template-matching chord extraction.
pub struct HeuristicAnalyzer;

impl MusicAnalyzer for HeuristicAnalyzer {
    fn analyze_key(&self, notes: &[TimedNote], context: &MidiFileContext) -> KeyDetection {
        detect_key(notes, context)
    }

    fn analyze_meter(&self, notes: &[TimedNote], context: &MidiFileContext) -> MeterDetection {
        detect_meter(notes, context)
    }

    fn extract_chords(
        &self,
        harmony_notes: &[TimedNote],
        bass_notes: &[TimedNote],
        context: &MidiFileContext,
        key: &KeyDetection,
    ) -> Vec<ChordEvent> {
        extract_chords(harmony_notes, bass_notes, context, key)
    }

    fn classify_voices(
        &self,
        voices: &[SeparatedVoice],
        context: &MidiFileContext,
        track_profiles: &[TrackProfile],
    ) -> Vec<ClassifiedVoice> {
        let classifications = midi_analysis::classify_voices(voices, context, track_profiles);

        classifications
            .into_iter()
            .zip(voices.iter())
            .map(|(cls, voice)| ClassifiedVoice {
                voice_index: cls.voice_index,
                role: cls.role,
                confidence: cls.confidence,
                notes: voice.notes.clone(),
                features: cls.features,
            })
            .collect()
    }
}

/// Partition classified voices into harmony and bass note sets.
///
/// Harmony = Melody, Countermelody, HarmonicFill, PrimaryHarmony, SecondaryHarmony, Padding.
/// Bass = Bass role.
pub fn partition_voices(voices: &[ClassifiedVoice]) -> (Vec<&TimedNote>, Vec<&TimedNote>) {
    let mut harmony = Vec::new();
    let mut bass = Vec::new();

    for voice in voices {
        match voice.role {
            VoiceRole::Bass => {
                bass.extend(voice.notes.iter());
            }
            VoiceRole::Percussion | VoiceRole::Rhythm => {
                // Skip percussion for chord extraction
            }
            _ => {
                harmony.extend(voice.notes.iter());
            }
        }
    }

    (harmony, bass)
}
