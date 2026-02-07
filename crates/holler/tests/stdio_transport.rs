//! Integration test for stdio MCP transport
//!
//! Tests the `holler mcp` binary command using stdin/stdout.
//! These tests verify MCP protocol handling - they don't require hootenanny to be running
//! since initialize/initialized are handled by rmcp before any backend calls.

use std::io::Write;
use std::process::{Command, Stdio};

/// Test that stdio transport responds to initialize request
#[test]
fn test_stdio_initialize() {
    // Binary path relative to test crate
    let binary = std::env::current_dir()
        .unwrap()
        .parent() // crates
        .unwrap()
        .parent() // hootenanny
        .unwrap()
        .join("target/debug/holler");

    if !binary.exists() {
        eprintln!("Skipping test: binary not found at {:?}", binary);
        return;
    }

    let mut child = Command::new(&binary)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start holler mcp");

    // Take ownership of stdin so we can close it
    let mut stdin = child.stdin.take().expect("Failed to open stdin");

    // Send initialize request
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
    writeln!(stdin, "{}", init_request).expect("Failed to write to stdin");

    // Send initialized notification (completes the handshake)
    let init_notification = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    writeln!(stdin, "{}", init_notification).expect("Failed to write to stdin");

    // Close stdin to signal EOF
    drop(stdin);

    // Wait for process
    let output = child.wait_with_output().expect("Failed to get output");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse the first line as JSON-RPC response
    let first_line = stdout.lines().next().expect("No output received");
    let response: serde_json::Value = serde_json::from_str(first_line)
        .expect("Failed to parse response as JSON");

    // Verify initialize response structure
    assert_eq!(response["jsonrpc"], "2.0");
    assert_eq!(response["id"], 1);
    assert!(response["result"]["serverInfo"].is_object(), "Missing serverInfo");
    assert!(response["result"]["capabilities"]["tools"].is_object(), "Missing tools capability");
    assert_eq!(
        response["result"]["instructions"],
        "Holler MCP gateway - forwards tool calls to hootenanny ZMQ backends. \
         Use resources to explore session context, artifacts, and soundfonts. \
         Use prompts for Trustfall query templates."
    );
}

/// Test that holler mcp binary exists and has help
#[test]
fn test_stdio_help() {
    let binary = std::env::current_dir()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/debug/holler");

    if !binary.exists() {
        eprintln!("Skipping test: binary not found at {:?}", binary);
        return;
    }

    let output = Command::new(&binary)
        .arg("mcp")
        .arg("--help")
        .output()
        .expect("Failed to run holler mcp --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Help should mention stdio
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("stdio") || combined.contains("Claude Code"),
        "Help should mention stdio or Claude Code: {}",
        combined
    );
}
