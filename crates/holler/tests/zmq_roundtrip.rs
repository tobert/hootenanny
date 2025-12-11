//! ZMQ roundtrip tests for hooteproto using localhost TCP

use hooteproto::{Envelope, Payload};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;
use zeromq::{DealerSocket, RouterSocket, Socket, SocketRecv, SocketSend};

static PORT_COUNTER: AtomicU16 = AtomicU16::new(15570);

fn next_endpoint() -> String {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

/// Simple mock backend that responds to Ping with Pong
async fn mock_router(endpoint: &str) -> anyhow::Result<()> {
    let mut socket = RouterSocket::new();
    socket.bind(endpoint).await?;

    // Handle one request
    let msg = socket.recv().await?;

    // ROUTER sockets prepend identity frame
    let identity = msg.get(0).unwrap().to_vec();
    let payload_bytes = msg.get(1).unwrap();
    let payload_str = std::str::from_utf8(payload_bytes)?;
    let envelope: Envelope = serde_json::from_str(payload_str)?;

    // Create response based on request
    let response_payload = match envelope.payload {
        Payload::Ping => Payload::Pong {
            worker_id: Uuid::new_v4(),
            uptime_secs: 42,
        },
        Payload::LuaEval { code, .. } => Payload::Success {
            result: serde_json::json!({
                "evaluated": code,
                "result": "mock result"
            }),
        },
        Payload::JobStatus { job_id } => Payload::Success {
            result: serde_json::json!({
                "job_id": job_id,
                "status": "complete"
            }),
        },
        _ => Payload::Error {
            code: "not_implemented".to_string(),
            message: "Mock doesn't handle this payload type".to_string(),
            details: None,
        },
    };

    let response = Envelope {
        id: envelope.id,
        traceparent: envelope.traceparent,
        payload: response_payload,
    };

    let response_json = serde_json::to_string(&response)?;

    // Send response with identity frame
    let mut reply = zeromq::ZmqMessage::from(identity);
    reply.push_back(response_json.into_bytes().into());
    socket.send(reply).await?;

    Ok(())
}

#[tokio::test]
async fn test_ping_pong() {
    let endpoint = next_endpoint();

    // Start mock router
    let endpoint_clone = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        mock_router(&endpoint_clone).await.unwrap();
    });

    // Give router time to bind
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Connect dealer and send ping
    let mut dealer = DealerSocket::new();
    dealer.connect(&endpoint).await.unwrap();

    let envelope = Envelope::new(Payload::Ping);
    let json = serde_json::to_string(&envelope).unwrap();
    dealer
        .send(json.into_bytes().into())
        .await
        .unwrap();

    // Receive response
    let response = timeout(Duration::from_secs(1), dealer.recv())
        .await
        .unwrap()
        .unwrap();

    let response_str = std::str::from_utf8(response.get(0).unwrap()).unwrap();
    let response_envelope: Envelope = serde_json::from_str(response_str).unwrap();

    match response_envelope.payload {
        Payload::Pong { uptime_secs, .. } => {
            assert_eq!(uptime_secs, 42);
        }
        _ => panic!("Expected Pong"),
    }

    router_handle.await.unwrap();
}

#[tokio::test]
async fn test_lua_eval() {
    let endpoint = next_endpoint();

    let endpoint_clone = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        mock_router(&endpoint_clone).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    let mut dealer = DealerSocket::new();
    dealer.connect(&endpoint).await.unwrap();

    let envelope = Envelope::new(Payload::LuaEval {
        code: "return 1 + 1".to_string(),
        params: None,
    });
    let json = serde_json::to_string(&envelope).unwrap();
    dealer.send(json.into_bytes().into()).await.unwrap();

    let response = timeout(Duration::from_secs(1), dealer.recv())
        .await
        .unwrap()
        .unwrap();

    let response_str = std::str::from_utf8(response.get(0).unwrap()).unwrap();
    let response_envelope: Envelope = serde_json::from_str(response_str).unwrap();

    match response_envelope.payload {
        Payload::Success { result } => {
            assert_eq!(result["evaluated"], "return 1 + 1");
        }
        _ => panic!("Expected Success"),
    }

    router_handle.await.unwrap();
}

#[tokio::test]
async fn test_job_status() {
    let endpoint = next_endpoint();

    let endpoint_clone = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        mock_router(&endpoint_clone).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    let mut dealer = DealerSocket::new();
    dealer.connect(&endpoint).await.unwrap();

    let envelope = Envelope::new(Payload::JobStatus {
        job_id: "test-job-123".to_string(),
    });
    let json = serde_json::to_string(&envelope).unwrap();
    dealer.send(json.into_bytes().into()).await.unwrap();

    let response = timeout(Duration::from_secs(1), dealer.recv())
        .await
        .unwrap()
        .unwrap();

    let response_str = std::str::from_utf8(response.get(0).unwrap()).unwrap();
    let response_envelope: Envelope = serde_json::from_str(response_str).unwrap();

    match response_envelope.payload {
        Payload::Success { result } => {
            assert_eq!(result["job_id"], "test-job-123");
            assert_eq!(result["status"], "complete");
        }
        _ => panic!("Expected Success"),
    }

    router_handle.await.unwrap();
}
