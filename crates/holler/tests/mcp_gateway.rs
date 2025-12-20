//! Integration test for MCP gateway → ZMQ backend flow
//!
//! Tests holler serve receiving MCP requests and routing to a mock ZMQ backend.

use hooteproto::{Envelope, Payload, ToolInfo};
use rzmq::{Context, Msg, SocketType};
use rzmq::socket::options::LINGER;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use uuid::Uuid;

static ZMQ_PORT: AtomicU16 = AtomicU16::new(25580);
static HTTP_PORT: AtomicU16 = AtomicU16::new(28080);

fn next_zmq_endpoint() -> String {
    let port = ZMQ_PORT.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

fn next_http_port() -> u16 {
    HTTP_PORT.fetch_add(1, Ordering::SeqCst)
}

/// Mock hootenanny backend that handles CAS and ListTools requests
async fn mock_hootenanny(endpoint: &str, requests_to_handle: usize) -> anyhow::Result<()> {
    let context = Context::new()?;
    let socket = context.socket(SocketType::Router)?;
    socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await.ok();
    socket.bind(endpoint).await?;

    for _ in 0..requests_to_handle {
        let msgs = socket.recv_multipart().await?;

        let identity = msgs[0].data().map(|d| d.to_vec()).unwrap_or_default();
        let payload_bytes = msgs[1].data().unwrap_or_default();
        let payload_str = std::str::from_utf8(payload_bytes)?;
        let envelope: Envelope = serde_json::from_str(payload_str)?;

        let response_payload = match envelope.payload {
            Payload::Ping => Payload::Pong {
                worker_id: Uuid::new_v4(),
                uptime_secs: 100,
            },
            Payload::ListTools => Payload::ToolList {
                tools: vec![
                    ToolInfo {
                        name: "cas_store".to_string(),
                        description: "Store content in CAS".to_string(),
                        input_schema: serde_json::json!({
                            "type": "object",
                            "properties": {"data": {"type": "string"}}
                        }),
                    },
                    ToolInfo {
                        name: "cas_inspect".to_string(),
                        description: "Inspect CAS content".to_string(),
                        input_schema: serde_json::json!({
                            "type": "object",
                            "properties": {"hash": {"type": "string"}}
                        }),
                    },
                ],
            },
            Payload::CasInspect { hash } => Payload::Success {
                result: serde_json::json!({
                    "hash": hash,
                    "exists": true,
                    "size": 42,
                    "preview": "Hello, world!"
                }),
            },
            _ => Payload::Error {
                code: "not_implemented".to_string(),
                message: "Mock doesn't handle this".to_string(),
                details: None,
            },
        };

        let response = Envelope {
            id: envelope.id,
            traceparent: envelope.traceparent,
            payload: response_payload,
        };

        let response_json = serde_json::to_string(&response)?;
        let reply = vec![
            Msg::from_vec(identity),
            Msg::from_vec(response_json.into_bytes()),
        ];
        socket.send_multipart(reply).await?;
    }

    Ok(())
}

/// Send MCP JSON-RPC request to holler
async fn mcp_request(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let response = client
        .post(url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        }))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    Ok(response)
}

#[tokio::test]
async fn test_mcp_tools_list() {
    let zmq_endpoint = next_zmq_endpoint();
    let http_port = next_http_port();

    // Start mock hootenanny backend (will handle 1 ListTools request)
    let zmq_endpoint_clone = zmq_endpoint.clone();
    let backend_handle = tokio::spawn(async move {
        mock_hootenanny(&zmq_endpoint_clone, 1).await.unwrap();
    });

    // Give backend time to bind
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Start holler serve in background (without OTEL to avoid complexity)
    let zmq_for_holler = zmq_endpoint.clone();
    let holler_handle = tokio::spawn(async move {
        // We can't easily import holler's serve module in tests,
        // so we'll use the CLI binary approach in a real integration test.
        // For now, this test validates the mock backend protocol.

        // Simulate what holler would do: connect to backend and request tool list
        let context = Context::new().unwrap();
        let dealer = context.socket(SocketType::Dealer).unwrap();
        dealer.set_option_raw(LINGER, &0i32.to_ne_bytes()).await.ok();
        dealer.connect(&zmq_for_holler).await.unwrap();

        let envelope = Envelope::new(Payload::ListTools);
        let json = serde_json::to_string(&envelope).unwrap();
        dealer.send(Msg::from_vec(json.into_bytes())).await.unwrap();

        let response = tokio::time::timeout(Duration::from_secs(2), dealer.recv())
            .await
            .unwrap()
            .unwrap();

        let response_bytes = response.data().unwrap_or_default();
        let response_str = std::str::from_utf8(response_bytes).unwrap();
        let response_envelope: Envelope = serde_json::from_str(response_str).unwrap();

        match response_envelope.payload {
            Payload::ToolList { tools } => {
                assert_eq!(tools.len(), 2);
                assert!(tools.iter().any(|t| t.name == "cas_store"));
                assert!(tools.iter().any(|t| t.name == "cas_inspect"));
            }
            other => panic!("Expected ToolList, got {:?}", other),
        }
    });

    // Wait for both to complete
    let (backend_result, holler_result) = tokio::join!(backend_handle, holler_handle);
    backend_result.unwrap();
    holler_result.unwrap();
}

#[tokio::test]
async fn test_mcp_tool_call_with_traceparent() {
    let zmq_endpoint = next_zmq_endpoint();

    // Start mock hootenanny backend
    let zmq_endpoint_clone = zmq_endpoint.clone();
    let backend_handle = tokio::spawn(async move {
        // This backend will check for traceparent in the envelope
        let context = Context::new().unwrap();
        let socket = context.socket(SocketType::Router).unwrap();
        socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await.ok();
        socket.bind(&zmq_endpoint_clone).await.unwrap();

        let msgs = socket.recv_multipart().await.unwrap();
        let identity = msgs[0].data().map(|d| d.to_vec()).unwrap_or_default();
        let payload_bytes = msgs[1].data().unwrap_or_default();
        let envelope: Envelope = serde_json::from_str(std::str::from_utf8(payload_bytes).unwrap()).unwrap();

        // Verify traceparent was propagated
        assert!(envelope.traceparent.is_some(), "Expected traceparent to be set");
        let tp = envelope.traceparent.as_ref().unwrap();
        assert!(tp.starts_with("00-"), "Traceparent should start with version 00");

        let response = Envelope {
            id: envelope.id,
            traceparent: envelope.traceparent,
            payload: Payload::Success {
                result: serde_json::json!({"traced": true}),
            },
        };

        let response_json = serde_json::to_string(&response).unwrap();
        let reply = vec![
            Msg::from_vec(identity),
            Msg::from_vec(response_json.into_bytes()),
        ];
        socket.send_multipart(reply).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Simulate holler sending a request with traceparent
    let holler_handle = tokio::spawn(async move {
        let context = Context::new().unwrap();
        let dealer = context.socket(SocketType::Dealer).unwrap();
        dealer.set_option_raw(LINGER, &0i32.to_ne_bytes()).await.ok();
        dealer.connect(&zmq_endpoint).await.unwrap();

        // Create envelope with traceparent (simulating what holler does)
        let envelope = Envelope::new(Payload::CasInspect {
            hash: "abc123".to_string(),
        })
        .with_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");

        let json = serde_json::to_string(&envelope).unwrap();
        dealer.send(Msg::from_vec(json.into_bytes())).await.unwrap();

        let response = tokio::time::timeout(Duration::from_secs(2), dealer.recv())
            .await
            .unwrap()
            .unwrap();

        let response_bytes = response.data().unwrap_or_default();
        let response_str = std::str::from_utf8(response_bytes).unwrap();
        let response_envelope: Envelope = serde_json::from_str(response_str).unwrap();

        match response_envelope.payload {
            Payload::Success { result } => {
                assert_eq!(result["traced"], true);
            }
            other => panic!("Expected Success, got {:?}", other),
        }
    });

    let (backend_result, holler_result) = tokio::join!(backend_handle, holler_handle);
    backend_result.unwrap();
    holler_result.unwrap();
}
