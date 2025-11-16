//! Tests for dynamic tool discovery and caching
//!
//! These tests verify that the CLI can:
//! - Discover tools from the MCP server
//! - Cache tool schemas with TTL
//! - Fall back to cache when offline
//! - Handle discovery errors gracefully

mod common;

use assert_cmd::Command;
use common::TestMcpServer;
use predicates::prelude::*;
use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
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
    assert!(stdout.contains("play"), "Should find 'play' tool");
    assert!(stdout.contains("fork_branch"), "Should find 'fork_branch' tool");
    assert!(stdout.contains("add_node"), "Should find 'add_node' tool");
    assert!(stdout.contains("get_tree_status"), "Should find 'get_tree_status' tool");
}

#[tokio::test]
#[ignore = "Caching not yet implemented"]
async fn caches_discovered_tools() {
    let cache_dir = TempDir::new().unwrap();
    let server = TestMcpServer::start().await.unwrap();

    // First call - should discover and cache
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_CACHE_DIR", cache_dir.path())
        .env("HRCLI_SERVER", &server.url)
        .arg("discover")
        .assert()
        .success();

    // Verify cache file was created
    let cache_file = cache_dir.path().join("tools.json");
    assert!(cache_file.exists(), "Cache file should be created");

    // Shutdown the server
    server.shutdown().await;

    // Second call - should use cache (even with server down)
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_CACHE_DIR", cache_dir.path())
        .arg("--offline")  // Force offline mode
        .arg("discover")
        .assert()
        .success()
        .stdout(predicate::str::contains("play"));
}

#[test]
#[ignore = "Caching not yet implemented"]
fn uses_cache_in_offline_mode() {
    let cache_dir = TempDir::new().unwrap();

    // Pre-populate cache with test data
    let cache_data = json!({
        "version": "0.1.0",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "server_url": "http://127.0.0.1:8080",
        "tools": [
            {
                "name": "test_tool",
                "description": "A test tool",
                "parameters": {}
            }
        ]
    });

    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.path().join("tools.json"),
        serde_json::to_string_pretty(&cache_data).unwrap(),
    ).unwrap();

    // Run in offline mode
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_CACHE_DIR", cache_dir.path())
        .arg("--offline")
        .arg("discover")
        .assert()
        .success()
        .stdout(predicate::str::contains("test_tool"));
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

#[test]
#[ignore = "Caching not yet implemented"]
fn handles_corrupted_cache() {
    let cache_dir = TempDir::new().unwrap();

    // Write corrupted cache
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.path().join("tools.json"),
        "{ this is not valid json }",
    ).unwrap();

    // Should detect corruption and either refresh or error appropriately
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_CACHE_DIR", cache_dir.path())
        .arg("--offline")
        .arg("discover")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cache").or(predicate::str::contains("corrupted")));
}

#[tokio::test]
#[ignore = "Caching not yet implemented"]
async fn refreshes_stale_cache() {
    let cache_dir = TempDir::new().unwrap();
    let server = TestMcpServer::start().await.unwrap();

    // Create a stale cache (old timestamp)
    let stale_time = chrono::Utc::now() - chrono::Duration::minutes(10);
    let cache_data = json!({
        "version": "0.1.0",
        "timestamp": stale_time.to_rfc3339(),
        "server_url": &server.url,
        "tools": [
            {
                "name": "old_tool",
                "description": "An old tool",
                "parameters": {}
            }
        ]
    });

    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.path().join("tools.json"),
        serde_json::to_string_pretty(&cache_data).unwrap(),
    ).unwrap();

    // Should detect stale cache and refresh with real tools from server
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_CACHE_DIR", cache_dir.path())
        .env("HRCLI_CACHE_TTL", "300")  // 5 minutes
        .env("HRCLI_SERVER", &server.url)
        .arg("discover")
        .assert()
        .success()
        .stdout(predicate::str::contains("play"))  // New tools from real server
        .stdout(predicate::str::contains("old_tool").not());  // Old tool gone
}

#[tokio::test]
async fn discovers_real_tools_with_complex_schemas() {
    let server = TestMcpServer::start().await.unwrap();

    // The real server has tools with complex schemas (EmotionalVector, etc)
    Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", &server.url)
        .arg("discover")
        .assert()
        .success()
        .stdout(predicate::str::contains("play"))
        .stdout(predicate::str::contains("add_node"));
}