//! Test ROUTER/DEALER roundtrip with rzmq to verify framing

use bytes::Bytes;
use hooteproto::{Command, ContentType, HootFrame};
use rzmq::{Context, Msg, MsgFlags, SocketType};
use rzmq::socket::options::LINGER;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;
use uuid::Uuid;

static PORT: AtomicU16 = AtomicU16::new(17570);

fn next_endpoint() -> String {
    let port = PORT.fetch_add(1, Ordering::SeqCst);
    format!("tcp://127.0.0.1:{}", port)
}

fn frames_to_msgs(frames: &[Bytes]) -> Vec<Msg> {
    frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect()
}

fn msgs_to_frames(msgs: Vec<Msg>) -> Vec<Bytes> {
    msgs.into_iter()
        .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
        .collect()
}

#[tokio::test]
async fn test_dealer_router_roundtrip() {
    let endpoint = next_endpoint();

    // Create ROUTER (server)
    let router_ctx = Context::new().unwrap();
    let router = router_ctx.socket(SocketType::Router).unwrap();
    router.set_option_raw(LINGER, &0i32.to_ne_bytes()).await.ok();
    router.bind(&endpoint).await.unwrap();

    // Create DEALER (client)
    let dealer_ctx = Context::new().unwrap();
    let dealer = dealer_ctx.socket(SocketType::Dealer).unwrap();
    dealer.set_option_raw(LINGER, &0i32.to_ne_bytes()).await.ok();
    dealer.connect(&endpoint).await.unwrap();

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
        println!("  Frame {}: {} bytes, starts with {:?}", i, f.len(),
            if f.len() > 10 { &f[..10] } else { &f[..] });
    }

    let request_msgs = frames_to_msgs(&request_frames);
    dealer.send_multipart(request_msgs).await.unwrap();

    // ROUTER receives - should get [identity][payload...]
    let router_recv = tokio::time::timeout(
        Duration::from_secs(2),
        router.recv_multipart()
    ).await.unwrap().unwrap();

    let router_frames = msgs_to_frames(router_recv);
    println!("\nROUTER received {} frames:", router_frames.len());
    for (i, f) in router_frames.iter().enumerate() {
        println!("  Frame {}: {} bytes, starts with {:?}", i, f.len(),
            if f.len() > 10 { &f[..10] } else { &f[..] });
    }

    // Parse - should find HOOT01 after identity
    let (identity, parsed_request) = HootFrame::from_frames_with_identity(&router_frames)
        .expect("Failed to parse request from DEALER");

    println!("\nParsed request: command={:?}, request_id={}",
        parsed_request.command, parsed_request.request_id);
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
    println!("\nROUTER sending {} frames using individual send():", reply_frames.len());
    for (i, f) in reply_frames.iter().enumerate() {
        println!("  Frame {}: {} bytes, starts with {:?}", i, f.len(),
            if f.len() > 10 { &f[..10] } else { &f[..] });
    }

    // Use individual send() with MORE flags (rzmq send_multipart has a bug)
    let last_idx = reply_frames.len() - 1;
    for (i, frame) in reply_frames.iter().enumerate() {
        let mut msg = Msg::from_vec(frame.to_vec());
        if i < last_idx {
            msg.set_flags(MsgFlags::MORE);
        }
        router.send(msg).await.unwrap();
    }

    // DEALER receives reply
    let dealer_recv = tokio::time::timeout(
        Duration::from_secs(2),
        dealer.recv_multipart()
    ).await.unwrap().unwrap();

    let dealer_frames = msgs_to_frames(dealer_recv);
    println!("\nDEALER received {} frames:", dealer_frames.len());
    for (i, f) in dealer_frames.iter().enumerate() {
        println!("  Frame {}: {} bytes, starts with {:?}", i, f.len(),
            if f.len() > 10 { &f[..10] } else { &f[..] });
    }

    // Parse reply - DEALER should receive just the payload (identity stripped by rzmq)
    let parsed_reply = HootFrame::from_frames(&dealer_frames)
        .expect("Failed to parse reply at DEALER");

    println!("\nParsed reply: command={:?}, request_id={}",
        parsed_reply.command, parsed_reply.request_id);

    assert_eq!(parsed_reply.command, Command::Reply);
    assert_eq!(parsed_reply.request_id, request_id);
}

#[tokio::test]
async fn test_raw_frames_roundtrip() {
    // Test what rzmq actually does with raw frames (no HOOT01 protocol)
    let endpoint = next_endpoint();

    let router_ctx = Context::new().unwrap();
    let router = router_ctx.socket(SocketType::Router).unwrap();
    router.set_option_raw(LINGER, &0i32.to_ne_bytes()).await.ok();
    router.bind(&endpoint).await.unwrap();

    let dealer_ctx = Context::new().unwrap();
    let dealer = dealer_ctx.socket(SocketType::Dealer).unwrap();
    dealer.set_option_raw(LINGER, &0i32.to_ne_bytes()).await.ok();
    dealer.connect(&endpoint).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // DEALER sends simple message
    let msg = vec![
        Msg::from_vec(b"hello".to_vec()),
        Msg::from_vec(b"world".to_vec()),
    ];
    dealer.send_multipart(msg).await.unwrap();

    // ROUTER receives
    let router_recv = tokio::time::timeout(
        Duration::from_secs(2),
        router.recv_multipart()
    ).await.unwrap().unwrap();

    println!("Raw test - ROUTER received {} frames:", router_recv.len());
    for (i, m) in router_recv.iter().enumerate() {
        let data = m.data().unwrap_or_default();
        println!("  Frame {}: {} bytes = {:?}", i, data.len(),
            String::from_utf8_lossy(data));
    }

    // First frame should be identity, then our data
    assert!(router_recv.len() >= 2, "Expected at least identity + data");

    // Identity should be non-empty
    let identity = router_recv[0].data().unwrap_or_default();
    assert!(!identity.is_empty(), "Identity should not be empty");

    // Get remaining frames
    let payload_start = 1; // After identity

    // ROUTER echoes back using individual send() with MORE flags
    let mut id_msg = Msg::from_vec(identity.to_vec());
    id_msg.set_flags(MsgFlags::MORE);
    router.send(id_msg).await.unwrap();

    // Send payload frames
    let last_idx = router_recv.len() - 1;
    for i in payload_start..router_recv.len() {
        let data = router_recv[i].data().unwrap_or_default().to_vec();
        let mut msg = Msg::from_vec(data);
        if i < last_idx {
            msg.set_flags(MsgFlags::MORE);
        }
        router.send(msg).await.unwrap();
    }

    println!("Raw test - ROUTER sent identity + {} payload frames individually", router_recv.len() - payload_start);

    // DEALER receives reply
    let dealer_recv = tokio::time::timeout(
        Duration::from_secs(2),
        dealer.recv_multipart()
    ).await.unwrap().unwrap();

    println!("Raw test - DEALER received {} frames:", dealer_recv.len());
    for (i, m) in dealer_recv.iter().enumerate() {
        let data = m.data().unwrap_or_default();
        println!("  Frame {}: {} bytes = {:?}", i, data.len(),
            String::from_utf8_lossy(data));
    }

    // Should receive both hello and world
    assert_eq!(dealer_recv.len(), 2, "DEALER should receive 2 payload frames");
}
