//! Test if subprocess can read SSE stream

mod common;

use common::TestMcpServer;
use std::process::Command;

#[tokio::test]
#[ignore = "Subprocess + async server requires multi-threaded runtime"]
async fn test_subprocess_can_read_sse() {
    let server = TestMcpServer::start().await.unwrap();
    println!("✓ Server started on: {}", server.url);

    // Try curl as a subprocess
    let output = Command::new("timeout")
        .arg("2")
        .arg("curl")
        .arg("-N")  // No buffering
        .arg(&server.sse_url())
        .output()
        .expect("Failed to run curl");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("curl output:\n{}", stdout);

    assert!(stdout.contains("sessionId"), "Subprocess should be able to read SSE stream with session ID");
}
