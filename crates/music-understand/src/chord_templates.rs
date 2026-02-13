use crate::types::ChordQuality;

/// A chord template: quality enum + interval set from root (as bitmask over 12 pitch classes).
pub struct ChordTemplate {
    pub quality: ChordQuality,
    pub suffix: &'static str,
    pub intervals: u16, // bitmask: bit i set means interval i is in the template
    pub size: usize,
}

impl ChordTemplate {
    const fn new(quality: ChordQuality, suffix: &'static str, intervals: &[u8]) -> Self {
        let mut mask = 0u16;
        let mut i = 0;
        while i < intervals.len() {
            mask |= 1 << intervals[i];
            i += 1;
        }
        Self {
            quality,
            suffix,
            intervals: mask,
            size: intervals.len(),
        }
    }
}

/// All recognized chord templates, ordered by specificity (larger first for tiebreaking).
pub static TEMPLATES: &[ChordTemplate] = &[
    // 4-note chords first (more specific)
    ChordTemplate::new(ChordQuality::Dominant7, "7", &[0, 4, 7, 10]),
    ChordTemplate::new(ChordQuality::Major7, "maj7", &[0, 4, 7, 11]),
    ChordTemplate::new(ChordQuality::Minor7, "m7", &[0, 3, 7, 10]),
    ChordTemplate::new(ChordQuality::MinorMajor7, "m(maj7)", &[0, 3, 7, 11]),
    ChordTemplate::new(ChordQuality::Diminished7, "dim7", &[0, 3, 6, 9]),
    ChordTemplate::new(ChordQuality::HalfDiminished7, "m7b5", &[0, 3, 6, 10]),
    ChordTemplate::new(ChordQuality::Major6, "6", &[0, 4, 7, 9]),
    ChordTemplate::new(ChordQuality::Minor6, "m6", &[0, 3, 7, 9]),
    ChordTemplate::new(ChordQuality::Add9, "add9", &[0, 2, 4, 7]),
    // Triads
    ChordTemplate::new(ChordQuality::Major, "", &[0, 4, 7]),
    ChordTemplate::new(ChordQuality::Minor, "m", &[0, 3, 7]),
    ChordTemplate::new(ChordQuality::Diminished, "dim", &[0, 3, 6]),
    ChordTemplate::new(ChordQuality::Augmented, "aug", &[0, 4, 8]),
    ChordTemplate::new(ChordQuality::Suspended4, "sus4", &[0, 5, 7]),
    ChordTemplate::new(ChordQuality::Suspended2, "sus2", &[0, 2, 7]),
    // Dyad
    ChordTemplate::new(ChordQuality::Power, "5", &[0, 7]),
];

const NOTE_NAMES_SHARP: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const NOTE_NAMES_FLAT: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// Pitch classes conventionally spelled with flats.
pub static FLAT_KEY_ROOTS: [u8; 6] = [1, 3, 5, 6, 8, 10];

pub fn note_name(pitch_class: u8, use_flats: bool) -> &'static str {
    let idx = (pitch_class % 12) as usize;
    if use_flats {
        NOTE_NAMES_FLAT[idx]
    } else {
        NOTE_NAMES_SHARP[idx]
    }
}

/// Convert a set of pitch classes to an interval bitmask relative to a root.
fn to_interval_mask(pitch_classes: &[u8], root: u8) -> u16 {
    let mut mask = 0u16;
    for &pc in pitch_classes {
        let interval = (pc + 12 - root) % 12;
        mask |= 1 << interval;
    }
    mask
}

/// Count set bits in a u16.
fn popcount(mut x: u16) -> usize {
    let mut count = 0;
    while x != 0 {
        count += x & 1;
        x >>= 1;
    }
    count as usize
}

/// Match a set of pitch classes against chord templates.
///
/// Returns `(root_pc, symbol, quality, confidence)` or `None` if no match.
/// Tries all 12 possible roots and all templates, scores by template coverage.
/// `bass_hint` biases root selection when ambiguous.
pub fn match_chord(
    pitch_classes: &[u8],
    bass_hint: Option<u8>,
    use_flats: bool,
) -> Option<(u8, String, ChordQuality, f64)> {
    if pitch_classes.len() < 2 {
        return None;
    }

    let mut best_root: u8 = 0;
    let mut best_score = 0.0_f64;
    let mut best_quality = ChordQuality::Major;
    let mut best_suffix = "";

    for root in 0..12u8 {
        let intervals = to_interval_mask(pitch_classes, root);

        for template in TEMPLATES {
            // How many template tones are present?
            let matched = popcount(intervals & template.intervals);
            if matched < template.size.min(2) {
                continue;
            }

            // Score: fraction of template matched, penalize extra notes
            let extra = popcount(intervals & !template.intervals);
            let mut score = matched as f64 / template.size as f64 - extra as f64 * 0.1;

            // Bonus for bass hint matching root
            if let Some(bass) = bass_hint {
                if bass % 12 == root {
                    score += 0.15;
                }
            }

            // Bonus for complete match (all template tones present)
            if intervals & template.intervals == template.intervals {
                score += 0.1;
            }

            if score > best_score {
                best_score = score;
                best_root = root;
                best_quality = template.quality;
                best_suffix = template.suffix;
            }
        }
    }

    if best_score > 0.4 {
        let root_name = note_name(best_root, use_flats);
        let symbol = format!("{}{}", root_name, best_suffix);
        Some((best_root, symbol, best_quality, best_score.min(1.0)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_major_triad() {
        let pcs = [0, 4, 7]; // C E G
        let result = match_chord(&pcs, None, false);
        assert!(result.is_some());
        let (root, symbol, quality, _) = result.unwrap();
        assert_eq!(root, 0);
        assert_eq!(symbol, "C");
        assert_eq!(quality, ChordQuality::Major);
    }

    #[test]
    fn d_minor_triad() {
        let pcs = [2, 5, 9]; // D F A
        let result = match_chord(&pcs, None, false);
        assert!(result.is_some());
        let (root, symbol, quality, _) = result.unwrap();
        assert_eq!(root, 2);
        assert_eq!(symbol, "Dm");
        assert_eq!(quality, ChordQuality::Minor);
    }

    #[test]
    fn g_dominant_7th() {
        let pcs = [7, 11, 2, 5]; // G B D F
        let result = match_chord(&pcs, None, false);
        assert!(result.is_some());
        let (root, symbol, quality, _) = result.unwrap();
        assert_eq!(root, 7);
        assert_eq!(symbol, "G7");
        assert_eq!(quality, ChordQuality::Dominant7);
    }

    #[test]
    fn bass_hint_disambiguates() {
        // C E G could be C major or Am (incomplete) — bass hint should resolve
        let pcs = [0, 4, 7];
        let result = match_chord(&pcs, Some(0), false);
        assert!(result.is_some());
        let (root, _, _, _) = result.unwrap();
        assert_eq!(root, 0, "bass on C should favor C as root");
    }

    #[test]
    fn flat_spelling() {
        let pcs = [1, 5, 8]; // Db F Ab
        let result = match_chord(&pcs, None, true);
        assert!(result.is_some());
        let (_, symbol, _, _) = result.unwrap();
        assert_eq!(symbol, "Db");
    }

    #[test]
    fn single_note_no_match() {
        let pcs = [0];
        assert!(match_chord(&pcs, None, false).is_none());
    }

    #[test]
    fn power_chord() {
        let pcs = [0, 7]; // C G
        let result = match_chord(&pcs, None, false);
        assert!(result.is_some());
        let (_, symbol, quality, _) = result.unwrap();
        assert_eq!(symbol, "C5");
        assert_eq!(quality, ChordQuality::Power);
    }
}
