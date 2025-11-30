//! Fixture-based tests for ABC parsing and MIDI generation.
//!
//! Each .abc file in tests/fixtures/ is parsed and converted to MIDI.

use abc::{parse, to_midi, MidiParams};
use std::fs;
use std::path::Path;

fn test_fixture(name: &str) {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(format!("{}.abc", name));

    let abc_content = fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {}", name, e));

    // Parse should succeed without errors
    let result = parse(&abc_content);
    assert!(
        !result.has_errors(),
        "Fixture {} had parse errors: {:?}",
        name,
        result.feedback
    );

    // MIDI generation should produce valid output
    let midi = to_midi(&result.value, &MidiParams::default());

    // Valid MIDI starts with MThd
    assert_eq!(
        &midi[0..4],
        b"MThd",
        "Fixture {} produced invalid MIDI header",
        name
    );

    // Should have reasonable length
    assert!(
        midi.len() > 20,
        "Fixture {} produced suspiciously short MIDI: {} bytes",
        name,
        midi.len()
    );

    println!(
        "Fixture {}: {} bytes MIDI, {} warnings",
        name,
        midi.len(),
        result.feedback.len()
    );
}

#[test]
fn test_fixture_simple_melody() {
    test_fixture("simple_melody");
}

#[test]
fn test_fixture_accidentals() {
    test_fixture("accidentals");
}

#[test]
fn test_fixture_durations() {
    test_fixture("durations");
}

#[test]
fn test_fixture_chords() {
    test_fixture("chords");
}

#[test]
fn test_fixture_repeats() {
    test_fixture("repeats");
}

#[test]
fn test_fixture_ties() {
    test_fixture("ties");
}

#[test]
fn test_fixture_triplets() {
    test_fixture("triplets");
}

#[test]
fn test_fixture_keys() {
    test_fixture("keys");
}

#[test]
fn test_fixture_two_voices() {
    test_fixture("two_voices");
}

#[test]
fn test_multivoice_structure() {
    let abc = r#"X:1
T:Two Voice Test
M:4/4
L:1/4
V:1 name="Melody"
V:2 name="Bass"
K:C
V:1
cdef|
V:2
C,D,E,F,|
"#;

    let result = parse(abc);
    assert!(!result.has_errors(), "Parse errors: {:?}", result.feedback);

    // Should have 2 voice definitions
    assert_eq!(result.value.header.voice_defs.len(), 2);
    assert_eq!(result.value.header.voice_defs[0].id, "1");
    assert_eq!(result.value.header.voice_defs[0].name, Some("Melody".to_string()));
    assert_eq!(result.value.header.voice_defs[1].id, "2");
    assert_eq!(result.value.header.voice_defs[1].name, Some("Bass".to_string()));

    // Should have 2 voices in the tune
    assert_eq!(result.value.voices.len(), 2);

    // Each voice should have content (notes)
    let voice1_notes = result.value.voices[0].elements.iter()
        .filter(|e| matches!(e, abc::Element::Note(_)))
        .count();
    let voice2_notes = result.value.voices[1].elements.iter()
        .filter(|e| matches!(e, abc::Element::Note(_)))
        .count();

    assert_eq!(voice1_notes, 4, "Voice 1 should have 4 notes");
    assert_eq!(voice2_notes, 4, "Voice 2 should have 4 notes");

    // MIDI should be format 1 (check header)
    let midi = to_midi(&result.value, &MidiParams::default());
    assert_eq!(&midi[0..4], b"MThd");
    // Format at bytes 8-9
    assert_eq!(midi[9], 1, "Should be format 1 for multiple voices");
}

/// Test that all fixtures in the directory are covered by tests
#[test]
fn test_all_fixtures_have_tests() {
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");

    let fixture_names: Vec<_> = fs::read_dir(&fixtures_dir)
        .expect("Failed to read fixtures directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()? == "abc" {
                path.file_stem()?.to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();

    // List of fixtures we have tests for
    let tested = [
        "simple_melody",
        "accidentals",
        "durations",
        "chords",
        "two_voices",
        "repeats",
        "ties",
        "triplets",
        "keys",
    ];

    for name in &fixture_names {
        assert!(
            tested.contains(&name.as_str()),
            "Fixture {} exists but has no test",
            name
        );
    }
}
