//! Integration tests for the IPC layer
//!
//! Tests client/server communication using inproc:// transport for speed.

use std::sync::Arc;
use std::time::Duration;

use chaosgarden::ipc::{
    client::GardenClient,
    server::{GardenServer, Handler},
    Beat, ControlReply, ControlRequest, GardenEndpoints, QueryReply, QueryRequest, ShellReply,
    ShellRequest,
};

/// Test handler that echoes back predictable responses
struct TestHandler;

impl Handler for TestHandler {
    fn handle_shell(&self, req: ShellRequest) -> ShellReply {
        match req {
            ShellRequest::GetTransportState => ShellReply::TransportState {
                playing: false,
                position: Beat(0.0),
                tempo: 120.0,
            },
            ShellRequest::Play => ShellReply::Ok {
                result: serde_json::json!({"status": "playing"}),
            },
            ShellRequest::Pause => ShellReply::Ok {
                result: serde_json::json!({"status": "paused"}),
            },
            ShellRequest::Stop => ShellReply::Ok {
                result: serde_json::json!({"status": "stopped"}),
            },
            ShellRequest::SetTempo { bpm } => ShellReply::Ok {
                result: serde_json::json!({"tempo": bpm}),
            },
            ShellRequest::Seek { beat } => ShellReply::Ok {
                result: serde_json::json!({"position": beat.0}),
            },
            _ => ShellReply::Error {
                error: "not implemented".to_string(),
                traceback: None,
            },
        }
    }

    fn handle_control(&self, req: ControlRequest) -> ControlReply {
        match req {
            ControlRequest::Shutdown => ControlReply::ShuttingDown,
            ControlRequest::Interrupt => ControlReply::Interrupted {
                was_running: "test".to_string(),
            },
            ControlRequest::EmergencyPause => ControlReply::Ok,
            ControlRequest::DebugDump => ControlReply::DebugDump {
                state: serde_json::json!({"test": true}),
            },
        }
    }

    fn handle_query(&self, req: QueryRequest) -> QueryReply {
        if req.query.contains("test") {
            QueryReply::Results {
                rows: vec![serde_json::json!({"test": "result"})],
            }
        } else {
            QueryReply::Error {
                error: "unknown query".to_string(),
            }
        }
    }
}

fn unique_endpoints() -> GardenEndpoints {
    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    GardenEndpoints {
        control: format!("ipc:///tmp/cg-test-{}-control", id),
        shell: format!("ipc:///tmp/cg-test-{}-shell", id),
        iopub: format!("ipc:///tmp/cg-test-{}-iopub", id),
        heartbeat: format!("ipc:///tmp/cg-test-{}-hb", id),
        query: format!("ipc:///tmp/cg-test-{}-query", id),
    }
}

#[tokio::test]
async fn test_shell_request_reply() {
    let endpoints = unique_endpoints();
    let server = GardenServer::bind(&endpoints).await.unwrap();
    let handler = Arc::new(TestHandler);

    let server_handle = tokio::spawn(async move {
        server.run(handler).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = GardenClient::connect(&endpoints).await.unwrap();

    let reply = client
        .request(ShellRequest::GetTransportState)
        .await
        .unwrap();
    match reply {
        ShellReply::TransportState {
            playing,
            position,
            tempo,
        } => {
            assert!(!playing);
            assert_eq!(position.0, 0.0);
            assert_eq!(tempo, 120.0);
        }
        _ => panic!("unexpected reply: {:?}", reply),
    }

    let reply = client.request(ShellRequest::Play).await.unwrap();
    match reply {
        ShellReply::Ok { result } => {
            assert_eq!(result["status"], "playing");
        }
        _ => panic!("unexpected reply: {:?}", reply),
    }

    let reply = client
        .request(ShellRequest::SetTempo { bpm: 140.0 })
        .await
        .unwrap();
    match reply {
        ShellReply::Ok { result } => {
            assert_eq!(result["tempo"], 140.0);
        }
        _ => panic!("unexpected reply: {:?}", reply),
    }

    let reply = client.control(ControlRequest::Shutdown).await.unwrap();
    assert!(matches!(reply, ControlReply::ShuttingDown));

    let _ = tokio::time::timeout(Duration::from_secs(1), server_handle).await;
}

#[tokio::test]
async fn test_control_request_reply() {
    let endpoints = unique_endpoints();
    let server = GardenServer::bind(&endpoints).await.unwrap();
    let handler = Arc::new(TestHandler);

    let server_handle = tokio::spawn(async move {
        server.run(handler).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = GardenClient::connect(&endpoints).await.unwrap();

    let reply = client.control(ControlRequest::Interrupt).await.unwrap();
    match reply {
        ControlReply::Interrupted { was_running } => {
            assert_eq!(was_running, "test");
        }
        _ => panic!("unexpected reply: {:?}", reply),
    }

    let reply = client.control(ControlRequest::DebugDump).await.unwrap();
    match reply {
        ControlReply::DebugDump { state } => {
            assert_eq!(state["test"], true);
        }
        _ => panic!("unexpected reply: {:?}", reply),
    }

    let reply = client.control(ControlRequest::Shutdown).await.unwrap();
    assert!(matches!(reply, ControlReply::ShuttingDown));

    let _ = tokio::time::timeout(Duration::from_secs(1), server_handle).await;
}

#[tokio::test]
async fn test_query_request_reply() {
    let endpoints = unique_endpoints();
    let server = GardenServer::bind(&endpoints).await.unwrap();
    let handler = Arc::new(TestHandler);

    let server_handle = tokio::spawn(async move {
        server.run(handler).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = GardenClient::connect(&endpoints).await.unwrap();

    let reply = client
        .query("{ test }", std::collections::HashMap::new())
        .await
        .unwrap();

    match reply {
        QueryReply::Results { rows } => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["test"], "result");
        }
        _ => panic!("unexpected reply: {:?}", reply),
    }

    let reply = client
        .query("{ unknown }", std::collections::HashMap::new())
        .await
        .unwrap();

    match reply {
        QueryReply::Error { error } => {
            assert_eq!(error, "unknown query");
        }
        _ => panic!("unexpected reply: {:?}", reply),
    }

    let mut client2 = GardenClient::connect(&endpoints).await.unwrap();
    let _ = client2.control(ControlRequest::Shutdown).await;

    let _ = tokio::time::timeout(Duration::from_secs(1), server_handle).await;
}

#[tokio::test]
async fn test_heartbeat() {
    let endpoints = unique_endpoints();
    let server = GardenServer::bind(&endpoints).await.unwrap();
    let handler = Arc::new(TestHandler);

    let server_handle = tokio::spawn(async move {
        server.run(handler).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = GardenClient::connect(&endpoints).await.unwrap();

    let alive = client.ping(Duration::from_secs(1)).await.unwrap();
    assert!(alive);

    let mut client2 = GardenClient::connect(&endpoints).await.unwrap();
    let _ = client2.control(ControlRequest::Shutdown).await;

    let _ = tokio::time::timeout(Duration::from_secs(1), server_handle).await;
}

#[tokio::test]
async fn test_multiple_sequential_requests() {
    let endpoints = unique_endpoints();
    let server = GardenServer::bind(&endpoints).await.unwrap();
    let handler = Arc::new(TestHandler);

    let server_handle = tokio::spawn(async move {
        server.run(handler).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = GardenClient::connect(&endpoints).await.unwrap();

    for i in 0..10 {
        let bpm = 100.0 + (i as f64);
        let reply = client
            .request(ShellRequest::SetTempo { bpm })
            .await
            .unwrap();
        match reply {
            ShellReply::Ok { result } => {
                assert_eq!(result["tempo"], bpm);
            }
            _ => panic!("unexpected reply: {:?}", reply),
        }
    }

    let _ = client.control(ControlRequest::Shutdown).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), server_handle).await;
}
