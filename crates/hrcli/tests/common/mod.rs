//! Common test utilities for hrcli tests
//!
//! This module provides helpers for setting up ephemeral MCP servers
//! for integration testing.

use anyhow::{anyhow, Context, Result};
use hootenanny::persistence::conversation_store::ConversationStore;
use hootenanny::server::{ConversationState, EventDualityServer};
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// A test MCP server that runs on an ephemeral port
pub struct TestMcpServer {
    /// The actual port the server is listening on
    pub port: u16,
    /// The full URL to connect to (http://127.0.0.1:PORT)
    pub url: String,
    /// Temporary directory for server state (auto-cleaned on drop)
    _temp_dir: TempDir,
    /// Cancellation token to shut down the server
    shutdown_token: CancellationToken,
}

impl TestMcpServer {
    /// Wait for the MCP service to be ready by performing an actual MCP handshake
    async fn wait_for_mcp_ready(port: u16) -> Result<()> {
        use futures::StreamExt;
        use serde_json::json;

        let start = std::time::Instant::now();
        let max_wait = Duration::from_secs(10);

        loop {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()?;

            // Try to connect to SSE endpoint and get session ID
            let sse_url = format!("http://127.0.0.1:{}/sse", port);
            let sse_result = client.get(&sse_url).send().await;

            match sse_result {
                Ok(response) if response.status().is_success() => {
                    // Read the SSE stream to extract session ID
                    let mut stream = response.bytes_stream();
                    if let Some(Ok(chunk)) = stream.next().await {
                        let text = String::from_utf8_lossy(&chunk);

                        // Look for session ID in the endpoint event
                        for line in text.lines() {
                            if line.starts_with("data: /message?sessionId=") {
                                let session_id = &line[25..];

                                // Try to send a test initialize request
                                let message_url = format!(
                                    "http://127.0.0.1:{}/message?sessionId={}",
                                    port, session_id
                                );

                                let init_request = json!({
                                    "jsonrpc": "2.0",
                                    "id": 1,
                                    "method": "initialize",
                                    "params": {
                                        "protocolVersion": "2025-06-18",
                                        "capabilities": {},
                                        "clientInfo": {
                                            "name": "test-client",
                                            "version": "0.1.0"
                                        }
                                    }
                                });

                                let post_result = client
                                    .post(&message_url)
                                    .header("Content-Type", "application/json")
                                    .body(init_request.to_string())
                                    .send()
                                    .await;

                                if let Ok(resp) = post_result {
                                    if resp.status() == reqwest::StatusCode::ACCEPTED {
                                        // Server accepted the message, it's ready!
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if start.elapsed() > max_wait {
                return Err(anyhow!(
                    "MCP service did not become ready within {:?}",
                    max_wait
                ));
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Start a new ephemeral MCP server on a random port
    pub async fn start() -> Result<Self> {
        Self::start_on_port(0).await
    }

    /// Start a new MCP server on a specific port (use 0 for ephemeral)
    pub async fn start_on_port(port: u16) -> Result<Self> {
        // Create temporary directory for state
        let temp_dir = TempDir::new().context("Failed to create temp dir")?;
        let state_dir = temp_dir.path().to_path_buf();

        // Initialize conversation state
        let conversation_dir = state_dir.join("conversation");
        std::fs::create_dir_all(&conversation_dir)
            .context("Failed to create conversation dir")?;
        let conversation_state = ConversationState::new(conversation_dir)
            .context("Failed to initialize conversation state")?;
        let shared_state = Arc::new(Mutex::new(conversation_state));

        // Create SSE server configuration
        let bind_addr: SocketAddr = format!("127.0.0.1:{}", port)
            .parse()
            .context("Failed to parse bind address")?;

        let shutdown_token = CancellationToken::new();
        let sse_config = SseServerConfig {
            bind: bind_addr,
            sse_path: "/sse".to_string(),
            post_path: "/message".to_string(),
            ct: shutdown_token.clone(),
            sse_keep_alive: Some(Duration::from_secs(15)),
        };

        let (sse_server, router) = SseServer::new(sse_config);

        // Get actual port by pre-binding
        let actual_port = if port == 0 {
            // For ephemeral ports, bind temporarily to get the port
            let temp_listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
            let port = temp_listener.local_addr()?.port();
            drop(temp_listener);
            port
        } else {
            port
        };

        let bind_str = format!("127.0.0.1:{}", actual_port);

        // Create channel to signal when MCP service is ready
        use tokio::sync::oneshot;
        let (ready_tx, ready_rx) = oneshot::channel();

        // Run server in a dedicated thread with its own runtime
        // This is critical for subprocess connections to work!
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                // Register the service INSIDE the server thread's runtime
                // This ensures the service handler is registered in the correct runtime context
                let ct = sse_server.with_service(move || {
                    EventDualityServer::new_with_state(shared_state.clone())
                });

                let listener = match tokio::net::TcpListener::bind(&bind_str).await {
                    Ok(l) => l,
                    Err(e) => {
                        eprintln!("Failed to bind test server: {}", e);
                        let _ = ready_tx.send(Err(anyhow!("Failed to bind: {}", e)));
                        return;
                    }
                };

                // Signal that server is bound and service is registered
                let _ = ready_tx.send(Ok(()));

                let server = axum::serve(listener, router).with_graceful_shutdown(async move {
                    ct.cancelled().await;
                });

                if let Err(e) = server.await {
                    eprintln!("Test MCP server error: {:?}", e);
                }
            });
        });

        // Wait for server to signal readiness
        ready_rx
            .await
            .context("Server thread dropped before signaling ready")??;

        // Wait for MCP service to be fully functional with handshake test
        Self::wait_for_mcp_ready(actual_port).await?;

        let url = format!("http://127.0.0.1:{}", actual_port);

        Ok(Self {
            port: actual_port,
            url,
            _temp_dir: temp_dir,
            shutdown_token,
        })
    }

    /// Get the SSE endpoint URL
    pub fn sse_url(&self) -> String {
        format!("{}/sse", self.url)
    }

    /// Get the message endpoint URL
    pub fn message_url(&self) -> String {
        format!("{}/message", self.url)
    }

    /// Shutdown the server gracefully
    pub async fn shutdown(self) {
        self.shutdown_token.cancel();
        // Give server time to shutdown gracefully
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

impl Drop for TestMcpServer {
    fn drop(&mut self) {
        self.shutdown_token.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_starts_on_ephemeral_port() {
        let server = TestMcpServer::start().await.unwrap();
        assert!(server.port > 0, "Port should be assigned");
        assert!(server.port != 8080, "Should not be default port");
        println!("Test server started on port {}", server.port);
    }

    #[tokio::test]
    async fn test_server_responds_to_health_check() {
        let server = TestMcpServer::start().await.unwrap();

        // Try to connect to the SSE endpoint
        let client = reqwest::Client::new();
        let response = client
            .get(&server.sse_url())
            .send()
            .await
            .expect("Failed to connect to server");

        assert!(
            response.status().is_success(),
            "Server should respond to SSE requests"
        );
    }
}
