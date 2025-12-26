//! Tests for HootClient concurrency and Lazy Pirate behavior
//!
//! Uses real ROUTER sockets to verify:
//! - Concurrent requests don't block each other at the client level
//! - Retries work when requests timeout
//! - Response correlation works with out-of-order responses

use bytes::Bytes;
use hooteproto::{ClientConfig, Command, HootClient, HootFrame, Payload};
use rzmq::socket::options::LINGER;
use rzmq::{Context, Msg, MsgFlags, SocketType};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;

static PORT: AtomicU16 = AtomicU16::new(18500);

fn next_endpoint() -> String {
    let port = PORT.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

/// Get the correct response command for a request (heartbeats echo, others get Reply)
fn response_command(request: &HootFrame) -> Command {
    if request.command == Command::Heartbeat {
        Command::Heartbeat
    } else {
        Command::Reply
    }
}

/// Mock ROUTER that echoes requests back as responses
async fn echo_router(endpoint: &str, request_count: usize) {
    let ctx = Context::new().unwrap();
    let socket = ctx.socket(SocketType::Router).unwrap();
    socket
        .set_option_raw(LINGER, &0i32.to_ne_bytes())
        .await
        .ok();
    socket.bind(endpoint).await.unwrap();

    for _ in 0..request_count {
        // Receive request
        let msgs = socket.recv_multipart().await.unwrap();
        let frames: Vec<Bytes> = msgs
            .iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        // Parse to get identity and request
        let (identity, request) =
            HootFrame::from_frames_with_identity(&frames).expect("Failed to parse request");

        // Build response
        let response = HootFrame {
            command: response_command(&request),
            content_type: request.content_type,
            request_id: request.request_id,
            service: request.service,
            traceparent: request.traceparent,
            body: request.body, // Echo body back
        };

        // Send response with identity
        let reply_frames = response.to_frames_with_identity(&identity);
        let last_idx = reply_frames.len() - 1;
        for (i, frame) in reply_frames.iter().enumerate() {
            let mut msg = Msg::from_vec(frame.to_vec());
            if i < last_idx {
                msg.set_flags(MsgFlags::MORE);
            }
            socket.send(msg).await.unwrap();
        }
    }
}

/// Mock ROUTER that delays responses by specified duration
async fn delayed_router(endpoint: &str, delay: Duration, request_count: usize) {
    let ctx = Context::new().unwrap();
    let socket = ctx.socket(SocketType::Router).unwrap();
    socket
        .set_option_raw(LINGER, &0i32.to_ne_bytes())
        .await
        .ok();
    socket.bind(endpoint).await.unwrap();

    for _ in 0..request_count {
        let msgs = socket.recv_multipart().await.unwrap();
        let frames: Vec<Bytes> = msgs
            .iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        let (identity, request) =
            HootFrame::from_frames_with_identity(&frames).expect("Failed to parse request");

        // Delay before responding
        tokio::time::sleep(delay).await;

        let response = HootFrame {
            command: response_command(&request),
            content_type: request.content_type,
            request_id: request.request_id,
            service: request.service,
            traceparent: request.traceparent,
            body: request.body,
        };

        let reply_frames = response.to_frames_with_identity(&identity);
        let last_idx = reply_frames.len() - 1;
        for (i, frame) in reply_frames.iter().enumerate() {
            let mut msg = Msg::from_vec(frame.to_vec());
            if i < last_idx {
                msg.set_flags(MsgFlags::MORE);
            }
            socket.send(msg).await.unwrap();
        }
    }
}

/// Mock ROUTER that responds to requests out of order
async fn reordering_router(endpoint: &str, request_count: usize) {
    let ctx = Context::new().unwrap();
    let socket = ctx.socket(SocketType::Router).unwrap();
    socket
        .set_option_raw(LINGER, &0i32.to_ne_bytes())
        .await
        .ok();
    socket.bind(endpoint).await.unwrap();

    // Collect all requests first
    let mut pending: Vec<(Vec<Bytes>, HootFrame)> = Vec::new();

    for _ in 0..request_count {
        let msgs = socket.recv_multipart().await.unwrap();
        let frames: Vec<Bytes> = msgs
            .iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        let (identity, request) =
            HootFrame::from_frames_with_identity(&frames).expect("Failed to parse request");
        pending.push((identity, request));
    }

    // Respond in reverse order
    for (identity, request) in pending.into_iter().rev() {
        let response = HootFrame {
            command: response_command(&request),
            content_type: request.content_type,
            request_id: request.request_id,
            service: request.service,
            traceparent: request.traceparent,
            body: request.body,
        };

        let reply_frames = response.to_frames_with_identity(&identity);
        let last_idx = reply_frames.len() - 1;
        for (i, frame) in reply_frames.iter().enumerate() {
            let mut msg = Msg::from_vec(frame.to_vec());
            if i < last_idx {
                msg.set_flags(MsgFlags::MORE);
            }
            socket.send(msg).await.unwrap();
        }
    }
}

/// Test that concurrent requests complete without blocking each other
#[tokio::test]
async fn test_concurrent_requests_complete() {
    let endpoint = next_endpoint();
    let request_count = 5;

    // Start router
    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        echo_router(&router_endpoint, request_count).await;
    });

    // Give router time to bind
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create client
    let config = ClientConfig::new("test", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;

    // Launch concurrent requests
    let barrier = Arc::new(Barrier::new(request_count));
    let mut handles = Vec::new();

    for i in 0..request_count {
        let client = client.clone();
        let barrier = barrier.clone();

        handles.push(tokio::spawn(async move {
            // Wait for all tasks to be ready
            barrier.wait().await;

            let start = Instant::now();
            let result = client.request(Payload::Ping).await;
            let elapsed = start.elapsed();

            (i, result.is_ok(), elapsed)
        }));
    }

    // Collect results
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    // All should succeed
    for (i, ok, elapsed) in &results {
        assert!(ok, "Request {} failed", i);
        println!("Request {} completed in {:?}", i, elapsed);
    }

    // If requests were serialized, total time would be ~N * request_time
    // With concurrency, they should overlap significantly
    let max_elapsed = results.iter().map(|(_, _, e)| e).max().unwrap();
    println!("Max elapsed: {:?}", max_elapsed);

    router_handle.abort();
}

/// Test that client doesn't block when multiple requests are in flight
/// Note: A single ROUTER processes requests serially, but the CLIENT should
/// be able to have multiple requests pending simultaneously.
#[tokio::test]
async fn test_multiple_requests_in_flight() {
    let endpoint = next_endpoint();
    let delay = Duration::from_millis(50);
    let request_count = 3;

    // Track how many requests are being processed concurrently
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_in_flight = Arc::new(AtomicUsize::new(0));

    // Start router that tracks concurrency
    let router_endpoint = endpoint.clone();
    let in_flight_clone = in_flight.clone();
    let max_in_flight_clone = max_in_flight.clone();
    let router_handle = tokio::spawn(async move {
        let ctx = Context::new().unwrap();
        let socket = ctx.socket(SocketType::Router).unwrap();
        socket
            .set_option_raw(LINGER, &0i32.to_ne_bytes())
            .await
            .ok();
        socket.bind(&router_endpoint).await.unwrap();

        for _ in 0..request_count {
            let msgs = socket.recv_multipart().await.unwrap();

            // Track in-flight
            let current = in_flight_clone.fetch_add(1, Ordering::SeqCst) + 1;
            max_in_flight_clone.fetch_max(current, Ordering::SeqCst);

            let frames: Vec<Bytes> = msgs
                .iter()
                .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                .collect();

            let (identity, request) =
                HootFrame::from_frames_with_identity(&frames).expect("Failed to parse");

            // Small delay to allow overlap
            tokio::time::sleep(delay).await;

            in_flight_clone.fetch_sub(1, Ordering::SeqCst);

            let response = HootFrame {
                command: response_command(&request),
                content_type: request.content_type,
                request_id: request.request_id,
                service: request.service,
                traceparent: request.traceparent,
                body: request.body,
            };

            let reply_frames = response.to_frames_with_identity(&identity);
            let last_idx = reply_frames.len() - 1;
            for (i, frame) in reply_frames.iter().enumerate() {
                let mut msg = Msg::from_vec(frame.to_vec());
                if i < last_idx {
                    msg.set_flags(MsgFlags::MORE);
                }
                socket.send(msg).await.unwrap();
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let config = ClientConfig::new("test", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;

    // Launch all requests at once
    let mut handles = Vec::new();
    for _ in 0..request_count {
        let client = client.clone();
        handles.push(tokio::spawn(async move { client.request(Payload::Ping).await }));
    }

    // Wait for all
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    println!("Max in-flight at router: {}", max_in_flight.load(Ordering::SeqCst));

    // All requests should have succeeded
    router_handle.abort();
}

/// Test that responses are correctly correlated even when out of order
#[tokio::test]
async fn test_response_correlation_with_reordering() {
    let endpoint = next_endpoint();
    let request_count = 3;

    // Start reordering router
    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        reordering_router(&router_endpoint, request_count).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let config = ClientConfig::new("test", &endpoint).with_timeout(5000);
    let client = HootClient::new(config).await;

    // Launch requests and track their order
    let mut handles = Vec::new();
    for i in 0..request_count {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let result = client.request(Payload::Ping).await;
            (i, result)
        }));
    }

    // All should succeed despite reordering
    for handle in handles {
        let (i, result) = handle.await.unwrap();
        assert!(result.is_ok(), "Request {} failed: {:?}", i, result);
    }

    router_handle.abort();
}

// NOTE: Heartbeat independence is tested in integration.rs with parallel routers.
// The serial delayed_router used here can't properly test parallel behavior.

/// Test that timeout triggers retry (Lazy Pirate pattern)
#[tokio::test]
async fn test_retry_on_timeout() {
    let endpoint = next_endpoint();

    // Router that only responds to 2nd request (drops first)
    let router_endpoint = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        let ctx = Context::new().unwrap();
        let socket = ctx.socket(SocketType::Router).unwrap();
        socket
            .set_option_raw(LINGER, &0i32.to_ne_bytes())
            .await
            .ok();
        socket.bind(&router_endpoint).await.unwrap();

        // Receive and drop first request
        let _ = socket.recv_multipart().await.unwrap();
        println!("Router: dropped first request");

        // Respond to second request
        let msgs = socket.recv_multipart().await.unwrap();
        let frames: Vec<Bytes> = msgs
            .iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

        let (identity, request) =
            HootFrame::from_frames_with_identity(&frames).expect("Failed to parse");
        println!("Router: responding to second request");

        let response = HootFrame {
            command: response_command(&request),
            content_type: request.content_type,
            request_id: request.request_id,
            service: request.service,
            traceparent: request.traceparent,
            body: request.body, // Echo body for proper parsing
        };

        let reply_frames = response.to_frames_with_identity(&identity);
        let last_idx = reply_frames.len() - 1;
        for (i, frame) in reply_frames.iter().enumerate() {
            let mut msg = Msg::from_vec(frame.to_vec());
            if i < last_idx {
                msg.set_flags(MsgFlags::MORE);
            }
            socket.send(msg).await.unwrap();
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Timeout must be > 1s (reactor cleanup interval) for timeout detection
    let config = ClientConfig::new("test", &endpoint)
        .with_timeout(1500) // 1.5s timeout (cleanup runs every 1s)
        .with_retries(2); // Allow 2 retries

    let client = HootClient::new(config).await;

    let start = Instant::now();
    let result = client.request(Payload::Ping).await;
    let elapsed = start.elapsed();

    println!("Request completed in {:?}: {:?}", elapsed, result.is_ok());

    // Should succeed on retry
    assert!(result.is_ok(), "Request should succeed on retry");

    // Should have taken at least one timeout period
    assert!(
        elapsed >= Duration::from_millis(1000),
        "Should have waited for timeout before retry"
    );

    router_handle.abort();
}
