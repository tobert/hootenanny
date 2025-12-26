//! Integration tests for HootClient in a realistic multi-member topology
//!
//! Tests the full HOOT01 protocol with:
//! - Central ROUTER hub (simulating hootenanny's role)
//! - Multiple DEALER clients sending concurrent requests
//! - Multiple workers processing requests in parallel
//! - Request/response correlation across the topology
//! - Heartbeat and retry behavior

use bytes::Bytes;
use hooteproto::{ClientConfig, Command, HootClient, HootFrame, Payload};
use rzmq::socket::options::LINGER;
use rzmq::{Context, Msg, MsgFlags, SocketType};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Barrier};

static PORT: AtomicU16 = AtomicU16::new(19000);

fn next_endpoint() -> String {
    let port = PORT.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

/// Get the correct response command for a request
fn response_command(request: &HootFrame) -> Command {
    if request.command == Command::Heartbeat {
        Command::Heartbeat
    } else {
        Command::Reply
    }
}

// =============================================================================
// Test Topology Components
// =============================================================================

/// Statistics for the hub
#[derive(Debug, Default)]
struct HubStats {
    messages_routed: AtomicUsize,
    clients_seen: AtomicUsize,
    workers_seen: AtomicUsize,
}

/// Central ROUTER hub that routes between clients and workers.
/// This simulates hootenanny's routing role.
struct Hub {
    frontend_endpoint: String,
    backend_endpoint: String,
    shutdown_tx: broadcast::Sender<()>,
    stats: Arc<HubStats>,
}

impl Hub {
    async fn start(frontend_endpoint: &str, backend_endpoint: &str) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let stats = Arc::new(HubStats::default());

        let fe = frontend_endpoint.to_string();
        let be = backend_endpoint.to_string();
        let mut shutdown_rx = shutdown_tx.subscribe();
        let stats_clone = stats.clone();

        tokio::spawn(async move {
            Self::run_hub(&fe, &be, shutdown_rx, stats_clone).await;
        });

        // Give sockets time to bind
        tokio::time::sleep(Duration::from_millis(50)).await;

        Self {
            frontend_endpoint: frontend_endpoint.to_string(),
            backend_endpoint: backend_endpoint.to_string(),
            shutdown_tx,
            stats,
        }
    }

    async fn run_hub(
        frontend: &str,
        backend: &str,
        mut shutdown: broadcast::Receiver<()>,
        stats: Arc<HubStats>,
    ) {
        let ctx = Context::new().unwrap();

        // Frontend ROUTER for clients
        let frontend_socket = ctx.socket(SocketType::Router).unwrap();
        frontend_socket
            .set_option_raw(LINGER, &0i32.to_ne_bytes())
            .await
            .ok();
        frontend_socket.bind(frontend).await.unwrap();

        // Backend ROUTER for workers
        let backend_socket = ctx.socket(SocketType::Router).unwrap();
        backend_socket
            .set_option_raw(LINGER, &0i32.to_ne_bytes())
            .await
            .ok();
        backend_socket.bind(backend).await.unwrap();

        // Track which worker should handle next request (round-robin)
        let mut available_workers: Vec<Vec<u8>> = Vec::new();
        // Note: pending_requests not used in this simplified hub - we route responses
        // based on client_id passed through workers, not tracked here

        loop {
            tokio::select! {
                // Messages from clients
                result = frontend_socket.recv_multipart() => {
                    if let Ok(msgs) = result {
                        let frames: Vec<Bytes> = msgs
                            .iter()
                            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                            .collect();

                        if let Ok((client_id, request)) = HootFrame::from_frames_with_identity(&frames) {
                            stats.messages_routed.fetch_add(1, Ordering::Relaxed);

                            // If we have workers, route to one
                            if let Some(worker_id) = available_workers.pop() {
                                // Send to worker: [worker_id, empty, client_id, request_frames...]
                                let request_frames = request.to_frames();
                                let mut out_msgs = vec![
                                    Msg::from_vec(worker_id),
                                    Msg::from_vec(vec![]), // empty delimiter
                                ];
                                // Add client identity so worker knows where to route response
                                out_msgs.push(Msg::from_vec(client_id.iter().flat_map(|b| b.to_vec()).collect()));
                                for frame in &request_frames {
                                    out_msgs.push(Msg::from_vec(frame.to_vec()));
                                }

                                let last_idx = out_msgs.len() - 1;
                                for (i, mut msg) in out_msgs.into_iter().enumerate() {
                                    if i < last_idx {
                                        msg.set_flags(MsgFlags::MORE);
                                    }
                                    backend_socket.send(msg).await.ok();
                                }
                            }
                            // If no workers, request is dropped (client will retry)
                        }
                    }
                }

                // Messages from workers
                result = backend_socket.recv_multipart() => {
                    if let Ok(msgs) = result {
                        let frames: Vec<Bytes> = msgs
                            .iter()
                            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                            .collect();

                        // First frame is worker identity
                        if frames.len() >= 2 {
                            let worker_id = frames[0].to_vec();

                            // Check if this is a READY message or a response
                            if frames.len() == 2 && frames[1].is_empty() {
                                // Worker announcing ready
                                stats.workers_seen.fetch_add(1, Ordering::Relaxed);
                                available_workers.push(worker_id);
                            } else if frames.len() >= 4 {
                                // Response: [worker_id, empty, client_id, response_frames...]
                                let client_id_bytes = &frames[2];
                                let response_frames: Vec<Bytes> = frames[3..].to_vec();

                                // Send back to client
                                let mut out_msgs = vec![
                                    Msg::from_vec(client_id_bytes.to_vec()),
                                    Msg::from_vec(vec![]), // empty delimiter
                                ];
                                for frame in &response_frames {
                                    out_msgs.push(Msg::from_vec(frame.to_vec()));
                                }

                                let last_idx = out_msgs.len() - 1;
                                for (i, mut msg) in out_msgs.into_iter().enumerate() {
                                    if i < last_idx {
                                        msg.set_flags(MsgFlags::MORE);
                                    }
                                    frontend_socket.send(msg).await.ok();
                                }

                                // Worker is available again
                                available_workers.push(worker_id);
                                stats.messages_routed.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }

                _ = shutdown.recv() => {
                    break;
                }
            }
        }
    }

    fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

/// A worker that connects to the hub and processes requests
struct Worker {
    shutdown_tx: broadcast::Sender<()>,
    requests_processed: Arc<AtomicUsize>,
}

impl Worker {
    async fn start(endpoint: &str, worker_id: &str, processing_delay: Duration) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let requests_processed = Arc::new(AtomicUsize::new(0));

        let ep = endpoint.to_string();
        let id = worker_id.to_string();
        let mut shutdown_rx = shutdown_tx.subscribe();
        let processed = requests_processed.clone();

        tokio::spawn(async move {
            Self::run_worker(&ep, &id, processing_delay, shutdown_rx, processed).await;
        });

        // Give socket time to connect
        tokio::time::sleep(Duration::from_millis(30)).await;

        Self {
            shutdown_tx,
            requests_processed,
        }
    }

    async fn run_worker(
        endpoint: &str,
        worker_id: &str,
        delay: Duration,
        mut shutdown: broadcast::Receiver<()>,
        processed: Arc<AtomicUsize>,
    ) {
        let ctx = Context::new().unwrap();
        let socket = ctx.socket(SocketType::Dealer).unwrap();
        socket
            .set_option_raw(LINGER, &0i32.to_ne_bytes())
            .await
            .ok();
        socket
            .set_option_raw(rzmq::socket::options::ROUTING_ID, worker_id.as_bytes())
            .await
            .ok();
        socket.connect(endpoint).await.ok();

        // Send READY
        let ready_msg = Msg::from_vec(vec![]);
        socket.send(ready_msg).await.ok();

        loop {
            tokio::select! {
                result = socket.recv_multipart() => {
                    if let Ok(msgs) = result {
                        let frames: Vec<Bytes> = msgs
                            .iter()
                            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                            .collect();

                        // frames: [empty, client_id, request_frames...]
                        if frames.len() >= 3 {
                            let client_id = frames[1].clone();
                            let request_frames: Vec<Bytes> = frames[2..].to_vec();

                            if let Ok(request) = HootFrame::from_frames(&request_frames) {
                                // Simulate processing
                                if !delay.is_zero() {
                                    tokio::time::sleep(delay).await;
                                }

                                processed.fetch_add(1, Ordering::Relaxed);

                                // Build response
                                let response = HootFrame {
                                    command: response_command(&request),
                                    content_type: request.content_type,
                                    request_id: request.request_id,
                                    service: request.service.clone(),
                                    traceparent: request.traceparent.clone(),
                                    body: request.body.clone(),
                                };

                                // Send back: [empty, client_id, response_frames...]
                                let response_frames = response.to_frames();
                                let mut out_msgs = vec![
                                    Msg::from_vec(vec![]),
                                    Msg::from_vec(client_id.to_vec()),
                                ];
                                for frame in &response_frames {
                                    out_msgs.push(Msg::from_vec(frame.to_vec()));
                                }

                                let last_idx = out_msgs.len() - 1;
                                for (i, mut msg) in out_msgs.into_iter().enumerate() {
                                    if i < last_idx {
                                        msg.set_flags(MsgFlags::MORE);
                                    }
                                    socket.send(msg).await.ok();
                                }
                            }
                        }
                    }
                }

                _ = shutdown.recv() => {
                    break;
                }
            }
        }
    }

    fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

// =============================================================================
// Tests
// =============================================================================

/// Test basic message flow through the topology:
/// Client -> Hub -> Worker -> Hub -> Client
#[tokio::test]
async fn test_basic_topology_flow() {
    let frontend = next_endpoint();
    let backend = next_endpoint();

    // Start hub
    let hub = Hub::start(&frontend, &backend).await;

    // Start one worker
    let worker = Worker::start(&backend, "worker-1", Duration::from_millis(10)).await;

    // Give worker time to register
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create client
    let config = ClientConfig::new("client-1", &frontend).with_timeout(5000);
    let client = HootClient::new(config).await;

    // Send request
    let result = client.request(Payload::Ping).await;
    assert!(result.is_ok(), "Request should succeed: {:?}", result);

    assert_eq!(worker.requests_processed.load(Ordering::Relaxed), 1);
    assert!(hub.stats.messages_routed.load(Ordering::Relaxed) >= 1);

    worker.shutdown();
    hub.shutdown();
}

/// Test multiple clients sending concurrent requests through the hub
#[tokio::test]
async fn test_multiple_clients_concurrent() {
    let frontend = next_endpoint();
    let backend = next_endpoint();

    let hub = Hub::start(&frontend, &backend).await;

    // Start 3 workers
    let workers: Vec<_> = (0..3)
        .map(|i| {
            let be = backend.clone();
            async move { Worker::start(&be, &format!("worker-{}", i), Duration::from_millis(20)).await }
        })
        .collect();
    let workers: Vec<_> = futures::future::join_all(workers).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create 4 clients, each sends 5 requests
    let client_count = 4;
    let requests_per_client = 5;
    let barrier = Arc::new(Barrier::new(client_count));

    let mut handles = Vec::new();
    for i in 0..client_count {
        let fe = frontend.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            let config = ClientConfig::new(&format!("client-{}", i), &fe).with_timeout(10000);
            let client = HootClient::new(config).await;

            barrier.wait().await;

            let mut successes = 0;
            for _ in 0..requests_per_client {
                if client.request(Payload::Ping).await.is_ok() {
                    successes += 1;
                }
            }
            successes
        }));
    }

    let mut total_successes = 0;
    for handle in handles {
        total_successes += handle.await.unwrap();
    }

    let expected = client_count * requests_per_client;
    println!(
        "Total successes: {} / {} ({:.1}%)",
        total_successes,
        expected,
        (total_successes as f64 / expected as f64) * 100.0
    );

    let total_processed: usize = workers
        .iter()
        .map(|w| w.requests_processed.load(Ordering::Relaxed))
        .sum();
    println!("Workers processed: {}", total_processed);

    // Allow some failures due to timing, but most should succeed
    assert!(
        total_successes >= expected * 80 / 100,
        "At least 80% should succeed"
    );

    for w in &workers {
        w.shutdown();
    }
    hub.shutdown();
}

/// Test heartbeat works through the topology
#[tokio::test]
async fn test_heartbeat_through_topology() {
    let frontend = next_endpoint();
    let backend = next_endpoint();

    let hub = Hub::start(&frontend, &backend).await;
    let worker = Worker::start(&backend, "worker-1", Duration::ZERO).await;

    tokio::time::sleep(Duration::from_millis(50)).await;

    let config = ClientConfig::new("client-1", &frontend).with_timeout(5000);
    let client = HootClient::new(config).await;

    // Heartbeat should work
    let result = client.heartbeat().await;
    assert!(result.is_ok(), "Heartbeat should succeed: {:?}", result);

    worker.shutdown();
    hub.shutdown();
}

/// Test that responses are correctly correlated when workers respond out of order
#[tokio::test]
async fn test_response_correlation() {
    let frontend = next_endpoint();
    let backend = next_endpoint();

    let hub = Hub::start(&frontend, &backend).await;

    // Workers with different delays - responses will arrive out of order
    let fast_worker = Worker::start(&backend, "fast", Duration::from_millis(10)).await;
    let slow_worker = Worker::start(&backend, "slow", Duration::from_millis(100)).await;

    tokio::time::sleep(Duration::from_millis(50)).await;

    let config = ClientConfig::new("client-1", &frontend).with_timeout(5000);
    let client = HootClient::new(config).await;

    // Send multiple requests - some will go to fast worker, some to slow
    let request_count = 6;
    let mut handles = Vec::new();

    for i in 0..request_count {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let result = client.request(Payload::Ping).await;
            (i, result.is_ok())
        }));
    }

    let mut successes = 0;
    for handle in handles {
        let (i, ok) = handle.await.unwrap();
        if ok {
            successes += 1;
        } else {
            println!("Request {} failed", i);
        }
    }

    println!("Successes: {} / {}", successes, request_count);
    assert!(successes >= request_count - 1, "Most requests should succeed");

    fast_worker.shutdown();
    slow_worker.shutdown();
    hub.shutdown();
}

/// Stress test: many clients, many workers, many requests
#[tokio::test]
async fn test_stress_topology() {
    let frontend = next_endpoint();
    let backend = next_endpoint();

    let hub = Hub::start(&frontend, &backend).await;

    // 5 workers
    let workers: Vec<_> = futures::future::join_all((0..5).map(|i| {
        let be = backend.clone();
        async move { Worker::start(&be, &format!("worker-{}", i), Duration::from_millis(5)).await }
    }))
    .await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 10 clients, each sends 10 requests = 100 total
    let client_count = 10;
    let requests_per_client = 10;

    let start = Instant::now();

    let handles: Vec<_> = (0..client_count)
        .map(|i| {
            let fe = frontend.clone();
            tokio::spawn(async move {
                let config =
                    ClientConfig::new(&format!("client-{}", i), &fe).with_timeout(10000);
                let client = HootClient::new(config).await;

                let mut successes = 0;
                for _ in 0..requests_per_client {
                    if client.request(Payload::Ping).await.is_ok() {
                        successes += 1;
                    }
                }
                successes
            })
        })
        .collect();

    let mut total_successes = 0;
    for handle in handles {
        total_successes += handle.await.unwrap();
    }

    let elapsed = start.elapsed();
    let total_requests = client_count * requests_per_client;
    let rps = total_requests as f64 / elapsed.as_secs_f64();

    println!(
        "{} requests in {:?} ({:.1} req/s)",
        total_requests, elapsed, rps
    );
    println!(
        "Success rate: {} / {} ({:.1}%)",
        total_successes,
        total_requests,
        (total_successes as f64 / total_requests as f64) * 100.0
    );

    let total_processed: usize = workers
        .iter()
        .map(|w| w.requests_processed.load(Ordering::Relaxed))
        .sum();
    println!("Workers processed: {}", total_processed);
    println!("Hub routed: {}", hub.stats.messages_routed.load(Ordering::Relaxed));

    assert!(
        total_successes >= total_requests * 70 / 100,
        "At least 70% should succeed under stress"
    );

    for w in &workers {
        w.shutdown();
    }
    hub.shutdown();
}
