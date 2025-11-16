//! Test SSE stream behavior from test server

mod common;

use common::TestMcpServer;
use futures::StreamExt;

#[tokio::test]
async fn test_sse_stream_sends_session_id() {
    let server = TestMcpServer::start().await.unwrap();
    println!("✓ Server started on: {}", server.url);

    let client = reqwest::Client::new();
    let response = client.get(&server.sse_url()).send().await.unwrap();

    println!("✓ SSE response status: {}", response.status());

    let mut stream = response.bytes_stream();
    let mut chunks_received = 0;
    let mut found_session_id = false;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                let text = String::from_utf8_lossy(&chunk);
                println!("  Chunk {}: {:?}", chunks_received, text);

                if text.contains("sessionId") {
                    println!("  ✓ Found session ID in chunk!");
                    found_session_id = true;
                    break;
                }

                chunks_received += 1;
                if chunks_received >= 5 {
                    println!("  Stopping after 5 chunks");
                    break;
                }
            }
            Err(e) => {
                println!("  ✗ Error reading chunk: {}", e);
                break;
            }
        }
    }

    assert!(found_session_id, "Should have received session ID in SSE stream");
}
