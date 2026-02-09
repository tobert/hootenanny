use crate::analyze::{MidiFileContext, TrackProfile};
use crate::note::{SeparatedVoice, TimedNote};
use serde::{Deserialize, Serialize};

/// Musical role a voice plays within an ensemble.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceRole {
    Melody,
    Bass,
    Countermelody,
    HarmonicFill,
    Percussion,
    Rhythm,
    PrimaryHarmony,
    SecondaryHarmony,
    Padding,
}

impl VoiceRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Melody => "melody",
            Self::Bass => "bass",
            Self::Countermelody => "countermelody",
            Self::HarmonicFill => "harmonic_fill",
            Self::Percussion => "percussion",
            Self::Rhythm => "rhythm",
            Self::PrimaryHarmony => "primary_harmony",
            Self::SecondaryHarmony => "secondary_harmony",
            Self::Padding => "padding",
        }
    }
}

impl std::fmt::Display for VoiceRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Numeric feature vector extracted from a voice in context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceFeatures {
    // Register
    pub mean_pitch_normalized: f64,
    pub pitch_min: u8,
    pub pitch_max: u8,
    pub pitch_range_semitones: u8,
    pub pitch_std_dev: f64,

    // Temporal
    pub coverage: f64,
    pub notes_per_beat: f64,
    pub mean_ioi_beats: f64,
    pub ioi_std_dev_beats: f64,
    pub mean_duration_beats: f64,

    // Rhythmic
    pub on_beat_fraction: f64,
    pub on_downbeat_fraction: f64,

    // Velocity
    pub mean_velocity: f64,
    pub velocity_std_dev: f64,
    pub velocity_range: u8,

    // Polyphony (within voice)
    pub max_simultaneous: usize,
    pub polyphonic_fraction: f64,

    // Instrument hint
    pub gm_program_category: u8,
    pub is_drum_channel: bool,

    // Comparative (vs sibling voices)
    pub pitch_rank_normalized: f64,
    pub is_highest_voice: bool,
    pub is_lowest_voice: bool,
    pub coverage_rank_normalized: f64,
}

/// How a voice was classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationMethod {
    Heuristic,
    MachineLearning,
}

/// A classified voice with its role, confidence, features, and alternatives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceClassification {
    pub voice_index: usize,
    pub role: VoiceRole,
    pub confidence: f64,
    pub method: ClassificationMethod,
    pub features: VoiceFeatures,
    pub alternative_roles: Vec<(VoiceRole, f64)>,
}

/// Extract features from a single voice in the context of all sibling voices.
pub fn extract_features(
    voice: &SeparatedVoice,
    all_voices: &[SeparatedVoice],
    context: &MidiFileContext,
    track_profiles: &[TrackProfile],
) -> VoiceFeatures {
    let notes = &voice.notes;
    let ppq = context.ppq as f64;

    // Beats per measure from first time signature (default 4/4)
    let beats_per_measure = context
        .time_signatures
        .first()
        .map(|ts| ts.numerator as f64)
        .unwrap_or(4.0);

    let total_beats = if context.total_ticks > 0 {
        context.total_ticks as f64 / ppq
    } else {
        1.0
    };

    // --- Register ---

    let mean_pitch = voice.stats.mean_pitch;
    let mean_pitch_normalized = mean_pitch / 127.0;
    let pitch_min = voice.stats.pitch_min;
    let pitch_max = voice.stats.pitch_max;
    let pitch_range_semitones = pitch_max.saturating_sub(pitch_min);

    let pitch_std_dev = if notes.len() > 1 {
        let variance = notes
            .iter()
            .map(|n| {
                let diff = n.pitch as f64 - mean_pitch;
                diff * diff
            })
            .sum::<f64>()
            / notes.len() as f64;
        variance.sqrt()
    } else {
        0.0
    };

    // --- Temporal ---

    let coverage = voice.stats.coverage;
    let notes_per_beat = if total_beats > 0.0 {
        notes.len() as f64 / total_beats
    } else {
        0.0
    };

    let (mean_ioi_beats, ioi_std_dev_beats) = compute_ioi_stats(notes, ppq);

    let mean_duration_beats = if !notes.is_empty() {
        let total_dur: f64 = notes.iter().map(|n| n.duration_ticks() as f64 / ppq).sum();
        total_dur / notes.len() as f64
    } else {
        0.0
    };

    // --- Rhythmic ---

    let beat_tolerance = ppq / 8.0;
    let measure_ticks = ppq * beats_per_measure;

    let (on_beat_count, on_downbeat_count) = notes.iter().fold((0usize, 0usize), |(beat, down), n| {
        let onset = n.onset_tick as f64;
        let on_beat = (onset % ppq) < beat_tolerance || (ppq - (onset % ppq)) < beat_tolerance;
        let on_down = (onset % measure_ticks) < beat_tolerance
            || (measure_ticks - (onset % measure_ticks)) < beat_tolerance;
        (beat + on_beat as usize, down + on_down as usize)
    });

    let on_beat_fraction = if !notes.is_empty() {
        on_beat_count as f64 / notes.len() as f64
    } else {
        0.0
    };
    let on_downbeat_fraction = if !notes.is_empty() {
        on_downbeat_count as f64 / notes.len() as f64
    } else {
        0.0
    };

    // --- Velocity ---

    let (mean_velocity, velocity_std_dev, velocity_range) = compute_velocity_stats(notes);

    // --- Polyphony within voice ---

    let (max_simultaneous, polyphonic_fraction) = compute_voice_polyphony(notes);

    // --- Instrument hint ---

    let (gm_program_category, is_drum_channel) =
        instrument_hints(voice, track_profiles);

    // --- Comparative ranks ---

    let mean_pitches: Vec<f64> = all_voices.iter().map(|v| v.stats.mean_pitch).collect();
    let coverages: Vec<f64> = all_voices.iter().map(|v| v.stats.coverage).collect();

    let pitch_rank_normalized = rank_normalized(mean_pitch, &mean_pitches);
    let coverage_rank_normalized = rank_normalized(coverage, &coverages);

    let is_highest_voice = mean_pitches
        .iter()
        .all(|&p| p <= mean_pitch + f64::EPSILON);
    let is_lowest_voice = mean_pitches
        .iter()
        .all(|&p| p >= mean_pitch - f64::EPSILON);

    VoiceFeatures {
        mean_pitch_normalized,
        pitch_min,
        pitch_max,
        pitch_range_semitones,
        pitch_std_dev,
        coverage,
        notes_per_beat,
        mean_ioi_beats,
        ioi_std_dev_beats,
        mean_duration_beats,
        on_beat_fraction,
        on_downbeat_fraction,
        mean_velocity,
        velocity_std_dev,
        velocity_range,
        max_simultaneous,
        polyphonic_fraction,
        gm_program_category,
        is_drum_channel,
        pitch_rank_normalized,
        is_highest_voice,
        is_lowest_voice,
        coverage_rank_normalized,
    }
}

/// Heuristic rule-based classification for a single voice.
///
/// Returns (role, confidence, alternative_roles) using a priority-ordered
/// decision tree based on musical knowledge.
pub fn classify_heuristic(features: &VoiceFeatures) -> (VoiceRole, f64, Vec<(VoiceRole, f64)>) {
    let mut candidates: Vec<(VoiceRole, f64)> = Vec::new();

    // Rule 1: Percussion channel or GM percussion category
    if features.is_drum_channel || features.gm_program_category == 14 {
        candidates.push((VoiceRole::Percussion, 0.95));
    }

    // Rule 2: GM bass program (category 4 = programs 32-39)
    if features.gm_program_category == 4 {
        candidates.push((VoiceRole::Bass, 0.85));
    }

    // Rule 3: Lowest voice with low pitch + decent coverage
    if features.is_lowest_voice
        && features.mean_pitch_normalized < 0.378 // ~C3
        && features.coverage > 0.15
    {
        candidates.push((VoiceRole::Bass, 0.75));
    }

    // Rule 4: Highest voice with good coverage + activity
    if features.is_highest_voice
        && features.coverage > 0.3
        && features.notes_per_beat > 0.5
    {
        candidates.push((VoiceRole::Melody, 0.70));
    }

    // Rule 5: Rhythmic - steady IOI, narrow pitch range, good coverage
    if features.coverage > 0.4
        && features.ioi_std_dev_beats < 0.2
        && features.pitch_range_semitones <= 7
    {
        candidates.push((VoiceRole::Rhythm, 0.65));
    }

    // Rule 6: Countermelody - mid-high register, active, not the top voice
    if features.pitch_rank_normalized > 0.5
        && features.coverage > 0.2
        && !features.is_highest_voice
        && features.notes_per_beat > 0.3
    {
        candidates.push((VoiceRole::Countermelody, 0.55));
    }

    // Rule 7: Primary harmony - polyphonic with 3+ simultaneous
    if features.polyphonic_fraction > 0.3 && features.max_simultaneous >= 3 {
        candidates.push((VoiceRole::PrimaryHarmony, 0.60));
    }

    // Rule 8: Secondary harmony - some polyphony, lower coverage
    if features.polyphonic_fraction > 0.1
        && features.max_simultaneous >= 2
        && features.coverage < 0.5
    {
        candidates.push((VoiceRole::SecondaryHarmony, 0.50));
    }

    // Rule 9: Padding - sparse and slow
    if features.coverage < 0.15 && features.notes_per_beat < 0.3 {
        candidates.push((VoiceRole::Padding, 0.45));
    }

    // Rule 10: Default
    if candidates.is_empty() {
        candidates.push((VoiceRole::HarmonicFill, 0.35));
    }

    // Sort by confidence descending
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (role, confidence) = candidates[0];
    let alternative_roles = candidates[1..].to_vec();

    (role, confidence, alternative_roles)
}

/// Classify all voices with conflict resolution.
///
/// Ensures at most one Melody and one Bass per ensemble,
/// demoting lower-confidence duplicates.
pub fn classify_voices(
    voices: &[SeparatedVoice],
    context: &MidiFileContext,
    track_profiles: &[TrackProfile],
) -> Vec<VoiceClassification> {
    if voices.is_empty() {
        return Vec::new();
    }

    // Extract features for all voices
    let features: Vec<VoiceFeatures> = voices
        .iter()
        .map(|v| extract_features(v, voices, context, track_profiles))
        .collect();

    // Initial classification
    let mut classifications: Vec<VoiceClassification> = features
        .into_iter()
        .enumerate()
        .map(|(i, feat)| {
            let (role, confidence, alternatives) = classify_heuristic(&feat);
            VoiceClassification {
                voice_index: i,
                role,
                confidence,
                method: ClassificationMethod::Heuristic,
                features: feat,
                alternative_roles: alternatives,
            }
        })
        .collect();

    // Conflict resolution: only one Melody
    resolve_unique_role(&mut classifications, VoiceRole::Melody, VoiceRole::Countermelody);

    // Conflict resolution: only one Bass
    resolve_unique_role(&mut classifications, VoiceRole::Bass, VoiceRole::HarmonicFill);

    classifications
}

/// Ensure at most one voice has the given role.
/// If duplicates exist, keep the highest confidence and demote others.
fn resolve_unique_role(
    classifications: &mut [VoiceClassification],
    target_role: VoiceRole,
    demotion_role: VoiceRole,
) {
    let claimants: Vec<usize> = classifications
        .iter()
        .enumerate()
        .filter(|(_, c)| c.role == target_role)
        .map(|(i, _)| i)
        .collect();

    if claimants.len() <= 1 {
        return;
    }

    // Find the best claimant by confidence
    let best = *claimants
        .iter()
        .max_by(|&&a, &&b| {
            classifications[a]
                .confidence
                .partial_cmp(&classifications[b].confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("claimants is non-empty");

    // Demote all others
    for &idx in &claimants {
        if idx != best {
            let old_role = classifications[idx].role;
            let old_conf = classifications[idx].confidence;
            classifications[idx].role = demotion_role;
            classifications[idx].confidence = (old_conf * 0.8).min(0.50);
            classifications[idx]
                .alternative_roles
                .insert(0, (old_role, old_conf));
        }
    }
}

// --- Internal helpers ---

fn compute_ioi_stats(notes: &[TimedNote], ppq: f64) -> (f64, f64) {
    if notes.len() < 2 {
        return (0.0, 0.0);
    }

    let mut onsets: Vec<u64> = notes.iter().map(|n| n.onset_tick).collect();
    onsets.sort_unstable();
    onsets.dedup();

    if onsets.len() < 2 {
        return (0.0, 0.0);
    }

    let iois: Vec<f64> = onsets
        .windows(2)
        .map(|w| (w[1] - w[0]) as f64 / ppq)
        .collect();

    let mean = iois.iter().sum::<f64>() / iois.len() as f64;
    let variance = iois.iter().map(|&x| (x - mean) * (x - mean)).sum::<f64>() / iois.len() as f64;

    (mean, variance.sqrt())
}

fn compute_velocity_stats(notes: &[TimedNote]) -> (f64, f64, u8) {
    if notes.is_empty() {
        return (0.0, 0.0, 0);
    }

    let mean = notes.iter().map(|n| n.velocity as f64).sum::<f64>() / notes.len() as f64;

    let std_dev = if notes.len() > 1 {
        let variance = notes
            .iter()
            .map(|n| {
                let diff = n.velocity as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / notes.len() as f64;
        variance.sqrt()
    } else {
        0.0
    };

    let vel_min = notes.iter().map(|n| n.velocity).min().unwrap_or(0);
    let vel_max = notes.iter().map(|n| n.velocity).max().unwrap_or(0);

    (mean, std_dev, vel_max.saturating_sub(vel_min))
}

fn compute_voice_polyphony(notes: &[TimedNote]) -> (usize, f64) {
    if notes.is_empty() {
        return (0, 0.0);
    }

    let mut events: Vec<(u64, i32)> = Vec::with_capacity(notes.len() * 2);
    for note in notes {
        events.push((note.onset_tick, 1));
        events.push((note.offset_tick, -1));
    }
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut current = 0i32;
    let mut max_sim = 0usize;
    let mut polyphonic_onsets = 0usize;
    let mut total_onsets = 0usize;

    let onset_set: std::collections::HashSet<u64> =
        notes.iter().map(|n| n.onset_tick).collect();

    for &(tick, delta) in &events {
        current += delta;
        let sim = current.max(0) as usize;
        max_sim = max_sim.max(sim);

        if onset_set.contains(&tick) && delta > 0 {
            total_onsets += 1;
            if sim > 1 {
                polyphonic_onsets += 1;
            }
        }
    }

    let fraction = if total_onsets > 0 {
        polyphonic_onsets as f64 / total_onsets as f64
    } else {
        0.0
    };

    (max_sim, fraction)
}

fn instrument_hints(voice: &SeparatedVoice, track_profiles: &[TrackProfile]) -> (u8, bool) {
    let track_idx = voice.source_track.unwrap_or(0);
    let channel = voice.source_channel.unwrap_or(0);

    let is_drum = channel == 9
        || track_profiles
            .get(track_idx)
            .is_some_and(|tp| tp.is_percussion);

    let category = track_profiles
        .get(track_idx)
        .and_then(|tp| tp.programs_used.first())
        .map(|&p| p / 8)
        .unwrap_or(0);

    (category, is_drum)
}

/// Compute the normalized rank of a value within a set (0.0 = lowest, 1.0 = highest).
fn rank_normalized(value: f64, all_values: &[f64]) -> f64 {
    if all_values.len() <= 1 {
        return 0.5;
    }

    let below = all_values
        .iter()
        .filter(|&&v| v < value - f64::EPSILON)
        .count();

    below as f64 / (all_values.len() - 1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::TimeSignature;
    use crate::note::{SeparationMethod, VoiceStats};

    fn make_context(ppq: u16, total_ticks: u64) -> MidiFileContext {
        MidiFileContext {
            ppq,
            format: 1,
            track_count: 2,
            tempo_changes: vec![],
            time_signatures: vec![TimeSignature {
                tick: 0,
                numerator: 4,
                denominator: 4,
            }],
            total_ticks,
        }
    }

    fn make_notes(specs: &[(u64, u64, u8, u8)]) -> Vec<TimedNote> {
        specs
            .iter()
            .map(|&(onset, offset, pitch, channel)| TimedNote {
                onset_tick: onset,
                offset_tick: offset,
                pitch,
                velocity: 100,
                channel,
                track_index: 0,
            })
            .collect()
    }

    fn make_voice(notes: Vec<TimedNote>, index: usize) -> SeparatedVoice {
        SeparatedVoice {
            stats: VoiceStats::from_notes(&notes),
            notes,
            method: SeparationMethod::PitchContiguity,
            voice_index: index,
            source_channel: Some(0),
            source_track: Some(1),
        }
    }

    fn make_track_profile(track_index: usize, programs: Vec<u8>, is_percussion: bool) -> TrackProfile {
        use crate::analyze::{DensityProfile, PitchRange, PolyphonyProfile};
        TrackProfile {
            track_index,
            name: None,
            instrument: None,
            programs_used: programs,
            channels_used: vec![0],
            is_percussion,
            note_count: 10,
            pitch_range: PitchRange::default(),
            polyphony: PolyphonyProfile::default(),
            density: DensityProfile::default(),
            merged_voices_likely: false,
        }
    }

    #[test]
    fn percussion_detected_by_drum_channel() {
        let notes = make_notes(&[
            (0, 240, 36, 9),
            (480, 720, 38, 9),
            (960, 1200, 42, 9),
        ]);
        let mut voice = make_voice(notes, 0);
        voice.source_channel = Some(9);

        let context = make_context(480, 1920);
        let profiles = vec![make_track_profile(0, vec![], true)];

        let features = extract_features(&voice, &[voice.clone()], &context, &profiles);
        assert!(features.is_drum_channel);

        let (role, confidence, _) = classify_heuristic(&features);
        assert_eq!(role, VoiceRole::Percussion);
        assert!(confidence >= 0.9);
    }

    #[test]
    fn bass_detected_by_gm_program() {
        let notes = make_notes(&[
            (0, 480, 36, 0),
            (480, 960, 40, 0),
            (960, 1440, 43, 0),
        ]);
        let voice = make_voice(notes, 0);
        let context = make_context(480, 1920);
        // GM program 33 = Electric Bass (finger), category = 33/8 = 4
        let profiles = vec![
            make_track_profile(0, vec![], false),
            make_track_profile(1, vec![33], false),
        ];

        let features = extract_features(&voice, &[voice.clone()], &context, &profiles);
        assert_eq!(features.gm_program_category, 4);

        let (role, confidence, _) = classify_heuristic(&features);
        assert_eq!(role, VoiceRole::Bass);
        assert!(confidence >= 0.8);
    }

    #[test]
    fn melody_detected_as_highest_active_voice() {
        // High voice with good coverage
        let melody_notes = make_notes(&[
            (0, 480, 72, 0),
            (480, 960, 76, 0),
            (960, 1440, 79, 0),
            (1440, 1920, 84, 0),
        ]);
        let melody = make_voice(melody_notes, 0);

        // Low accompanying voice
        let bass_notes = make_notes(&[
            (0, 960, 36, 0),
            (960, 1920, 40, 0),
        ]);
        let bass = make_voice(bass_notes, 1);

        let all_voices = vec![melody.clone(), bass.clone()];
        let context = make_context(480, 1920);
        let profiles = vec![
            make_track_profile(0, vec![], false),
            make_track_profile(1, vec![0], false),
        ];

        let features = extract_features(&melody, &all_voices, &context, &profiles);
        assert!(features.is_highest_voice);
        assert!(features.coverage > 0.3);

        let (role, _, _) = classify_heuristic(&features);
        assert_eq!(role, VoiceRole::Melody);
    }

    #[test]
    fn bass_detected_as_lowest_voice() {
        let melody_notes = make_notes(&[
            (0, 480, 72, 0),
            (480, 960, 76, 0),
            (960, 1440, 79, 0),
            (1440, 1920, 84, 0),
        ]);
        let melody = make_voice(melody_notes, 0);

        let bass_notes = make_notes(&[
            (0, 480, 36, 0),
            (480, 960, 40, 0),
            (960, 1440, 36, 0),
            (1440, 1920, 43, 0),
        ]);
        let bass = make_voice(bass_notes, 1);

        let all_voices = vec![melody.clone(), bass.clone()];
        let context = make_context(480, 1920);
        let profiles = vec![
            make_track_profile(0, vec![], false),
            make_track_profile(1, vec![0], false),
        ];

        let features = extract_features(&bass, &all_voices, &context, &profiles);
        assert!(features.is_lowest_voice);
        assert!(features.mean_pitch_normalized < 0.378);

        let (role, _, _) = classify_heuristic(&features);
        assert_eq!(role, VoiceRole::Bass);
    }

    #[test]
    fn conflict_resolution_single_melody() {
        // Two high voices that both want to be melody
        let voice1_notes = make_notes(&[
            (0, 480, 72, 0),
            (480, 960, 76, 0),
            (960, 1440, 79, 0),
            (1440, 1920, 84, 0),
        ]);
        let voice2_notes = make_notes(&[
            (0, 480, 71, 0),
            (480, 960, 74, 0),
            (960, 1440, 77, 0),
            (1440, 1920, 82, 0),
        ]);

        let voice1 = make_voice(voice1_notes, 0);
        let voice2 = make_voice(voice2_notes, 1);

        let context = make_context(480, 1920);
        let profiles = vec![
            make_track_profile(0, vec![], false),
            make_track_profile(1, vec![0], false),
        ];

        let classifications = classify_voices(&[voice1, voice2], &context, &profiles);

        let melody_count = classifications
            .iter()
            .filter(|c| c.role == VoiceRole::Melody)
            .count();
        assert!(melody_count <= 1, "Expected at most 1 melody, got {}", melody_count);
    }

    #[test]
    fn conflict_resolution_single_bass() {
        // Two low voices
        let voice1_notes = make_notes(&[
            (0, 480, 36, 0),
            (480, 960, 40, 0),
            (960, 1440, 43, 0),
            (1440, 1920, 36, 0),
        ]);
        let voice2_notes = make_notes(&[
            (0, 480, 38, 0),
            (480, 960, 41, 0),
            (960, 1440, 45, 0),
            (1440, 1920, 38, 0),
        ]);

        // Need a high voice so these appear as lowest
        let melody_notes = make_notes(&[
            (0, 480, 72, 0),
            (480, 960, 76, 0),
            (960, 1440, 79, 0),
            (1440, 1920, 84, 0),
        ]);

        let voice1 = make_voice(voice1_notes, 0);
        let voice2 = make_voice(voice2_notes, 1);
        let melody = make_voice(melody_notes, 2);

        let context = make_context(480, 1920);
        let profiles = vec![
            make_track_profile(0, vec![], false),
            make_track_profile(1, vec![33], false),
            make_track_profile(1, vec![0], false),
        ];

        let classifications = classify_voices(&[voice1, voice2, melody], &context, &profiles);

        let bass_count = classifications
            .iter()
            .filter(|c| c.role == VoiceRole::Bass)
            .count();
        assert!(bass_count <= 1, "Expected at most 1 bass, got {}", bass_count);
    }

    #[test]
    fn empty_voices_returns_empty() {
        let context = make_context(480, 1920);
        let profiles = vec![];
        let result = classify_voices(&[], &context, &profiles);
        assert!(result.is_empty());
    }

    #[test]
    fn single_voice_gets_classified() {
        let notes = make_notes(&[
            (0, 480, 60, 0),
            (480, 960, 64, 0),
            (960, 1440, 67, 0),
        ]);
        let voice = make_voice(notes, 0);

        let context = make_context(480, 1920);
        let profiles = vec![
            make_track_profile(0, vec![], false),
            make_track_profile(1, vec![0], false),
        ];

        let result = classify_voices(&[voice], &context, &profiles);
        assert_eq!(result.len(), 1);
        assert!(result[0].confidence > 0.0);
    }

    #[test]
    fn feature_extraction_ioi_stats() {
        // Evenly spaced notes: IOI should be consistent
        let notes = make_notes(&[
            (0, 240, 60, 0),
            (480, 720, 64, 0),
            (960, 1200, 67, 0),
            (1440, 1680, 72, 0),
        ]);
        let voice = make_voice(notes, 0);
        let context = make_context(480, 1920);
        let profiles = vec![
            make_track_profile(0, vec![], false),
            make_track_profile(1, vec![0], false),
        ];

        let features = extract_features(&voice, &[voice.clone()], &context, &profiles);

        // IOI = 480 ticks / 480 ppq = 1.0 beat
        assert!((features.mean_ioi_beats - 1.0).abs() < 0.01);
        // Perfectly even → std dev near 0
        assert!(features.ioi_std_dev_beats < 0.01);
    }

    #[test]
    fn feature_extraction_on_beat_fraction() {
        // All notes start on beat boundaries
        let notes = make_notes(&[
            (0, 240, 60, 0),
            (480, 720, 64, 0),
            (960, 1200, 67, 0),
        ]);
        let voice = make_voice(notes, 0);
        let context = make_context(480, 1920);
        let profiles = vec![
            make_track_profile(0, vec![], false),
            make_track_profile(1, vec![0], false),
        ];

        let features = extract_features(&voice, &[voice.clone()], &context, &profiles);
        assert!((features.on_beat_fraction - 1.0).abs() < 0.01);
    }

    #[test]
    fn all_percussion_ensemble() {
        let kick = make_notes(&[(0, 240, 36, 9), (960, 1200, 36, 9)]);
        let snare = make_notes(&[(480, 720, 38, 9), (1440, 1680, 38, 9)]);

        let mut v1 = make_voice(kick, 0);
        v1.source_channel = Some(9);
        let mut v2 = make_voice(snare, 1);
        v2.source_channel = Some(9);

        let context = make_context(480, 1920);
        let profiles = vec![
            make_track_profile(0, vec![], true),
            make_track_profile(1, vec![], true),
        ];

        let result = classify_voices(&[v1, v2], &context, &profiles);
        assert!(result.iter().all(|c| c.role == VoiceRole::Percussion));
    }

    #[test]
    fn rank_normalized_correct() {
        assert!((rank_normalized(1.0, &[1.0, 2.0, 3.0]) - 0.0).abs() < 0.01);
        assert!((rank_normalized(2.0, &[1.0, 2.0, 3.0]) - 0.5).abs() < 0.01);
        assert!((rank_normalized(3.0, &[1.0, 2.0, 3.0]) - 1.0).abs() < 0.01);
        assert!((rank_normalized(5.0, &[5.0]) - 0.5).abs() < 0.01);
    }
}
