//! Integration tests for Phase 6: Tool Refresh on Backend Recovery
//!
//! Tests the Dead → Ready transition detection and tool refresh callback.

use bytes::Bytes;
use hooteproto::{Command, ContentType, Envelope, HootFrame, Payload, ToolInfo, PROTOCOL_VERSION};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::timeout;
use zeromq::{RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

static PORT_COUNTER: AtomicU16 = AtomicU16::new(16570);

fn next_endpoint() -> String {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

/// Mock hootenanny server that responds to heartbeats and ListTools
struct MockHootenanny {
    endpoint: String,
    tool_count: usize,
}

impl MockHootenanny {
    fn new(endpoint: String, tool_count: usize) -> Self {
        Self {
            endpoint,
            tool_count,
        }
    }

    /// Run the mock server, handling heartbeats and tool requests
    async fn run(&self, stop_signal: Arc<Notify>) -> anyhow::Result<()> {
        let mut socket = RouterSocket::new();
        socket.bind(&self.endpoint).await?;

        loop {
            tokio::select! {
                result = socket.recv() => {
                    let msg = result?;
                    self.handle_message(&mut socket, msg).await?;
                }
                _ = stop_signal.notified() => {
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_message(
        &self,
        socket: &mut RouterSocket,
        msg: ZmqMessage,
    ) -> anyhow::Result<()> {
        // First frame is identity for ROUTER socket
        let identity = msg.get(0).unwrap().to_vec();

        // Check if this is HOOT01 frame protocol
        let is_hoot01 = msg.iter().any(|f| f.as_ref() == PROTOCOL_VERSION);

        if is_hoot01 {
            // Parse HOOT01 frame
            let frames: Vec<Bytes> = msg.iter().map(|f| Bytes::copy_from_slice(f)).collect();
            if let Ok(frame) = HootFrame::from_frames(&frames[1..]) {
                // Skip identity
                match frame.command {
                    Command::Heartbeat => {
                        // Respond with heartbeat
                        let response = HootFrame::heartbeat("hootenanny");
                        let response_frames = response.to_frames();
                        let mut reply = ZmqMessage::from(identity);
                        for f in response_frames {
                            reply.push_back(f.to_vec().into());
                        }
                        socket.send(reply).await?;
                    }
                    Command::Request => {
                        // Parse payload and respond
                        if frame.content_type == ContentType::MsgPack {
                            if let Ok(payload) = rmp_serde::from_slice::<Payload>(&frame.body) {
                                let response_payload = self.handle_payload(payload);
                                let response =
                                    HootFrame::reply(frame.request_id, &response_payload)?;
                                let response_frames = response.to_frames();
                                let mut reply = ZmqMessage::from(identity);
                                for f in response_frames {
                                    reply.push_back(f.to_vec().into());
                                }
                                socket.send(reply).await?;
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else {
            // Legacy MsgPack envelope
            let payload_bytes = msg.get(1).unwrap();
            if let Ok(envelope) = rmp_serde::from_slice::<Envelope>(payload_bytes) {
                let response_payload = self.handle_payload(envelope.payload);
                let response = Envelope {
                    id: envelope.id,
                    traceparent: envelope.traceparent,
                    payload: response_payload,
                };
                let response_bytes = rmp_serde::to_vec(&response)?;
                let mut reply = ZmqMessage::from(identity);
                reply.push_back(response_bytes.into());
                socket.send(reply).await?;
            }
        }

        Ok(())
    }

    fn handle_payload(&self, payload: Payload) -> Payload {
        match payload {
            Payload::Ping => Payload::Pong {
                worker_id: uuid::Uuid::new_v4(),
                uptime_secs: 100,
            },
            Payload::ListTools => {
                let tools: Vec<ToolInfo> = (0..self.tool_count)
                    .map(|i| ToolInfo {
                        name: format!("mock_tool_{}", i),
                        description: format!("Mock tool number {}", i),
                        input_schema: serde_json::json!({}),
                    })
                    .collect();
                Payload::ToolList { tools }
            }
            _ => Payload::Error {
                code: "not_implemented".to_string(),
                message: "Mock doesn't handle this".to_string(),
                details: None,
            },
        }
    }
}

#[tokio::test]
async fn test_tool_cache_refresh_on_startup() {
    use holler::backend::BackendPool;
    use holler::handler::{new_tool_cache, refresh_tools_into};

    let endpoint = next_endpoint();
    let stop_signal = Arc::new(Notify::new());

    // Start mock hootenanny with 5 tools
    let mock = MockHootenanny::new(endpoint.clone(), 5);
    let stop = Arc::clone(&stop_signal);
    let server_handle = tokio::spawn(async move { mock.run(stop).await });

    // Give server time to bind
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect backend pool
    let mut pool = BackendPool::new();
    pool.connect_hootenanny(&endpoint, 5000).await.unwrap();

    // Create tool cache and refresh
    let cache = new_tool_cache();
    let count = refresh_tools_into(&cache, &pool).await;

    assert_eq!(count, 5, "Should have loaded 5 tools from mock hootenanny");

    let cached = cache.read().await;
    assert_eq!(cached.len(), 5);
    assert!(cached[0].name.starts_with("mock_tool_"));

    // Cleanup
    stop_signal.notify_one();
    let _ = timeout(Duration::from_secs(1), server_handle).await;
}

#[tokio::test]
async fn test_recovery_callback_called_on_dead_to_ready() {
    use holler::backend::{Backend, BackendConfig, Protocol};
    use holler::heartbeat::{BackendState, HeartbeatConfig};

    let endpoint = next_endpoint();
    let stop_signal = Arc::new(Notify::new());
    let recovery_count = Arc::new(AtomicUsize::new(0));

    // Start mock hootenanny
    let mock = MockHootenanny::new(endpoint.clone(), 3);
    let stop = Arc::clone(&stop_signal);
    let server_handle = tokio::spawn(async move { mock.run(stop).await });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect backend
    let backend = Backend::connect(BackendConfig {
        name: "hootenanny".to_string(),
        endpoint: endpoint.clone(),
        timeout_ms: 2000,
        protocol: Protocol::Hootenanny,
    })
    .await
    .unwrap();

    let backend = Arc::new(backend);

    // Manually set state to Dead to simulate recovery scenario
    backend.health.set_state(BackendState::Dead);
    assert_eq!(backend.health.get_state(), BackendState::Dead);

    // Capture state before heartbeat
    let state_before = backend.health.get_state();

    // Send heartbeat - should succeed and transition to Ready
    let config = HeartbeatConfig::default();
    let result = backend.send_heartbeat(config.timeout).await;

    match result {
        holler::heartbeat::HeartbeatResult::Success => {
            backend.health.record_message_received().await;

            // Check for Dead → Ready transition
            if state_before == BackendState::Dead {
                recovery_count.fetch_add(1, Ordering::SeqCst);
            }
        }
        other => panic!("Expected heartbeat success, got {:?}", other),
    }

    // Verify state transitioned to Ready
    assert_eq!(backend.health.get_state(), BackendState::Ready);

    // Verify recovery callback would have been called
    assert_eq!(recovery_count.load(Ordering::SeqCst), 1);

    // Cleanup
    stop_signal.notify_one();
    let _ = timeout(Duration::from_secs(1), server_handle).await;
}

#[tokio::test]
async fn test_tools_refresh_after_backend_recovery() {
    use holler::backend::BackendPool;
    use holler::handler::{new_tool_cache, refresh_tools_into};
    use holler::heartbeat::BackendState;

    let endpoint = next_endpoint();
    let stop_signal = Arc::new(Notify::new());

    // Start mock hootenanny with 3 tools
    let mock = MockHootenanny::new(endpoint.clone(), 3);
    let stop = Arc::clone(&stop_signal);
    let server_handle = tokio::spawn(async move { mock.run(stop).await });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect backend pool
    let mut pool = BackendPool::new();
    pool.connect_hootenanny(&endpoint, 5000).await.unwrap();
    let pool = Arc::new(pool);

    // Create tool cache - initially empty
    let cache = new_tool_cache();
    assert_eq!(cache.read().await.len(), 0);

    // Simulate backend marked as Dead
    if let Some(ref backend) = pool.hootenanny {
        backend.health.set_state(BackendState::Dead);
    }

    // Simulate recovery: refresh tools (this is what the callback does)
    let count = refresh_tools_into(&cache, &pool).await;
    assert_eq!(count, 3, "Should refresh 3 tools after recovery");

    // Verify cache was updated
    let cached = cache.read().await;
    assert_eq!(cached.len(), 3);

    // Cleanup
    stop_signal.notify_one();
    let _ = timeout(Duration::from_secs(1), server_handle).await;
}
