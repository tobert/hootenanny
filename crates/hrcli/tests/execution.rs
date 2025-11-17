//! Tests for tool execution and response formatting
//!
//! These tests verify that the CLI can:
//! - Transform CLI arguments to MCP JSON-RPC requests
//! - Execute tool calls against the server
//! - Format responses beautifully for both audiences
//! - Handle errors gracefully

mod common;

use assert_cmd::Command;
use common::TestMcpServer;
use predicates::prelude::*;
use serde_json::json;

#[tokio::test]
async fn executes_play_tool_successfully() {
    let server = TestMcpServer::start().await.unwrap();

    // Execute the play tool with the real server
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", &server.url)
        .arg("play")
        .arg("--what").arg("C")
        .arg("--how").arg("softly")
        .arg("--valence").arg("0.5")
        .arg("--arousal").arg("0.3")
        .arg("--agency").arg("0.2")
        .arg("--agent-id").arg("test-agent")
        .output()
        .expect("Failed to execute hrcli");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        eprintln!("stdout: {}", stdout);
        eprintln!("stderr: {}", stderr);
    }

    assert!(output.status.success(), "Play command should succeed");
    // Should return some output about the played note
    assert!(!stdout.is_empty(), "Should have output");
}

#[tokio::test]
async fn executes_add_node_tool_successfully() {
    let server = TestMcpServer::start().await.unwrap();

    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", &server.url)
        .arg("add_node")
        .arg("--what").arg("C")
        .arg("--how").arg("softly")
        .arg("--valence").arg("0.5")
        .arg("--arousal").arg("0.3")
        .arg("--agency").arg("0.2")
        .arg("--agent-id").arg("test-agent")
        .output()
        .expect("Failed to execute hrcli");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        eprintln!("stdout: {}", stdout);
        eprintln!("stderr: {}", stderr);
    }

    assert!(output.status.success(), "Add node command should succeed");
    // Should show node_id in response
    assert!(stdout.contains("node") || stdout.contains("Node"),
            "Should mention node in output");
}

#[tokio::test]
async fn executes_fork_branch_successfully() {
    let server = TestMcpServer::start().await.unwrap();

    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", &server.url)
        .arg("fork_branch")
        .arg("--branch-name").arg("jazz-exploration")
        .arg("--reason-description").arg("Exploring jazz harmonies")
        .arg("--participants").arg("claude,gemini")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        eprintln!("stdout: {}", stdout);
        eprintln!("stderr: {}", stderr);
    }

    assert!(output.status.success(), "Fork branch should succeed");
    assert!(stdout.contains("branch"), "Should show branch in output");
}

#[tokio::test]
async fn executes_get_tree_status_successfully() {
    let server = TestMcpServer::start().await.unwrap();

    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", &server.url)
        .arg("get_tree_status")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "Get tree status should succeed");

    let stdout_lower = stdout.to_lowercase();
    assert!(stdout_lower.contains("branch") || stdout_lower.contains("node"),
            "Should show tree information (got: {})", stdout);
}

#[test]
fn handles_network_errors_gracefully() {
    // Server that doesn't exist
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", "http://localhost:99999")  // Invalid port
        .arg("discover")
        .timeout(std::time::Duration::from_secs(5))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to discover")
            .or(predicate::str::contains("Failed to connect")));
}

#[test]
#[ignore = "Requires CLI flags implementation"]
fn supports_json_output_mode() {
    // For scripting, should support --json flag
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("--json")  // Machine-readable output
        .arg("play")
        .arg("--what").arg("C")
        .arg("--how").arg("softly")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("{")
            .and(predicate::str::ends_with("}\n")));
}

#[test]
#[ignore = "Requires CLI flags implementation"]
fn supports_quiet_mode() {
    // Should support --quiet for minimal output
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("--quiet")
        .arg("play")
        .arg("--what").arg("C")
        .arg("--how").arg("softly")
        .assert()
        .success()
        .stdout(predicate::str::is_empty()
            .or(predicate::function(|s: &str| s.lines().count() <= 1)));
}

#[test]
#[ignore = "Requires CLI flags implementation"]
fn supports_verbose_mode() {
    // Should support --verbose for detailed output
    Command::cargo_bin("hrcli")
        .unwrap()
        .arg("--verbose")
        .arg("play")
        .arg("--what").arg("C")
        .arg("--how").arg("softly")
        .assert()
        .success()
        .stderr(predicate::str::contains("Connecting")
            .or(predicate::str::contains("Sending"))
            .or(predicate::str::contains("DEBUG")));
}