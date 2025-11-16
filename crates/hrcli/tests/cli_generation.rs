//! Tests for dynamic CLI generation and parameter mapping
//!
//! These tests verify that the CLI can:
//! - Generate subcommands from discovered tools
//! - Map complex types (EmotionalVector) to multiple flags
//! - Generate help text for both human and AI audiences
//! - Handle environment variable defaults

use assert_cmd::Command;
use predicates::prelude::*;
use std::env;
use tempfile::TempDir;

/// Test that EmotionalVector is mapped to three separate flags
#[test]
fn maps_emotional_vector_to_three_flags() {
    // Once dynamic CLI is implemented, this would test:
    // EmotionalVector { valence, arousal, agency } becomes:
    // --valence <-1.0..1.0>
    // --arousal <0.0..1.0>
    // --agency <-1.0..1.0>

    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("C")
        .arg("--how")
        .arg("softly")
        .arg("--valence")
        .arg("0.5")
        .arg("--arousal")
        .arg("0.3")
        .arg("--agency")
        .arg("0.2")
        .arg("--agent-id")
        .arg("test-agent")
        .assert()
        .success();
}

#[test]
fn validates_emotional_parameter_ranges() {
    // Valence outside range (-1.0 to 1.0)
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("C")
        .arg("--how")
        .arg("softly")
        .arg("--valence")
        .arg("2.0")  // Outside valid range
        .assert()
        .failure()
        .stderr(predicate::str::contains("valence"))
        .stderr(predicate::str::contains("-1.0 to 1.0"));

    // Arousal outside range (0.0 to 1.0)
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("C")
        .arg("--how")
        .arg("softly")
        .arg("--arousal")
        .arg("-0.5")  // Negative not allowed
        .assert()
        .failure()
        .stderr(predicate::str::contains("arousal"))
        .stderr(predicate::str::contains("0.0 to 1.0"));
}

#[test]
fn uses_environment_variable_defaults() {
    env::set_var("HRCLI_DEFAULT_VALENCE", "0.3");
    env::set_var("HRCLI_DEFAULT_AROUSAL", "0.6");
    env::set_var("HRCLI_DEFAULT_AGENCY", "-0.2");
    env::set_var("HRCLI_AGENT", "env-agent");

    // Should use env defaults when not specified
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("C")
        .arg("--how")
        .arg("softly")
        // No emotional parameters specified - should use env defaults
        .assert()
        .success();

    // Clean up
    env::remove_var("HRCLI_DEFAULT_VALENCE");
    env::remove_var("HRCLI_DEFAULT_AROUSAL");
    env::remove_var("HRCLI_DEFAULT_AGENCY");
    env::remove_var("HRCLI_AGENT");
}

#[test]
fn generates_help_for_human_audience() {
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--help")
        .output()
        .unwrap();

    let help = String::from_utf8_lossy(&output.stdout);

    // Check for human-friendly sections
    assert!(help.contains("WHEN TO USE") || help.contains("when to use"),
            "Should have 'when to use' section for humans");
    assert!(help.contains("EXAMPLES") || help.contains("examples"),
            "Should have examples section");
    assert!(help.contains("FOR HUMANS") || help.contains("for humans"),
            "Should have explicit human section");

    // Check for practical information
    assert!(help.contains("--what"),
            "Should show parameter flags");
    assert!(help.contains("--valence"),
            "Should show emotional parameters");
}

#[test]
fn generates_help_for_ai_audience() {
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--help")
        .output()
        .unwrap();

    let help = String::from_utf8_lossy(&output.stdout);

    // Check for AI-friendly sections
    assert!(help.contains("FOR AI AGENTS") || help.contains("for AI agents") ||
            help.contains("emotional") || help.contains("intention"),
            "Should have AI-focused content");

    // Check for emotional context explanation
    assert!(help.contains("valence") &&
            (help.contains("joy") || help.contains("sorrow") || help.contains("emotional")),
            "Should explain emotional dimensions");
    assert!(help.contains("arousal") &&
            (help.contains("energy") || help.contains("activation")),
            "Should explain arousal dimension");
    assert!(help.contains("agency") &&
            (help.contains("leading") || help.contains("following") || help.contains("initiative")),
            "Should explain agency dimension");
}

#[test]
fn shows_global_help_with_philosophy() {
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();

    let help = String::from_utf8_lossy(&output.stdout);

    // Check for philosophical/conceptual content
    assert!(help.contains("musical") || help.contains("Musical"),
            "Should mention music");
    assert!(help.contains("conversation") || help.contains("Conversation"),
            "Should mention conversation");

    // Check for both audiences mentioned
    assert!((help.contains("human") || help.contains("Human")) &&
            (help.contains("AI") || help.contains("language model") || help.contains("agent")),
            "Should acknowledge both human and AI users");
}

#[test]
fn handles_required_vs_optional_parameters() {
    // Missing required parameter
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--how")
        .arg("softly")
        // Missing required --what parameter
        .assert()
        .failure()
        .stderr(predicate::str::contains("required")
            .and(predicate::str::contains("what")));

    // Optional parameters should work without values
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("C")
        .arg("--how")
        .arg("softly")
        // Optional: valence, arousal, agency, description
        .assert()
        .success();
}

#[test]
fn supports_musical_parameter_types() {
    // Note parameter
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("C#")  // Sharp
        .arg("--how")
        .arg("softly")
        .assert()
        .success();

    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("Eb")  // Flat
        .arg("--how")
        .arg("softly")
        .assert()
        .success();

    // Chord parameter
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("Cmaj7")  // Chord symbol
        .arg("--how")
        .arg("softly")
        .assert()
        .success();

    // Invalid musical notation
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("H#")  // H is not a valid note
        .arg("--how")
        .arg("softly")
        .assert()
        .failure();
}

#[test]
fn generates_subcommands_for_all_discovered_tools() {
    // Each discovered tool should become a subcommand
    let tools = vec!["play", "fork_branch", "evaluate_branch", "add_node"];

    for tool in tools {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg(tool)
            .arg("--help")
            .assert()
            .success()
            .stdout(predicate::str::contains(tool));
    }
}

#[test]
fn handles_complex_json_parameters() {
    // Some parameters might need JSON input
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("call")  // Generic call command for complex types
        .arg("complex_tool")
        .arg(r#"{"nested": {"value": 42}}"#)
        .assert()
        .success();
}

#[test]
fn shows_parameter_descriptions_in_help() {
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--help")
        .output()
        .unwrap();

    let help = String::from_utf8_lossy(&output.stdout);

    // Each parameter should have a description
    assert!(help.contains("--what") && help.contains("musical content"),
            "Should describe what parameter");
    assert!(help.contains("--how") && help.contains("performance") || help.contains("character"),
            "Should describe how parameter");
    assert!(help.contains("--valence") && help.contains("joy") || help.contains("sorrow"),
            "Should describe valence parameter");
}

#[test]
fn supports_environment_variable_in_help() {
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--help")
        .output()
        .unwrap();

    let help = String::from_utf8_lossy(&output.stdout);

    // Should show which env vars affect each parameter
    assert!(help.contains("HRCLI_") || help.contains("env:"),
            "Should mention environment variables");
}

#[test]
#[ignore = "Requires snapshot testing setup"]
fn help_text_matches_snapshot() {
    // Using insta for snapshot testing of help text
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();

    let help = String::from_utf8_lossy(&output.stdout);
    insta::assert_snapshot!(help);
}

#[test]
fn generates_shell_completions() {
    // Test that completion generation works
    for shell in &["bash", "zsh", "fish"] {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("completions")
            .arg(shell)
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }
}