//! ZMQ roundtrip tests for hooteproto using localhost TCP

use futures::{SinkExt, StreamExt};
use hooteproto::socket_config::{ZmqContext, Multipart};
use hooteproto::{Envelope, Payload, ResponseEnvelope};
use hooteproto::request::{JobStatusRequest, ToolRequest, WeaveEvalRequest};
use hooteproto::responses::{JobState, ToolResponse};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::time::timeout;
use tmq::{dealer, router};
use uuid::Uuid;

static PORT_COUNTER: AtomicU16 = AtomicU16::new(15570);

fn next_endpoint() -> String {
    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

/// Simple mock backend that responds to Ping with Pong
async fn mock_router(endpoint: &str) -> anyhow::Result<()> {
    let context = ZmqContext::new();
    let socket = router(&context)
        .set_linger(0)
        .bind(endpoint)?;

    let (mut tx, mut rx) = socket.split();

    // Handle one request
    let mp = rx.next().await.ok_or(anyhow::anyhow!("Stream ended"))??;
    let frames: Vec<Vec<u8>> = mp.into_iter().map(|m| m.to_vec()).collect();

    // ROUTER sockets prepend identity frame
    let identity = frames[0].clone();
    let payload_bytes = &frames[1];
    let payload_str = std::str::from_utf8(payload_bytes)?;
    let envelope: Envelope = serde_json::from_str(payload_str)?;

    // Create response based on request
    let response_payload = match envelope.payload {
        Payload::Ping => Payload::Pong {
            worker_id: Uuid::new_v4(),
            uptime_secs: 42,
        },
        Payload::ToolRequest(ToolRequest::WeaveEval(_)) => {
            Payload::TypedResponse(ResponseEnvelope::success(ToolResponse::WeaveEval(
                hooteproto::responses::WeaveEvalResponse {
                    output_type: hooteproto::responses::WeaveOutputType::Expression,
                    result: Some("mock result".to_string()),
                    stdout: None,
                    stderr: None,
                },
            )))
        }
        Payload::ToolRequest(ToolRequest::JobStatus(ref req)) => {
            Payload::TypedResponse(ResponseEnvelope::success(ToolResponse::JobStatus(
                hooteproto::responses::JobStatusResponse {
                    job_id: req.job_id.clone(),
                    status: JobState::Complete,
                    source: "mock".to_string(),
                    result: None,
                    error: None,
                    created_at: 0,
                    started_at: None,
                    completed_at: None,
                },
            )))
        }
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
    let reply: Multipart = vec![identity, response_json.into_bytes()].into();
    tx.send(reply).await?;

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
    let context = ZmqContext::new();
    let socket = dealer(&context)
        .set_linger(0)
        .connect(&endpoint)
        .unwrap();

    let (mut tx, mut rx) = socket.split();

    let envelope = Envelope::new(Payload::Ping);
    let json = serde_json::to_string(&envelope).unwrap();
    let mp: Multipart = vec![json.into_bytes()].into();
    tx.send(mp).await.unwrap();

    // Receive response
    let response_mp = timeout(Duration::from_secs(1), rx.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let response_bytes = response_mp.into_iter().next().unwrap();
    let response_str = std::str::from_utf8(&response_bytes).unwrap();
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
async fn test_weave_eval() {
    let endpoint = next_endpoint();

    let endpoint_clone = endpoint.clone();
    let router_handle = tokio::spawn(async move {
        mock_router(&endpoint_clone).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    let context = ZmqContext::new();
    let socket = dealer(&context)
        .set_linger(0)
        .connect(&endpoint)
        .unwrap();

    let (mut tx, mut rx) = socket.split();

    let envelope = Envelope::new(Payload::ToolRequest(ToolRequest::WeaveEval(
        WeaveEvalRequest {
            code: "print('hello')".to_string(),
        },
    )));
    let json = serde_json::to_string(&envelope).unwrap();
    let mp: Multipart = vec![json.into_bytes()].into();
    tx.send(mp).await.unwrap();

    let response_mp = timeout(Duration::from_secs(1), rx.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let response_bytes = response_mp.into_iter().next().unwrap();
    let response_str = std::str::from_utf8(&response_bytes).unwrap();
    let response_envelope: Envelope = serde_json::from_str(response_str).unwrap();

    match response_envelope.payload {
        Payload::TypedResponse(envelope) => {
            let result = envelope.to_json();
            assert_eq!(result["kind"], "success");
            assert_eq!(result["response"]["type"], "weave_eval");
            assert_eq!(result["response"]["result"], "mock result");
        }
        _ => panic!("Expected TypedResponse"),
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

    let context = ZmqContext::new();
    let socket = dealer(&context)
        .set_linger(0)
        .connect(&endpoint)
        .unwrap();

    let (mut tx, mut rx) = socket.split();

    let envelope = Envelope::new(Payload::ToolRequest(ToolRequest::JobStatus(
        JobStatusRequest {
            job_id: "test-job-123".to_string(),
        },
    )));
    let json = serde_json::to_string(&envelope).unwrap();
    let mp: Multipart = vec![json.into_bytes()].into();
    tx.send(mp).await.unwrap();

    let response_mp = timeout(Duration::from_secs(1), rx.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let response_bytes = response_mp.into_iter().next().unwrap();
    let response_str = std::str::from_utf8(&response_bytes).unwrap();
    let response_envelope: Envelope = serde_json::from_str(response_str).unwrap();

    match response_envelope.payload {
        Payload::TypedResponse(envelope) => {
            let result = envelope.to_json();
            assert_eq!(result["kind"], "success");
            assert_eq!(result["response"]["type"], "job_status");
            assert_eq!(result["response"]["job_id"], "test-job-123");
            assert_eq!(result["response"]["status"], "complete");
        }
        _ => panic!("Expected TypedResponse"),
    }

    router_handle.await.unwrap();
}
