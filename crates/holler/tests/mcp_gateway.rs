//! Integration test for MCP gateway → ZMQ backend flow
//!
//! Tests holler serve receiving MCP requests and routing to a mock ZMQ backend.

use futures::{SinkExt, StreamExt};
use hooteproto::request::{CasInspectRequest, ToolRequest};
use hooteproto::responses::ToolResponse;
use hooteproto::socket_config::{ZmqContext, Multipart};
use hooteproto::{Envelope, Payload, ResponseEnvelope};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tmq::{dealer, router};

static ZMQ_PORT: AtomicU16 = AtomicU16::new(25580);

fn next_zmq_endpoint() -> String {
    let port = ZMQ_PORT.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

#[tokio::test]
async fn test_mcp_tool_call_with_traceparent() {
    let zmq_endpoint = next_zmq_endpoint();

    // Start mock hootenanny backend
    let zmq_endpoint_clone = zmq_endpoint.clone();
    let backend_handle = tokio::spawn(async move {
        // This backend will check for traceparent in the envelope
        let context = ZmqContext::new();
        let socket = router(&context)
            .set_linger(0)
            .bind(&zmq_endpoint_clone)
            .unwrap();

        let (mut tx, mut rx) = socket.split();

        let mp = rx.next().await.unwrap().unwrap();
        let frames: Vec<Vec<u8>> = mp.into_iter().map(|m| m.to_vec()).collect();

        let identity = frames[0].clone();
        let payload_bytes = &frames[1];
        let envelope: Envelope =
            serde_json::from_str(std::str::from_utf8(payload_bytes).unwrap()).unwrap();

        // Verify traceparent was propagated
        assert!(
            envelope.traceparent.is_some(),
            "Expected traceparent to be set"
        );
        let tp = envelope.traceparent.as_ref().unwrap();
        assert!(
            tp.starts_with("00-"),
            "Traceparent should start with version 00"
        );

        let response = Envelope {
            id: envelope.id,
            traceparent: envelope.traceparent,
            payload: Payload::TypedResponse(ResponseEnvelope::success(ToolResponse::ack(
                "traced",
            ))),
        };

        let response_json = serde_json::to_string(&response).unwrap();
        let reply: Multipart = vec![identity, response_json.into_bytes()].into();
        tx.send(reply).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Simulate holler sending a request with traceparent
    let holler_handle = tokio::spawn(async move {
        let context = ZmqContext::new();
        let socket = dealer(&context)
            .set_linger(0)
            .connect(&zmq_endpoint)
            .unwrap();

        let (mut tx, mut rx) = socket.split();

        // Create envelope with traceparent (simulating what holler does)
        let envelope = Envelope::new(Payload::ToolRequest(ToolRequest::CasInspect(
            CasInspectRequest {
                hash: "abc123".to_string(),
            },
        )))
        .with_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");

        let json = serde_json::to_string(&envelope).unwrap();
        let mp: Multipart = vec![json.into_bytes()].into();
        tx.send(mp).await.unwrap();

        let response_mp = tokio::time::timeout(Duration::from_secs(2), rx.next())
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
                // ResponseEnvelope::Success has { "kind": "success", "response": { "type": "ack", "message": "traced" } }
                assert_eq!(result["kind"], "success");
                assert_eq!(result["response"]["type"], "ack");
                assert_eq!(result["response"]["message"], "traced");
            }
            other => panic!("Expected TypedResponse, got {:?}", other),
        }
    });

    let (backend_result, holler_result) = tokio::join!(backend_handle, holler_handle);
    backend_result.unwrap();
    holler_result.unwrap();
}
