//! Key signature parsing for ABC notation.

use crate::ast::{Accidental, Clef, Key, Mode, NoteName};
use crate::feedback::FeedbackCollector;

/// Parse a K: field value (e.g., "G", "Am", "D dorian", "F#m", "Bb")
pub fn parse_key_field(value: &str, collector: &mut FeedbackCollector) -> Key {
    let trimmed = value.trim();

    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
        return Key::default();
    }

    let mut chars = trimmed.chars().peekable();

    // Parse root note (A-G)
    let root = match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => match c.to_ascii_uppercase() {
            'C' => NoteName::C,
            'D' => NoteName::D,
            'E' => NoteName::E,
            'F' => NoteName::F,
            'G' => NoteName::G,
            'A' => NoteName::A,
            'B' => NoteName::B,
            _ => {
                collector.warning(format!("Invalid key root '{}', assuming C", c));
                NoteName::C
            }
        },
        _ => {
            collector.warning("Empty or invalid key, assuming C");
            return Key::default();
        }
    };

    // Parse optional accidental (#, b)
    let accidental = if chars.peek() == Some(&'#') {
        chars.next();
        Some(Accidental::Sharp)
    } else if chars.peek() == Some(&'b') {
        // 'b' is flat only if not followed by a letter (which would be mode like "bm")
        // Check what comes after the 'b'
        let mut lookahead = chars.clone();
        lookahead.next(); // skip the 'b'
        let next_char = lookahead.next();
        if !matches!(next_char, Some('a'..='z' | 'A'..='Z')) {
            chars.next();
            Some(Accidental::Flat)
        } else {
            None
        }
    } else {
        None
    };

    // Collect remaining for mode parsing
    let remaining: String = chars.collect();
    let remaining = remaining.trim();

    // Parse mode
    let mode = if remaining.is_empty() {
        Mode::Major
    } else {
        // Could be "m", "min", "minor", "dor", "dorian", etc.
        // Also handle "maj", "major" explicitly
        let mode_str = remaining.split_whitespace().next().unwrap_or("");

        Mode::from_str(mode_str).unwrap_or_else(|| {
            collector.warning(format!("Unknown mode '{}', assuming major", mode_str));
            Mode::Major
        })
    };

    // Parse optional clef (clef=bass, etc.) - for future use
    let clef = if remaining.contains("clef=") {
        parse_clef_from_key(remaining)
    } else {
        None
    };

    Key {
        root,
        accidental,
        mode,
        explicit_accidentals: Vec::new(), // TODO: parse exp ^f _b syntax
        clef,
    }
}

/// Parse clef specification from key field
fn parse_clef_from_key(s: &str) -> Option<Clef> {
    if let Some(pos) = s.find("clef=") {
        let after = &s[pos + 5..];
        let clef_name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphabetic() || *c == '-')
            .collect();

        match clef_name.to_lowercase().as_str() {
            "treble" | "treble-8" | "treble+8" => Some(Clef::Treble),
            "bass" | "bass-8" | "bass+8" => Some(Clef::Bass),
            "alto" => Some(Clef::Alto),
            "tenor" => Some(Clef::Tenor),
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_key() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("G", &mut collector);

        assert_eq!(key.root, NoteName::G);
        assert_eq!(key.accidental, None);
        assert_eq!(key.mode, Mode::Major);
        assert!(!collector.has_errors());
    }

    #[test]
    fn test_parse_minor_key() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("Am", &mut collector);

        assert_eq!(key.root, NoteName::A);
        assert_eq!(key.accidental, None);
        assert_eq!(key.mode, Mode::Minor);
    }

    #[test]
    fn test_parse_sharp_key() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("F#m", &mut collector);

        assert_eq!(key.root, NoteName::F);
        assert_eq!(key.accidental, Some(Accidental::Sharp));
        assert_eq!(key.mode, Mode::Minor);
    }

    #[test]
    fn test_parse_flat_key() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("Bb", &mut collector);

        assert_eq!(key.root, NoteName::B);
        assert_eq!(key.accidental, Some(Accidental::Flat));
        assert_eq!(key.mode, Mode::Major);
    }

    #[test]
    fn test_parse_modal_key() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("D dorian", &mut collector);

        assert_eq!(key.root, NoteName::D);
        assert_eq!(key.mode, Mode::Dorian);
    }

    #[test]
    fn test_parse_modal_abbreviated() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("E mix", &mut collector);

        assert_eq!(key.root, NoteName::E);
        assert_eq!(key.mode, Mode::Mixolydian);
    }

    #[test]
    fn test_parse_key_with_clef() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("G clef=bass", &mut collector);

        assert_eq!(key.root, NoteName::G);
        assert_eq!(key.clef, Some(Clef::Bass));
    }

    #[test]
    fn test_parse_empty_key() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("", &mut collector);

        assert_eq!(key.root, NoteName::C);
        assert_eq!(key.mode, Mode::Major);
    }

    #[test]
    fn test_parse_none_key() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("none", &mut collector);

        assert_eq!(key.root, NoteName::C);
    }

    #[test]
    fn test_lowercase_key() {
        let mut collector = FeedbackCollector::new();
        let key = parse_key_field("g", &mut collector);

        assert_eq!(key.root, NoteName::G);
        assert_eq!(key.mode, Mode::Major);
    }
}
