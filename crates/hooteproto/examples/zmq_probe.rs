//! ZMQ Probe - Simple CLI tool to test ZMQ connectivity
//!
//! Usage: cargo run --example zmq_probe -- [OPTIONS]
//!
//! Options:
//!   -e, --endpoint <ENDPOINT>  ZMQ endpoint to connect to [default: tcp://127.0.0.1:5580]
//!   -i, --identity <IDENTITY>  Socket identity [default: zmq-probe]
//!   -t, --timeout <MS>         Timeout in milliseconds [default: 5000]
//!   -c, --count <N>            Number of heartbeats to send [default: 1]

use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hooteproto::socket_config::{create_dealer_and_connect, Multipart, ZmqContext};
use hooteproto::{Command, ContentType, HootFrame};
use std::time::{Duration, Instant};
use uuid::Uuid;

fn frames_to_multipart(frames: &[Bytes]) -> Multipart {
    frames
        .iter()
        .map(|f| f.to_vec())
        .collect::<Vec<_>>()
        .into()
}

fn multipart_to_frames(mp: Multipart) -> Vec<Bytes> {
    mp.into_iter().map(|m| Bytes::from(m.to_vec())).collect()
}

fn print_frame_details(prefix: &str, frames: &[Bytes]) {
    println!("{} {} frames:", prefix, frames.len());
    for (i, frame) in frames.iter().enumerate() {
        let preview = if frame.len() <= 32 {
            format!("{:?}", frame.as_ref())
        } else {
            format!("{:?}... ({} bytes)", &frame[..32], frame.len())
        };
        println!("  [{}] {} bytes: {}", i, frame.len(), preview);
    }
}

fn parse_args() -> (String, String, u64, usize) {
    let args: Vec<String> = std::env::args().collect();
    let mut endpoint = "tcp://127.0.0.1:5580".to_string();
    let mut identity = "zmq-probe".to_string();
    let mut timeout_ms = 5000u64;
    let mut count = 1usize;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-e" | "--endpoint" => {
                i += 1;
                if i < args.len() {
                    endpoint = args[i].clone();
                }
            }
            "-i" | "--identity" => {
                i += 1;
                if i < args.len() {
                    identity = args[i].clone();
                }
            }
            "-t" | "--timeout" => {
                i += 1;
                if i < args.len() {
                    timeout_ms = args[i].parse().unwrap_or(5000);
                }
            }
            "-c" | "--count" => {
                i += 1;
                if i < args.len() {
                    count = args[i].parse().unwrap_or(1);
                }
            }
            "-h" | "--help" => {
                println!("ZMQ Probe - Test ZMQ connectivity to Hootenanny");
                println!();
                println!("Usage: cargo run --example zmq_probe -- [OPTIONS]");
                println!();
                println!("Options:");
                println!("  -e, --endpoint <ENDPOINT>  ZMQ endpoint [default: tcp://127.0.0.1:5580]");
                println!("  -i, --identity <IDENTITY>  Socket identity [default: zmq-probe]");
                println!("  -t, --timeout <MS>         Timeout in ms [default: 5000]");
                println!("  -c, --count <N>            Number of heartbeats [default: 1]");
                println!("  -h, --help                 Show this help");
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {}", other);
            }
        }
        i += 1;
    }

    (endpoint, identity, timeout_ms, count)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (endpoint, identity, timeout_ms, count) = parse_args();

    println!("🔌 ZMQ Probe");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Endpoint: {}", endpoint);
    println!("  Identity: {}", identity);
    println!("  Timeout:  {}ms", timeout_ms);
    println!("  Count:    {}", count);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();

    let ctx = ZmqContext::new();
    println!("📡 Creating DEALER socket with identity: {:?}", identity.as_bytes());

    let connect_start = Instant::now();
    let socket = create_dealer_and_connect(&ctx, &endpoint, identity.as_bytes(), &identity)?;
    println!("✅ Socket created in {:?} (note: connect is async)", connect_start.elapsed());

    let (mut tx, mut rx) = socket.split();

    for i in 0..count {
        let request_id = Uuid::new_v4();
        println!();
        println!("━━━ Heartbeat {}/{} ━━━", i + 1, count);
        println!("  Request ID: {}", request_id);

        let frame = HootFrame {
            command: Command::Heartbeat,
            content_type: ContentType::Empty,
            request_id,
            service: identity.clone(),
            traceparent: None,
            body: Bytes::new(),
        };

        let send_frames = frame.to_frames();
        print_frame_details("📤 Sending", &send_frames);

        let mp = frames_to_multipart(&send_frames);
        let send_start = Instant::now();
        tx.send(mp).await?;
        println!("✅ Sent in {:?}", send_start.elapsed());

        println!();
        println!("⏳ Waiting for response (timeout: {}ms)...", timeout_ms);

        let recv_start = Instant::now();
        match tokio::time::timeout(Duration::from_millis(timeout_ms), rx.next()).await {
            Ok(Some(Ok(response))) => {
                let elapsed = recv_start.elapsed();
                println!("✅ Response received in {:?}", elapsed);

                let frames = multipart_to_frames(response);
                print_frame_details("📥 Received", &frames);

                match HootFrame::from_frames(&frames) {
                    Ok(parsed) => {
                        println!();
                        println!("📋 Parsed frame:");
                        println!("   Command:      {:?}", parsed.command);
                        println!("   ContentType:  {:?}", parsed.content_type);
                        println!("   Request ID:   {}", parsed.request_id);
                        println!("   Service:      {:?}", parsed.service);
                        println!("   Traceparent:  {:?}", parsed.traceparent);
                        println!("   Body length:  {} bytes", parsed.body.len());

                        if parsed.request_id != request_id {
                            println!("⚠️  Request ID mismatch! Expected {}, got {}", request_id, parsed.request_id);
                        }
                    }
                    Err(e) => {
                        println!("❌ Failed to parse frame: {}", e);
                    }
                }
            }
            Ok(Some(Err(e))) => {
                println!("❌ Receive error: {}", e);
            }
            Ok(None) => {
                println!("❌ Stream ended unexpectedly");
            }
            Err(_) => {
                println!("❌ Timeout after {:?}", recv_start.elapsed());
            }
        }
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🏁 Done");

    Ok(())
}
