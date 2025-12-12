//! Integration tests for ZMQ SUB subscriber

use hooteproto::Broadcast;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tokio::sync::broadcast;
use zeromq::{PubSocket, Socket, SocketSend};

static PUB_PORT: AtomicU16 = AtomicU16::new(26580);

fn next_pub_endpoint() -> String {
    let port = PUB_PORT.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

#[tokio::test]
async fn test_subscriber_receives_broadcast() {
    let endpoint = next_pub_endpoint();

    // Create broadcast channel
    let (tx, mut rx) = broadcast::channel::<Broadcast>(16);

    // Start a mock PUB socket
    let mut pub_socket = PubSocket::new();
    pub_socket.bind(&endpoint).await.unwrap();

    // Give socket time to bind
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Start subscriber in background
    let endpoint_clone = endpoint.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        use zeromq::{Socket, SocketRecv, SubSocket};

        let mut sub_socket = SubSocket::new();
        sub_socket.subscribe("").await.unwrap();
        sub_socket.connect(&endpoint_clone).await.unwrap();

        // Receive one message and forward it
        if let Ok(msg) = sub_socket.recv().await {
            if let Some(bytes) = msg.get(0) {
                if let Ok(json) = std::str::from_utf8(bytes) {
                    if let Ok(broadcast) = serde_json::from_str::<Broadcast>(json) {
                        let _ = tx_clone.send(broadcast);
                    }
                }
            }
        }
    });

    // Give subscriber time to connect
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish a broadcast
    let broadcast = Broadcast::JobStateChanged {
        job_id: "test-job-123".to_string(),
        state: "completed".to_string(),
        result: Some(serde_json::json!({"output": "success"})),
    };

    let json = serde_json::to_string(&broadcast).unwrap();
    let msg = zeromq::ZmqMessage::from(json.into_bytes());
    pub_socket.send(msg).await.unwrap();

    // Receive from broadcast channel
    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Timeout waiting for broadcast")
        .expect("Channel closed");

    match received {
        Broadcast::JobStateChanged { job_id, state, .. } => {
            assert_eq!(job_id, "test-job-123");
            assert_eq!(state, "completed");
        }
        other => panic!("Expected JobStateChanged, got {:?}", other),
    }
}

#[tokio::test]
async fn test_subscriber_handles_multiple_broadcasts() {
    let endpoint = next_pub_endpoint();

    let (tx, mut rx) = broadcast::channel::<Broadcast>(16);

    let mut pub_socket = PubSocket::new();
    pub_socket.bind(&endpoint).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Start subscriber
    let endpoint_clone = endpoint.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        use zeromq::{Socket, SocketRecv, SubSocket};

        let mut sub_socket = SubSocket::new();
        sub_socket.subscribe("").await.unwrap();
        sub_socket.connect(&endpoint_clone).await.unwrap();

        // Receive multiple messages
        for _ in 0..3 {
            if let Ok(msg) = sub_socket.recv().await {
                if let Some(bytes) = msg.get(0) {
                    if let Ok(json) = std::str::from_utf8(bytes) {
                        if let Ok(broadcast) = serde_json::from_str::<Broadcast>(json) {
                            let _ = tx_clone.send(broadcast);
                        }
                    }
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish 3 broadcasts
    for i in 0..3 {
        let broadcast = Broadcast::BeatTick {
            beat: i,
            position_beats: i as f64,
            tempo_bpm: 120.0,
        };
        let json = serde_json::to_string(&broadcast).unwrap();
        pub_socket.send(zeromq::ZmqMessage::from(json.into_bytes())).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Receive all 3
    for i in 0..3 {
        let received = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("Timeout")
            .expect("Channel closed");

        match received {
            Broadcast::BeatTick { beat, .. } => {
                assert_eq!(beat, i);
            }
            other => panic!("Expected BeatTick, got {:?}", other),
        }
    }
}

#[tokio::test]
async fn test_subscriber_parses_artifact_created() {
    let endpoint = next_pub_endpoint();

    let (tx, mut rx) = broadcast::channel::<Broadcast>(16);

    let mut pub_socket = PubSocket::new();
    pub_socket.bind(&endpoint).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let endpoint_clone = endpoint.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        use zeromq::{Socket, SocketRecv, SubSocket};

        let mut sub_socket = SubSocket::new();
        sub_socket.subscribe("").await.unwrap();
        sub_socket.connect(&endpoint_clone).await.unwrap();

        if let Ok(msg) = sub_socket.recv().await {
            if let Some(bytes) = msg.get(0) {
                if let Ok(json) = std::str::from_utf8(bytes) {
                    if let Ok(broadcast) = serde_json::from_str::<Broadcast>(json) {
                        let _ = tx_clone.send(broadcast);
                    }
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let broadcast = Broadcast::ArtifactCreated {
        artifact_id: "art_abc123".to_string(),
        content_hash: "sha256_def456".to_string(),
        tags: vec!["type:midi".to_string(), "vibe:jazzy".to_string()],
        creator: Some("claude".to_string()),
    };

    let json = serde_json::to_string(&broadcast).unwrap();
    pub_socket.send(zeromq::ZmqMessage::from(json.into_bytes())).await.unwrap();

    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match received {
        Broadcast::ArtifactCreated { artifact_id, tags, creator, .. } => {
            assert_eq!(artifact_id, "art_abc123");
            assert_eq!(tags.len(), 2);
            assert_eq!(creator, Some("claude".to_string()));
        }
        other => panic!("Expected ArtifactCreated, got {:?}", other),
    }
}
