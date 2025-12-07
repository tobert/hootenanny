//! Tests for dynamic tool discovery
//!
//! These tests verify that the CLI can:
//! - Discover tools from the MCP server
//! - Handle discovery errors gracefully

mod common;

use assert_cmd::Command;
use common::TestMcpServer;
use predicates::prelude::*;
use std::time::Duration;

#[tokio::test]
#[ignore = "Requires multi-threaded runtime for subprocess + async server"]
async fn discovers_tools_from_real_server() {
    let server = TestMcpServer::start().await.unwrap();

    // Set the server URL via environment variable
    // Run the CLI to discover tools
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", &server.url)
        .arg("discover")
        .output()
        .expect("Failed to execute hrcli");

    // Check that it succeeded and found our tools
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        eprintln!("stdout: {}", stdout);
        eprintln!("stderr: {}", stderr);
    }

    assert!(output.status.success(), "CLI should succeed");

    // The real server has these actual tools
    assert!(stdout.contains("cas_store"), "Should find 'cas_store' tool");
    assert!(stdout.contains("orpheus_generate"), "Should find 'orpheus_generate' tool");
    assert!(stdout.contains("get_job_status"), "Should find 'get_job_status' tool");
}

#[test]
fn handles_unreachable_server_gracefully() {
    // Server that doesn't exist
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", "http://localhost:99999")  // Invalid port
        .arg("discover")
        .timeout(Duration::from_secs(5))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to discover")
            .or(predicate::str::contains("Failed to connect")));
}

#[tokio::test]
#[ignore = "Requires multi-threaded runtime for subprocess + async server"]
async fn discovers_real_tools_with_complex_schemas() {
    let server = TestMcpServer::start().await.unwrap();

    // The real server has tools with complex schemas
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", &server.url)
        .arg("discover")
        .assert()
        .success()
        .stdout(predicate::str::contains("cas_store"))
        .stdout(predicate::str::contains("orpheus_generate"));
}
