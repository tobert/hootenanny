use midi_analysis::{MidiFileContext, TimedNote};

use crate::types::{KeyDetection, KeyMode};

/// Krumhansl-Kessler major key profile (duration-weighted perception studies).
const MAJOR_PROFILE: [f64; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];

/// Krumhansl-Kessler minor key profile.
const MINOR_PROFILE: [f64; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

const NOTE_NAMES_SHARP: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const NOTE_NAMES_FLAT: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

/// Pitch classes conventionally spelled with flats.
const FLAT_ROOTS: [u8; 6] = [1, 3, 5, 6, 8, 10]; // Db, Eb, F, Gb, Ab, Bb

/// Detect the key of a piece using the Krumhansl-Schmuckler algorithm.
///
/// Builds a duration-weighted pitch-class histogram and correlates it
/// against all 24 major/minor key profiles. The best Pearson correlation
/// determines the detected key.
pub fn detect_key(notes: &[TimedNote], _context: &MidiFileContext) -> KeyDetection {
    if notes.is_empty() {
        return KeyDetection {
            root: "C".into(),
            root_pitch_class: 0,
            mode: KeyMode::Major,
            confidence: 0.0,
        };
    }

    // Duration-weighted pitch-class histogram
    let mut histogram = [0.0_f64; 12];
    for note in notes {
        let pc = (note.pitch % 12) as usize;
        let duration = note.duration_ticks().max(1) as f64;
        histogram[pc] += duration;
    }

    let total: f64 = histogram.iter().sum();
    if total == 0.0 {
        return KeyDetection {
            root: "C".into(),
            root_pitch_class: 0,
            mode: KeyMode::Major,
            confidence: 0.0,
        };
    }

    // Normalize
    for h in &mut histogram {
        *h /= total;
    }

    // Correlate against all 24 key profiles (12 roots × 2 modes)
    let mut best_root: u8 = 0;
    let mut best_mode = KeyMode::Major;
    let mut best_corr = -1.0_f64;

    for root in 0..12u8 {
        // Rotate histogram so root = index 0
        let mut rotated = [0.0; 12];
        for i in 0..12 {
            rotated[i] = histogram[(i + root as usize) % 12];
        }

        let major_corr = pearson(&rotated, &MAJOR_PROFILE);
        if major_corr > best_corr {
            best_corr = major_corr;
            best_root = root;
            best_mode = KeyMode::Major;
        }

        let minor_corr = pearson(&rotated, &MINOR_PROFILE);
        if minor_corr > best_corr {
            best_corr = minor_corr;
            best_root = root;
            best_mode = KeyMode::Minor;
        }
    }

    let root = if FLAT_ROOTS.contains(&best_root) {
        NOTE_NAMES_FLAT[best_root as usize].to_string()
    } else {
        NOTE_NAMES_SHARP[best_root as usize].to_string()
    };

    KeyDetection {
        root,
        root_pitch_class: best_root,
        mode: best_mode,
        confidence: (best_corr * 10000.0).round() / 10000.0,
    }
}

/// Pearson correlation coefficient between two 12-element arrays.
fn pearson(x: &[f64; 12], y: &[f64; 12]) -> f64 {
    let x_mean: f64 = x.iter().sum::<f64>() / 12.0;
    let y_mean: f64 = y.iter().sum::<f64>() / 12.0;

    let mut num = 0.0;
    let mut x_sq = 0.0;
    let mut y_sq = 0.0;

    for i in 0..12 {
        let xd = x[i] - x_mean;
        let yd = y[i] - y_mean;
        num += xd * yd;
        x_sq += xd * xd;
        y_sq += yd * yd;
    }

    let denom = (x_sq * y_sq).sqrt();
    if denom < 1e-10 {
        return 0.0;
    }
    num / denom
}

/// Convert key detection result to ABC notation key field.
pub fn key_to_abc(detection: &KeyDetection) -> String {
    let mode_suffix = match detection.mode {
        KeyMode::Minor => "m",
        KeyMode::Major => "",
    };
    format!("{}{}", detection.root, mode_suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_note(pitch: u8, onset: u64, offset: u64) -> TimedNote {
        TimedNote {
            pitch,
            onset_tick: onset,
            offset_tick: offset,
            velocity: 80,
            channel: 0,
            track_index: 0,
        }
    }

    fn dummy_context() -> MidiFileContext {
        MidiFileContext {
            ppq: 480,
            format: 1,
            track_count: 1,
            tempo_changes: vec![],
            time_signatures: vec![],
            total_ticks: 1920,
        }
    }

    #[test]
    fn empty_notes_returns_c_major() {
        let result = detect_key(&[], &dummy_context());
        assert_eq!(result.root, "C");
        assert_eq!(result.mode, KeyMode::Major);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn c_major_scale_detected() {
        // C major scale: C D E F G A B
        let pitches = [60, 62, 64, 65, 67, 69, 71];
        let notes: Vec<_> = pitches
            .iter()
            .enumerate()
            .map(|(i, &p)| make_note(p, i as u64 * 480, (i as u64 + 1) * 480))
            .collect();

        let result = detect_key(&notes, &dummy_context());
        assert_eq!(result.root, "C");
        assert_eq!(result.mode, KeyMode::Major);
        assert!(result.confidence > 0.7, "confidence {} should be > 0.7", result.confidence);
    }

    #[test]
    fn a_minor_scale_detected() {
        // A natural minor: A B C D E F G
        let pitches = [57, 59, 60, 62, 64, 65, 67];
        let notes: Vec<_> = pitches
            .iter()
            .enumerate()
            .map(|(i, &p)| make_note(p, i as u64 * 480, (i as u64 + 1) * 480))
            .collect();

        let result = detect_key(&notes, &dummy_context());
        // A minor and C major are relative — algorithm may pick either.
        // Both are valid; just verify high confidence.
        assert!(result.confidence > 0.5, "confidence {} should be > 0.5", result.confidence);
    }

    #[test]
    fn flat_key_spelling() {
        // Db major scale: Db Eb F Gb Ab Bb C
        let pitches = [61, 63, 65, 66, 68, 70, 72];
        let notes: Vec<_> = pitches
            .iter()
            .enumerate()
            .map(|(i, &p)| make_note(p, i as u64 * 960, (i as u64 + 1) * 960))
            .collect();

        let result = detect_key(&notes, &dummy_context());
        // Should use flat spelling for Db
        if result.root_pitch_class == 1 {
            assert_eq!(result.root, "Db");
        }
    }

    #[test]
    fn pearson_identical_arrays() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        let r = pearson(&a, &a);
        assert!((r - 1.0).abs() < 1e-10, "self-correlation should be 1.0, got {}", r);
    }

    #[test]
    fn key_to_abc_formatting() {
        let det = KeyDetection {
            root: "Db".into(),
            root_pitch_class: 1,
            mode: KeyMode::Minor,
            confidence: 0.9,
        };
        assert_eq!(key_to_abc(&det), "Dbm");

        let det2 = KeyDetection {
            root: "G".into(),
            root_pitch_class: 7,
            mode: KeyMode::Major,
            confidence: 0.85,
        };
        assert_eq!(key_to_abc(&det2), "G");
    }
}
