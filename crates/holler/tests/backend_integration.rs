//! Integration tests for BackendPool against real hootenanny
//!
//! These tests require hootenanny to be running on localhost:5580
//! Run with: cargo test -p holler --test backend_integration -- --ignored

use holler::backend::BackendPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

const HOOTENANNY_ENDPOINT: &str = "tcp://127.0.0.1:5580";
const TIMEOUT_MS: u64 = 5000;

/// Test that BackendPool.setup_hootenanny creates a working client
#[tokio::test]
#[ignore = "requires hootenanny running on localhost:5580"]
async fn test_backendpool_setup_hootenanny() {
    let mut pool = BackendPool::new();
    pool.setup_hootenanny(HOOTENANNY_ENDPOINT, TIMEOUT_MS).await;

    assert!(pool.hootenanny.is_some(), "Client should be created");

    let client = pool.hootenanny.as_ref().unwrap();
    let result = client.heartbeat().await;
    println!("Direct heartbeat: {:?}", result);
    assert!(result.is_ok(), "Heartbeat should succeed: {:?}", result);
}

/// Test heartbeat through Arc<RwLock<BackendPool>> - same pattern as stdio.rs
#[tokio::test]
#[ignore = "requires hootenanny running on localhost:5580"]
async fn test_backendpool_heartbeat_through_rwlock() {
    let mut pool = BackendPool::new();
    pool.setup_hootenanny(HOOTENANNY_ENDPOINT, TIMEOUT_MS).await;

    let pool = Arc::new(RwLock::new(pool));

    // This is the exact pattern used in stdio.rs
    let result = {
        let guard = pool.read().await;
        if let Some(ref client) = guard.hootenanny {
            client.heartbeat().await
        } else {
            Err(anyhow::anyhow!("No client"))
        }
    };

    println!("RwLock heartbeat: {:?}", result);
    assert!(result.is_ok(), "Heartbeat through RwLock should succeed: {:?}", result);
}

/// Test heartbeat immediately after setup (no delay)
#[tokio::test]
#[ignore = "requires hootenanny running on localhost:5580"]
async fn test_backendpool_heartbeat_immediate() {
    let mut pool = BackendPool::new();
    pool.setup_hootenanny(HOOTENANNY_ENDPOINT, TIMEOUT_MS).await;

    // Immediate heartbeat - no delay after setup
    let client = pool.hootenanny.as_ref().unwrap();
    let result = client.heartbeat().await;

    println!("Immediate heartbeat: {:?}", result);
    assert!(result.is_ok(), "Immediate heartbeat should succeed: {:?}", result);
}

/// Test multiple heartbeats in sequence
#[tokio::test]
#[ignore = "requires hootenanny running on localhost:5580"]
async fn test_backendpool_heartbeat_sequential() {
    let mut pool = BackendPool::new();
    pool.setup_hootenanny(HOOTENANNY_ENDPOINT, TIMEOUT_MS).await;

    let client = pool.hootenanny.as_ref().unwrap();

    for i in 0..5 {
        let start = std::time::Instant::now();
        let result = client.heartbeat().await;
        let elapsed = start.elapsed();
        println!("Heartbeat {}: {:?} in {:?}", i, result.is_ok(), elapsed);
        assert!(result.is_ok(), "Heartbeat {} failed: {:?}", i, result);
    }
}

/// Test the full stdio.rs pattern: setup, wrap in RwLock, wait for heartbeat
#[tokio::test]
#[ignore = "requires hootenanny running on localhost:5580"]
async fn test_stdio_pattern() {
    // Exactly what stdio.rs does
    let mut backends = BackendPool::new();
    backends.setup_hootenanny(HOOTENANNY_ENDPOINT, TIMEOUT_MS).await;

    let backends = Arc::new(RwLock::new(backends));

    // Block until hootenanny is reachable
    let connected = {
        let backends_guard = backends.read().await;
        if let Some(ref client) = backends_guard.hootenanny {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut connected = false;
            while std::time::Instant::now() < deadline {
                match client.heartbeat().await {
                    Ok(_) => {
                        println!("Hootenanny connected");
                        connected = true;
                        break;
                    }
                    Err(e) => {
                        println!("Heartbeat failed: {}, retrying...", e);
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
            connected
        } else {
            println!("No hootenanny client configured");
            false
        }
    };

    assert!(connected, "Should connect to hootenanny");
}

/// Test that route_tool returns working client
#[tokio::test]
#[ignore = "requires hootenanny running on localhost:5580"]
async fn test_backendpool_route_tool() {
    let mut pool = BackendPool::new();
    pool.setup_hootenanny(HOOTENANNY_ENDPOINT, TIMEOUT_MS).await;

    let client = pool.route_tool("any_tool").expect("Should route to hootenanny");
    let result = client.heartbeat().await;
    assert!(result.is_ok(), "Routed client heartbeat should work: {:?}", result);
}

/// Test BackendPool.request works
#[tokio::test]
#[ignore = "requires hootenanny running on localhost:5580"]
async fn test_backendpool_request() {
    use hooteproto::Payload;

    let mut pool = BackendPool::new();
    pool.setup_hootenanny(HOOTENANNY_ENDPOINT, TIMEOUT_MS).await;

    let result = pool.request(Payload::Ping).await;
    println!("Ping result: {:?}", result);
    assert!(result.is_ok(), "Ping should succeed: {:?}", result);

    if let Ok(Payload::Pong { worker_id, uptime_secs }) = result {
        println!("Pong from {} (uptime: {}s)", worker_id, uptime_secs);
    }
}
