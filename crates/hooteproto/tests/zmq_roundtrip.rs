//! Test ROUTER/DEALER roundtrip with tmq to verify framing

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hooteproto::socket_config::{Multipart, ZmqContext};
use hooteproto::{Command, ContentType, HootFrame};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use tmq::{dealer, router};
use uuid::Uuid;

static PORT: AtomicU16 = AtomicU16::new(17570);

fn next_endpoint() -> String {
    let port = PORT.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

fn frames_to_multipart(frames: &[Bytes]) -> Multipart {
    frames
        .iter()
        .map(|f| f.to_vec())
        .collect::<Vec<_>>()
        .into()
}

fn multipart_to_frames(mp: Multipart) -> Vec<Bytes> {
    mp.into_iter()
        .map(|m| Bytes::from(m.to_vec()))
        .collect()
}

#[tokio::test]
async fn test_dealer_router_roundtrip() {
    let endpoint = next_endpoint();

    // Create ROUTER (server)
    let router_ctx = ZmqContext::new();
    let router_socket = router(&router_ctx)
        .set_linger(0)
        .bind(&endpoint)
        .unwrap();

    let (mut router_tx, mut router_rx) = router_socket.split();

    // Create DEALER (client)
    let dealer_ctx = ZmqContext::new();
    let dealer_socket = dealer(&dealer_ctx)
        .set_linger(0)
        .connect(&endpoint)
        .unwrap();

    let (mut dealer_tx, mut dealer_rx) = dealer_socket.split();

    // Give sockets time to connect
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create a HootFrame request
    let request_id = Uuid::new_v4();
    let request_frame = HootFrame {
        command: Command::Request,
        content_type: ContentType::Empty,
        request_id,
        service: "test".to_string(),
        traceparent: None,
        body: Bytes::new(),
    };

    // DEALER sends request (no identity prefix needed)
    let request_frames = request_frame.to_frames();
    println!("DEALER sending {} frames:", request_frames.len());
    for (i, f) in request_frames.iter().enumerate() {
        println!(
            "  Frame {}: {} bytes, starts with {:?}",
            i,
            f.len(),
            if f.len() > 10 { &f[..10] } else { &f[..] }
        );
    }

    let mp = frames_to_multipart(&request_frames);
    dealer_tx.send(mp).await.unwrap();

    // ROUTER receives - should get [identity][payload...]
    let router_recv = tokio::time::timeout(Duration::from_secs(2), router_rx.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let router_frames = multipart_to_frames(router_recv);
    println!("\nROUTER received {} frames:", router_frames.len());
    for (i, f) in router_frames.iter().enumerate() {
        println!(
            "  Frame {}: {} bytes, starts with {:?}",
            i,
            f.len(),
            if f.len() > 10 { &f[..10] } else { &f[..] }
        );
    }

    // Parse - should find HOOT01 after identity
    let (identity, parsed_request) = HootFrame::from_frames_with_identity(&router_frames)
        .expect("Failed to parse request from DEALER");

    println!(
        "\nParsed request: command={:?}, request_id={}",
        parsed_request.command, parsed_request.request_id
    );
    println!("Identity frames: {}", identity.len());

    assert_eq!(parsed_request.command, Command::Request);
    assert_eq!(parsed_request.request_id, request_id);

    // ROUTER sends reply
    let reply_frame = HootFrame {
        command: Command::Reply,
        content_type: ContentType::Empty,
        request_id: parsed_request.request_id,
        service: "test".to_string(),
        traceparent: None,
        body: Bytes::new(),
    };

    let reply_frames = reply_frame.to_frames_with_identity(&identity);
    println!(
        "\nROUTER sending {} frames via multipart:",
        reply_frames.len()
    );
    for (i, f) in reply_frames.iter().enumerate() {
        println!(
            "  Frame {}: {} bytes, starts with {:?}",
            i,
            f.len(),
            if f.len() > 10 { &f[..10] } else { &f[..] }
        );
    }

    // tmq handles multipart correctly - no need for manual MORE flags
    let reply_mp = frames_to_multipart(&reply_frames);
    router_tx.send(reply_mp).await.unwrap();

    // DEALER receives reply
    let dealer_recv = tokio::time::timeout(Duration::from_secs(2), dealer_rx.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let dealer_frames = multipart_to_frames(dealer_recv);
    println!("\nDEALER received {} frames:", dealer_frames.len());
    for (i, f) in dealer_frames.iter().enumerate() {
        println!(
            "  Frame {}: {} bytes, starts with {:?}",
            i,
            f.len(),
            if f.len() > 10 { &f[..10] } else { &f[..] }
        );
    }

    // Parse reply - DEALER receives payload (identity stripped by libzmq)
    let parsed_reply =
        HootFrame::from_frames(&dealer_frames).expect("Failed to parse reply at DEALER");

    println!(
        "\nParsed reply: command={:?}, request_id={}",
        parsed_reply.command, parsed_reply.request_id
    );

    assert_eq!(parsed_reply.command, Command::Reply);
    assert_eq!(parsed_reply.request_id, request_id);
}

#[tokio::test]
async fn test_raw_frames_roundtrip() {
    // Test basic multipart handling with tmq
    let endpoint = next_endpoint();

    let router_ctx = ZmqContext::new();
    let router_socket = router(&router_ctx)
        .set_linger(0)
        .bind(&endpoint)
        .unwrap();

    let (mut router_tx, mut router_rx) = router_socket.split();

    let dealer_ctx = ZmqContext::new();
    let dealer_socket = dealer(&dealer_ctx)
        .set_linger(0)
        .connect(&endpoint)
        .unwrap();

    let (mut dealer_tx, mut dealer_rx) = dealer_socket.split();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // DEALER sends simple multipart message
    let mp: Multipart = vec![b"hello".to_vec(), b"world".to_vec()].into();
    dealer_tx.send(mp).await.unwrap();

    // ROUTER receives
    let router_recv = tokio::time::timeout(Duration::from_secs(2), router_rx.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let frames: Vec<Vec<u8>> = router_recv.into_iter().map(|m| m.to_vec()).collect();
    println!("Raw test - ROUTER received {} frames:", frames.len());
    for (i, f) in frames.iter().enumerate() {
        println!(
            "  Frame {}: {} bytes = {:?}",
            i,
            f.len(),
            String::from_utf8_lossy(f)
        );
    }

    // First frame should be identity, then our data
    assert!(frames.len() >= 2, "Expected at least identity + data");

    // Identity should be non-empty
    let identity = &frames[0];
    assert!(!identity.is_empty(), "Identity should not be empty");

    // ROUTER echoes back - tmq handles multipart correctly
    let reply: Multipart = frames.into();
    router_tx.send(reply).await.unwrap();

    // DEALER receives reply
    let dealer_recv = tokio::time::timeout(Duration::from_secs(2), dealer_rx.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let dealer_frames: Vec<Vec<u8>> = dealer_recv.into_iter().map(|m| m.to_vec()).collect();
    println!("Raw test - DEALER received {} frames:", dealer_frames.len());
    for (i, f) in dealer_frames.iter().enumerate() {
        println!(
            "  Frame {}: {} bytes = {:?}",
            i,
            f.len(),
            String::from_utf8_lossy(f)
        );
    }

    // Should receive both hello and world (identity stripped by libzmq)
    assert_eq!(
        dealer_frames.len(),
        2,
        "DEALER should receive 2 payload frames"
    );
    assert_eq!(dealer_frames[0], b"hello");
    assert_eq!(dealer_frames[1], b"world");
}
