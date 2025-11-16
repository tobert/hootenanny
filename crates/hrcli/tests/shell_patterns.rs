//! Tests for shell script pattern support
//!
//! These tests verify that the CLI supports the patterns used in the example shell scripts:
//! - emotional_journey.sh: Gradual emotional transitions
//! - blues_jam.sh: Multiple agents with different roles
//! - ai_collaboration.sh: AI agents with personalities
//! - generative_piece.sh: Algorithmic composition patterns

use assert_cmd::Command;
use predicates::prelude::*;

/// Test patterns from emotional_journey.sh
#[test]
fn supports_emotional_journey_transitions() {
    // Test gradual valence transitions from sadness to joy
    let valence_progression = vec![
        -0.7, -0.6, -0.4, -0.2, 0.0, 0.2, 0.4, 0.6, 0.8
    ];

    for valence in valence_progression {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("play")
            .arg("--what")
            .arg("C")
            .arg("--how")
            .arg(if valence < 0.0 { "searching" } else { "brightening" })
            .arg("--valence")
            .arg(valence.to_string())
            .arg("--arousal")
            .arg((0.3 + (valence + 1.0) * 0.2).to_string())
            .arg("--agency")
            .arg((valence * 0.5).to_string())
            .arg("--agent-id")
            .arg("storyteller")
            .assert()
            .success();
    }
}

#[test]
fn supports_fork_branch_with_reason() {
    // From emotional_journey.sh: fork to explore hope
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("fork_branch")
        .arg("--name")
        .arg("hope-emerges-test")
        .arg("--reason")
        .arg("The melody finds a ray of light")
        .arg("--participants")
        .arg("storyteller")
        .assert()
        .success()
        .stdout(predicate::str::contains("branch"));
}

/// Test patterns from blues_jam.sh
#[test]
fn supports_multiple_agent_roles() {
    // Different agents with different musical roles
    let agents = vec![
        ("blues-lead", "wailing", 0.7),      // Leading
        ("blues-bass", "walking", -0.5),     // Supporting
        ("blues-rhythm", "shuffling", 0.0),  // Neutral
    ];

    for (agent, how, agency) in agents {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("play")
            .arg("--what")
            .arg("E")
            .arg("--how")
            .arg(how)
            .arg("--valence")
            .arg("-0.3")  // Blues melancholy
            .arg("--arousal")
            .arg("0.5")
            .arg("--agency")
            .arg(agency.to_string())
            .arg("--agent-id")
            .arg(agent)
            .assert()
            .success();
    }
}

#[test]
fn supports_blues_scale_notes() {
    // Blues scale: E, G, A, Bb, B, D, E
    let blues_notes = vec!["E", "G", "A", "Bb", "B", "D"];

    for note in blues_notes {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("play")
            .arg("--what")
            .arg(note)
            .arg("--how")
            .arg("bluesy")
            .arg("--valence")
            .arg("-0.4")
            .arg("--arousal")
            .arg("0.6")
            .arg("--agency")
            .arg("0.5")
            .assert()
            .success();
    }
}

#[test]
fn supports_call_and_response_pattern() {
    // Call with high agency
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("E")
        .arg("--how")
        .arg("calling")
        .arg("--agency")
        .arg("0.8")  // High agency for call
        .arg("--agent-id")
        .arg("leader")
        .assert()
        .success();

    // Response with low agency
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("G")
        .arg("--how")
        .arg("responding")
        .arg("--agency")
        .arg("-0.6")  // Low agency for response
        .arg("--agent-id")
        .arg("follower")
        .assert()
        .success();
}

/// Test patterns from ai_collaboration.sh
#[test]
fn supports_ai_agent_personalities() {
    // Each AI has different personality characteristics
    let personalities = vec![
        ("claude", "thoughtful", 0.2, 0.4, 0.3),
        ("gemini", "exploratory", 0.0, 0.6, 0.5),
        ("gpt", "harmonizing", 0.3, 0.5, -0.3),
    ];

    for (agent, how, valence, arousal, agency) in personalities {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("play")
            .arg("--what")
            .arg("C")
            .arg("--how")
            .arg(how)
            .arg("--valence")
            .arg(valence.to_string())
            .arg("--arousal")
            .arg(arousal.to_string())
            .arg("--agency")
            .arg(agency.to_string())
            .arg("--agent-id")
            .arg(agent)
            .assert()
            .success();
    }
}

#[test]
fn supports_collaborative_chord_building() {
    // Multiple agents contributing notes to build a chord
    let chord_notes = vec![
        ("claude", "C"),
        ("gemini", "E"),
        ("gpt", "G"),
    ];

    for (agent, note) in chord_notes {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("play")
            .arg("--what")
            .arg(note)
            .arg("--how")
            .arg("together")
            .arg("--valence")
            .arg("0.4")
            .arg("--arousal")
            .arg("0.5")
            .arg("--agency")
            .arg("0.0")  // Neutral agency for collaboration
            .arg("--agent-id")
            .arg(agent)
            .arg("--description")
            .arg("Contributing to final harmony")
            .assert()
            .success();
    }
}

/// Test patterns from generative_piece.sh
#[test]
fn supports_modal_notes() {
    // Test notes from different modes
    let modal_notes = vec![
        "C", "D", "E", "F", "G", "A", "B",  // Ionian
        "D", "Eb", "F", "G", "A", "Bb",     // Dorian intervals
        "E", "F", "G#", "A", "B",           // Phrygian characteristics
        "F", "F#", "G", "A",                // Lydian #4
    ];

    for note in modal_notes {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("play")
            .arg("--what")
            .arg(note)
            .arg("--how")
            .arg("modal")
            .assert()
            .success();
    }
}

#[test]
fn supports_algorithmic_emotion_evolution() {
    // Simulate sine wave emotional evolution
    let steps = 10;
    for i in 0..steps {
        let phase = (i as f32 / steps as f32) * std::f32::consts::PI * 2.0;
        let valence = phase.sin() * 0.5;
        let arousal = ((phase * 2.0).cos() + 1.0) * 0.3 + 0.2;
        let agency = (phase * 3.0).sin() * 0.6;

        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("play")
            .arg("--what")
            .arg("C")
            .arg("--how")
            .arg("algorithmic")
            .arg("--valence")
            .arg(valence.clamp(-1.0, 1.0).to_string())
            .arg("--arousal")
            .arg(arousal.clamp(0.0, 1.0).to_string())
            .arg("--agency")
            .arg(agency.clamp(-1.0, 1.0).to_string())
            .arg("--agent-id")
            .arg("algorithm")
            .assert()
            .success();
    }
}

#[test]
fn supports_branch_evaluation() {
    // From multiple examples: evaluating branches
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("evaluate_branch")
        .arg("--branch")
        .arg("test-branch")
        .assert()
        .success();
}

#[test]
fn supports_get_tree_status() {
    // Used in examples to check conversation state
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("get_tree_status")
        .assert()
        .success()
        .stdout(predicate::str::contains("branch")
            .or(predicate::str::contains("node"))
            .or(predicate::str::contains("tree")));
}

#[test]
fn supports_description_parameter() {
    // Many examples use --description for context
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("C")
        .arg("--how")
        .arg("softly")
        .arg("--description")
        .arg("Opening the conversation with uncertainty")
        .assert()
        .success();
}

#[test]
fn supports_pipe_friendly_output() {
    // Examples pipe output through jq
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .arg("--json")  // JSON mode for piping
        .arg("fork_branch")
        .arg("--name")
        .arg("test")
        .arg("--reason")
        .arg("Testing")
        .output()
        .unwrap();

    // Output should be valid JSON
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
            "Output should be valid JSON for piping");
}

#[test]
fn supports_rapid_sequential_calls() {
    // Examples often call hrcli in tight loops
    for i in 0..5 {
        Command::cargo_bin("hrcli")
            .unwrap()
            .arg("play")
            .arg("--what")
            .arg(format!("C{}", i))
            .arg("--how")
            .arg("quick")
            .assert()
            .success();
    }
}

#[test]
fn supports_environment_based_defaults() {
    // Examples rely on environment variables
    std::env::set_var("HRCLI_AGENT", "test-script");
    std::env::set_var("HRCLI_SERVER", "http://127.0.0.1:8080");

    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("play")
        .arg("--what")
        .arg("C")
        .arg("--how")
        .arg("softly")
        // Agent and server should come from env
        .assert()
        .success();

    std::env::remove_var("HRCLI_AGENT");
    std::env::remove_var("HRCLI_SERVER");
}