//! Tests for HootClient connection and heartbeat behavior
//!
//! These tests focus on the client-side behavior:
//! - Heartbeat works against simple echo router
//! - Heartbeat works immediately after client creation (no delay)
//! - Client correctly handles the reactor startup

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hooteproto::socket_config::{Multipart, ZmqContext};
use hooteproto::{ClientConfig, Command, HootClient, HootFrame};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tmq::router;

static PORT: AtomicU16 = AtomicU16::new(18700);

fn next_endpoint() -> String {
    let port = PORT.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

fn frames_to_multipart(frames: &[Bytes]) -> Multipart {
    frames
        .iter()
        .map(|f| f.to_vec())
        .collect::<Vec<_>>()
        .into()
}

fn multipart_to_frames(mp: Multipart) -> Vec<Bytes> {
    mp.into_iter()
        .map(|m| Bytes::from(m.to_vec()))
        .collect()
}

/// Simple echo router that responds to any HOOT01 message
async fn simple_echo_router(endpoint: &str, message_count: usize) {
    let ctx = ZmqContext::new();
    let socket = router(&ctx)
        .set_linger(0)
        .bind(endpoint)
        .expect("Failed to bind router");

    let (mut tx, mut rx) = socket.split();

    for i in 0..message_count {
        let mp = rx.next().await.expect("Stream ended").expect("Recv failed");
        let frames = multipart_to_frames(mp);

        println!("[Router] Received message {} with {} frames", i, frames.len());

        let (identity, request) = match HootFrame::from_frames_with_identity(&frames) {
            Ok(r) => r,
            Err(e) => {
                println!("[Router] Failed to parse: {}", e);
                continue;
            }
        };

        println!(
            "[Router] Parsed: cmd={:?}, request_id={}",
            request.command, request.request_id
        );

        // Echo back with same request_id (critical for correlation)
        let response = HootFrame {
            command: if request.command == Command::Heartbeat {
                Command::Heartbeat
            } else {
                Command::Reply
            },
            content_type: request.content_type,
            request_id: request.request_id, // Echo the request ID!
            service: "test-router".to_string(),
            traceparent: request.traceparent,
            body: request.body,
        };

        let reply_frames = response.to_frames_with_identity(&identity);
        let reply_mp = frames_to_multipart(&reply_frames);
        tx.send(reply_mp).await.expect("Send failed");
        println!("[Router] Sent response {}", i);
    }
}

/// Test that HootClient heartbeat works against a simple echo router
#[tokio::test]
async fn test_hootclient_heartbeat_direct() {
    let endpoint = next_endpoint();

    // Start router
    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        simple_echo_router(&router_endpoint, 1).await;
    });

    // Give router time to bind
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create client
    let config = ClientConfig::new("test-client", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;

    // Test heartbeat
    let result = client.heartbeat().await;
    println!("Heartbeat result: {:?}", result);
    assert!(result.is_ok(), "Heartbeat should succeed: {:?}", result);

    router_handle.abort();
}

/// Test that heartbeat works IMMEDIATELY after client creation
/// This tests the reactor startup timing
#[tokio::test]
async fn test_hootclient_heartbeat_immediate() {
    let endpoint = next_endpoint();

    // Start router first
    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        simple_echo_router(&router_endpoint, 1).await;
    });

    // Give router time to bind
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create client and IMMEDIATELY try heartbeat (no delay after new())
    let config = ClientConfig::new("test-client", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;

    // Immediate heartbeat - tests reactor is ready
    let result = client.heartbeat().await;
    println!("Immediate heartbeat result: {:?}", result);
    assert!(
        result.is_ok(),
        "Immediate heartbeat should succeed: {:?}",
        result
    );

    router_handle.abort();
}

/// Test multiple sequential heartbeats
#[tokio::test]
async fn test_hootclient_heartbeat_sequential() {
    let endpoint = next_endpoint();

    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        simple_echo_router(&router_endpoint, 5).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let config = ClientConfig::new("test-client", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;

    for i in 0..5 {
        let result = client.heartbeat().await;
        assert!(result.is_ok(), "Heartbeat {} should succeed: {:?}", i, result);
        println!("Heartbeat {} succeeded", i);
    }

    router_handle.abort();
}

/// Test that cloned clients share the same reactor
#[tokio::test]
async fn test_hootclient_clone_heartbeat() {
    let endpoint = next_endpoint();

    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        simple_echo_router(&router_endpoint, 2).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let config = ClientConfig::new("test-client", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;
    let client2 = client.clone();

    // Both clones should work
    let result1 = client.heartbeat().await;
    assert!(result1.is_ok(), "Clone 1 heartbeat failed: {:?}", result1);

    let result2 = client2.heartbeat().await;
    assert!(result2.is_ok(), "Clone 2 heartbeat failed: {:?}", result2);

    router_handle.abort();
}

/// Test heartbeat through Arc<RwLock<>> pattern (like holler's BackendPool)
#[tokio::test]
async fn test_hootclient_heartbeat_through_rwlock() {
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let endpoint = next_endpoint();

    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        simple_echo_router(&router_endpoint, 1).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Mimic BackendPool pattern: client inside Arc<RwLock<Option<...>>>
    let config = ClientConfig::new("test-client", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;

    struct Pool {
        client: Option<Arc<HootClient>>,
    }

    let pool = Arc::new(RwLock::new(Pool {
        client: Some(client),
    }));

    // Access through RwLock like holler's BackendPool does
    let result = {
        let guard = pool.read().await;
        if let Some(ref c) = guard.client {
            c.heartbeat().await
        } else {
            Err(anyhow::anyhow!("No client"))
        }
    };

    println!("RwLock pattern heartbeat: {:?}", result);
    assert!(result.is_ok(), "Heartbeat through RwLock should work: {:?}", result);

    router_handle.abort();
}

/// Test that identity "hootenanny" works (same as BackendPool uses)
#[tokio::test]
async fn test_hootclient_identity_hootenanny() {
    let endpoint = next_endpoint();

    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        simple_echo_router(&router_endpoint, 1).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Use "hootenanny" identity like BackendPool does
    let config = ClientConfig::new("hootenanny", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;

    let result = client.heartbeat().await;
    println!("Identity 'hootenanny' heartbeat: {:?}", result);
    assert!(result.is_ok(), "Heartbeat with 'hootenanny' identity should work: {:?}", result);

    router_handle.abort();
}

/// Test that two clients with SAME identity causes problems
/// This documents the identity collision bug - ZMQ ROUTER can't distinguish them
#[tokio::test]
async fn test_identity_collision_causes_routing_failure() {
    let endpoint = next_endpoint();

    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        simple_echo_router(&router_endpoint, 10).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create two clients with the SAME identity - this is a bug scenario
    let config1 = ClientConfig::new("same-identity", &endpoint).with_timeout(500);
    let client1 = HootClient::new(config1).await;

    let config2 = ClientConfig::new("same-identity", &endpoint).with_timeout(500);
    let client2 = HootClient::new(config2).await;

    // With same identity, ZMQ routing becomes unpredictable
    // One client may receive the other's responses, or responses may be lost
    let r1 = client1.heartbeat().await;
    let r2 = client2.heartbeat().await;

    println!("Client1 (same identity): {:?}", r1);
    println!("Client2 (same identity): {:?}", r2);

    // Note: This test documents problematic behavior, not correct behavior
    // At least one will likely fail due to routing confusion
    // We don't assert here because the failure mode is unpredictable

    router_handle.abort();
}

/// Test that two clients with DIFFERENT identities both work
#[tokio::test]
async fn test_unique_identities_work_correctly() {
    let endpoint = next_endpoint();

    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        simple_echo_router(&router_endpoint, 4).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create two clients with UNIQUE identities - correct usage
    let config1 = ClientConfig::new("client-1", &endpoint).with_timeout(5000);
    let client1 = HootClient::new(config1).await;

    let config2 = ClientConfig::new("client-2", &endpoint).with_timeout(5000);
    let client2 = HootClient::new(config2).await;

    // Both should work correctly with unique identities
    let r1 = client1.heartbeat().await;
    let r2 = client2.heartbeat().await;

    println!("Client1 (unique): {:?}", r1);
    println!("Client2 (unique): {:?}", r2);

    assert!(r1.is_ok(), "Client1 with unique identity should work");
    assert!(r2.is_ok(), "Client2 with unique identity should work");

    router_handle.abort();
}

/// Test heartbeat against non-existent endpoint (should timeout gracefully)
#[tokio::test]
async fn test_hootclient_heartbeat_no_peer() {
    // Use a port that nothing is listening on
    let endpoint = "tcp://127.0.0.1:19999";

    let config = ClientConfig::new("test-client", endpoint)
        .with_timeout(500) // Short timeout
        .with_retries(1); // Only 1 retry
    let client = HootClient::new(config).await;

    let start = std::time::Instant::now();
    let result = client.heartbeat().await;
    let elapsed = start.elapsed();

    println!("No-peer heartbeat: {:?} in {:?}", result, elapsed);
    assert!(result.is_err(), "Heartbeat to non-existent peer should fail");
}
