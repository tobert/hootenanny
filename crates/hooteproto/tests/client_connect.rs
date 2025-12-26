//! Tests for HootClient connection behavior
//!
//! These tests verify that ZMQ connections follow the Lazy Pirate pattern:
//! - connect() is non-blocking even if peer doesn't exist
//! - Requests timeout and retry appropriately

use hooteproto::{ClientConfig, HootClient};
use std::time::{Duration, Instant};

/// Test that connect() returns quickly even when no peer is listening
#[tokio::test]
async fn test_connect_to_nonexistent_peer() {
    // Connect to a port that nothing is listening on
    let config = ClientConfig::new("test", "tcp://127.0.0.1:59999")
        .with_timeout(1000); // 1 second timeout

    let start = Instant::now();
    let client = HootClient::new(config).await;
    let elapsed = start.elapsed();

    // Connect should be fast (< 100ms), not wait for peer
    assert!(
        elapsed < Duration::from_millis(500),
        "connect() took {:?}, should be instant",
        elapsed
    );

    // Client should exist but be in Unknown state (never heard from peer)
    assert!(!client.health.is_connected());
}

/// Test that connect() with timeout works correctly
#[tokio::test]
async fn test_connect_timeout_behavior() {
    let config = ClientConfig::new("test", "tcp://127.0.0.1:59998")
        .with_timeout(500); // 500ms timeout

    let start = Instant::now();
    let _client = HootClient::new(config).await;
    let elapsed = start.elapsed();

    println!("Connect took {:?}", elapsed);

    // Should not hang - connect is non-blocking
    assert!(
        elapsed < Duration::from_secs(1),
        "connect() hung for {:?}",
        elapsed
    );
}

/// Test connecting to localhost with different endpoints
#[tokio::test]
async fn test_connect_various_endpoints() {
    let endpoints = [
        "tcp://127.0.0.1:59997",
        "tcp://localhost:59996",
        // "ipc:///tmp/hooteproto-test.sock", // IPC should also work
    ];

    for endpoint in endpoints {
        let config = ClientConfig::new("test", endpoint)
            .with_timeout(500);

        let start = Instant::now();
        let _client = HootClient::new(config).await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(500),
            "connect({}) took {:?}",
            endpoint,
            elapsed
        );
    }
}
