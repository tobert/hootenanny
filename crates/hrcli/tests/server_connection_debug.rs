//! Debug test to understand server connection issues

mod common;

use common::TestMcpServer;

#[tokio::test]
async fn debug_server_connection() {
    let server = TestMcpServer::start().await.unwrap();

    println!("✓ Server started on port: {}", server.port);
    println!("✓ Server URL: {}", server.url);
    println!("✓ SSE URL: {}", server.sse_url());

    // Test 1: Can we GET the SSE endpoint?
    let client = reqwest::Client::new();
    let sse_response = client.get(&server.sse_url()).send().await;

    match sse_response {
        Ok(resp) => {
            println!("✓ SSE endpoint responded with status: {}", resp.status());

            // Try to read a bit of the SSE stream
            let mut stream = resp.bytes_stream();
            use futures::StreamExt;

            println!("  Reading first few chunks from SSE stream:");
            let mut count = 0;
            while let Some(chunk) = stream.next().await {
                if count >= 3 { break; }
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        println!("  Chunk {}: {:?}", count, text);
                        count += 1;
                    }
                    Err(e) => {
                        println!("  ✗ Error reading chunk: {}", e);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to connect to SSE endpoint: {}", e);
        }
    }

    // Test 2: Can a subprocess (curl) reach the server?
    println!("\n  Testing if curl subprocess can reach server:");
    let curl_output = std::process::Command::new("curl")
        .arg("-v")
        .arg(&server.sse_url())
        .output();

    match curl_output {
        Ok(output) => {
            println!("  curl exit code: {}", output.status);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("200 OK") || stderr.contains("HTTP/1.1 200") {
                println!("  ✓ curl successfully connected!");
            } else {
                println!("  curl stderr: {}", stderr);
            }
        }
        Err(e) => {
            println!("  ✗ curl not available or failed: {}", e);
        }
    }

    // Test 3: Try running the CLI with explicit environment variable
    println!("\n  Testing CLI with HRCLI_SERVER={}", server.url);

    use assert_cmd::Command;
    let output = Command::cargo_bin("hrcli")
        .unwrap()
        .env("HRCLI_SERVER", &server.url)
        .env("RUST_LOG", "debug")
        .arg("discover")
        .output()
        .expect("Failed to run CLI");

    println!("  CLI exit code: {}", output.status);
    println!("  CLI stdout:");
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("  CLI stderr:");
    println!("{}", String::from_utf8_lossy(&output.stderr));
}
