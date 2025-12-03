//! Common test utilities for hrcli tests
//!
//! This module provides helpers for setting up ephemeral MCP servers
//! for integration testing.

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use audio_graph_mcp::{AudioGraphAdapter, Database as AudioGraphDb};
use hootenanny::api::handler::HootHandler;
use hootenanny::api::service::EventDualityServer;
use hootenanny::artifact_store::FileStore;
use hootenanny::cas::Cas;
use hootenanny::job_system::JobStore;
use hootenanny::mcp_tools::local_models::LocalModels;

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
    /// Wait for the MCP service to be ready by checking SSE endpoint
    async fn wait_for_mcp_ready(port: u16) -> Result<()> {
        let start = std::time::Instant::now();
        let max_wait = Duration::from_secs(5);

        loop {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()?;

            // Just check that SSE endpoint responds
            let sse_url = format!("http://127.0.0.1:{}/sse", port);
            if let Ok(response) = client.get(&sse_url).send().await {
                if response.status().is_success() {
                    // Server is ready
                    return Ok(());
                }
            }

            if start.elapsed() > max_wait {
                anyhow::bail!(
                    "MCP service did not become ready within {:?}",
                    max_wait
                );
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
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

        // Initialize CAS
        let cas_dir = state_dir.join("cas");
        std::fs::create_dir_all(&cas_dir)?;
        let cas = Cas::new(&cas_dir)?;

        // Initialize LocalModels (with dummy ports - tests won't call Orpheus/DeepSeek)
        let local_models = Arc::new(LocalModels::new(cas.clone(), 9999));

        // Initialize artifact store
        let artifact_store_path = state_dir.join("artifacts.json");
        let artifact_store = Arc::new(std::sync::RwLock::new(
            FileStore::new(&artifact_store_path)?
        ));

        // Initialize job store
        let job_store = Arc::new(JobStore::new());

        // Initialize audio graph
        let audio_graph_db = Arc::new(AudioGraphDb::in_memory()?);

        // Create artifact source for Trustfall
        let artifact_source = Arc::new(
            hootenanny::artifact_store::FileStoreSource::new(artifact_store.clone())
        );

        let graph_adapter = Arc::new(
            AudioGraphAdapter::new_with_artifacts(
                audio_graph_db.clone(),
                audio_graph_mcp::PipeWireSnapshot::default(),
                artifact_source,
            )?
        );

        // Create the EventDualityServer
        let event_duality_server = Arc::new(EventDualityServer::new(
            local_models,
            artifact_store,
            job_store,
            audio_graph_db,
            graph_adapter,
        ));

        // Create baton MCP handler and state
        let hoot_handler = HootHandler::new(event_duality_server);
        let mcp_state = Arc::new(baton::McpState::new(
            hoot_handler,
            "hootenanny-test",
            "0.1.0-test",
        ));

        let shutdown_token = CancellationToken::new();

        // Create the MCP router (supports both Streamable HTTP and SSE)
        // Don't nest - hrcli expects /sse at root
        let app_router = baton::dual_router(mcp_state.clone());

        // Bind to get actual port
        let bind_addr = format!("127.0.0.1:{}", port);
        let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
        let actual_port = listener.local_addr()?.port();

        let shutdown_token_clone = shutdown_token.clone();

        // Spawn the server
        tokio::spawn(async move {
            let server = axum::serve(listener, app_router).with_graceful_shutdown(async move {
                shutdown_token_clone.cancelled().await;
            });

            if let Err(e) = server.await {
                eprintln!("Test MCP server error: {:?}", e);
            }
        });

        // Wait for MCP service to be fully functional
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

    /// Get the streamable HTTP endpoint URL (POST /)
    pub fn streamable_url(&self) -> String {
        self.url.clone()
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
    async fn test_server_responds_to_sse() {
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
