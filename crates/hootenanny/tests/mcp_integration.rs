//! Integration tests for MCP transport
//!
//! These tests spin up actual servers and make real MCP calls.
//! All databases are ephemeral (in-memory or temp dirs) - no shared state.

mod common;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use audio_graph_mcp::{AudioGraphAdapter, Database as AudioGraphDb};
use hootenanny::api::handler::HootHandler;
use hootenanny::api::service::EventDualityServer;
use hootenanny::artifact_store::FileStore;
use cas::FileStore as CasFileStore;
use hootenanny::job_system::JobStore;
use hootenanny::mcp_tools::local_models::LocalModels;

/// Test server configuration
struct TestServerConfig {
    /// If Some, use file-backed audio graph DB at this path (for persistence tests)
    audio_graph_db_path: Option<PathBuf>,
    /// Shutdown signal sender (if graceful shutdown needed)
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Default for TestServerConfig {
    fn default() -> Self {
        Self {
            audio_graph_db_path: None,
            shutdown_tx: None,
        }
    }
}

/// Spawns a test server with all-ephemeral state (default for most tests)
async fn spawn_test_server() -> Result<(String, tokio::task::JoinHandle<()>)> {
    let (url, handle, _) = spawn_test_server_configured(TestServerConfig::default()).await?;
    Ok((url, handle))
}

/// Spawns a test server with configurable state persistence
async fn spawn_test_server_configured(
    config: TestServerConfig,
) -> Result<(String, tokio::task::JoinHandle<()>, Option<tokio::sync::oneshot::Sender<()>>)> {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);

    let base_url = format!("http://127.0.0.1:{}/mcp", port);

    // Create shutdown channel if requested
    let (shutdown_tx, shutdown_rx) = if config.shutdown_tx.is_some() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let audio_graph_db_path = config.audio_graph_db_path;

    let handle = tokio::spawn(async move {
        // All-ephemeral temp dir for CAS, artifacts
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let state_dir = temp_dir.path();

        // CAS - always ephemeral
        let cas_dir = state_dir.join("cas");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas = CasFileStore::at_path(&cas_dir).unwrap();
        let local_models = Arc::new(LocalModels::new(cas.clone(), 2000));

        // Artifacts - always ephemeral
        let artifact_store = Arc::new(std::sync::RwLock::new(
            FileStore::new(state_dir.join("artifacts.json")).unwrap()
        ));

        // Jobs - always in-memory
        let job_store = Arc::new(JobStore::new());

        // Audio graph - configurable (in-memory or file-backed)
        let audio_graph_db = match &audio_graph_db_path {
            Some(path) => Arc::new(AudioGraphDb::open(path).unwrap()),
            None => Arc::new(AudioGraphDb::in_memory().unwrap()),
        };

        // Create artifact source for Trustfall
        let artifact_source = Arc::new(
            hootenanny::artifact_store::FileStoreSource::new(artifact_store.clone())
        );

        let graph_adapter = Arc::new(
            AudioGraphAdapter::new_with_artifacts(
                audio_graph_db.clone(),
                audio_graph_mcp::PipeWireSnapshot::default(),
                artifact_source,
            ).unwrap()
        );

        let gpu_monitor = Arc::new(hootenanny::gpu_monitor::GpuMonitor::new());

        let event_duality_server = Arc::new(EventDualityServer::new(
            local_models,
            artifact_store,
            job_store,
            audio_graph_db,
            graph_adapter,
            gpu_monitor,
        ));

        let hoot_handler = HootHandler::new(event_duality_server);
        let mcp_state = Arc::new(baton::McpState::new(
            hoot_handler,
            "hootenanny-test",
            env!("CARGO_PKG_VERSION"),
        ));
        let mcp_router = baton::router(mcp_state);
        let app = axum::Router::new().nest("/mcp", mcp_router);

        let addr = format!("127.0.0.1:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

        if let Some(shutdown_rx) = shutdown_rx {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
                .await
                .unwrap();
        } else {
            axum::serve(listener, app).await.unwrap();
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok((base_url, handle, shutdown_tx))
}

#[tokio::test]
async fn test_mcp_connect_and_list_tools() -> Result<()> {
    let (base_url, _server_handle) = spawn_test_server().await?;

    // Connect to the server
    let client = timeout(
        Duration::from_secs(5),
        common::mcp_client::McpClient::connect(&base_url)
    )
    .await
    .expect("Timeout connecting to server")?;

    assert!(client.session_id.is_some(), "Should have a session ID");

    // List tools
    let tools = timeout(
        Duration::from_secs(5),
        client.list_tools()
    )
    .await
    .expect("Timeout listing tools")?;

    // We should have some tools available
    assert!(!tools.is_empty(), "Should have at least one tool");

    // Check for some expected tools
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(tool_names.contains(&"cas_store"), "Should have 'cas_store' tool");
    assert!(tool_names.contains(&"graph_bind"), "Should have 'graph_bind' tool");

    Ok(())
}

#[tokio::test]
async fn test_mcp_call_graph_find() -> Result<()> {
    let (base_url, _server_handle) = spawn_test_server().await?;

    let client = timeout(
        Duration::from_secs(5),
        common::mcp_client::McpClient::connect(&base_url)
    )
    .await
    .expect("Timeout connecting to server")?;

    // Call graph_find tool (should work without arguments)
    let result = timeout(
        Duration::from_secs(5),
        client.call_tool("graph_find", serde_json::json!({}))
    )
    .await
    .expect("Timeout calling tool")?;

    // Should return an empty array (no identities yet)
    assert!(result.is_array(), "Should return an array");

    Ok(())
}

#[tokio::test]
async fn test_mcp_session_resumption() -> Result<()> {
    let (base_url, _server_handle) = spawn_test_server().await?;

    // Connect first time
    let client1 = timeout(
        Duration::from_secs(5),
        common::mcp_client::McpClient::connect(&base_url)
    )
    .await
    .expect("Timeout connecting to server")?;

    let session_id = client1.session_id.clone().expect("Should have session ID");

    // Verify we can make a call
    let _ = timeout(
        Duration::from_secs(5),
        client1.call_tool("graph_find", serde_json::json!({}))
    )
    .await
    .expect("Timeout calling tool")?;

    // Drop the first client (simulating disconnect)
    drop(client1);

    // Small delay
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Reconnect with the same session ID
    let client2 = timeout(
        Duration::from_secs(5),
        common::mcp_client::McpClient::connect_with_session(&base_url, &session_id)
    )
    .await
    .expect("Timeout reconnecting to server")?;

    // Should have the same session ID
    assert_eq!(
        client2.session_id.as_ref(),
        Some(&session_id),
        "Should resume with same session ID"
    );

    // Should still be able to make calls
    let result = timeout(
        Duration::from_secs(5),
        client2.call_tool("graph_find", serde_json::json!({}))
    )
    .await
    .expect("Timeout calling tool after resumption")?;

    assert!(result.is_array(), "Should return an array");

    Ok(())
}

/// Spawns a test server with file-backed audio graph DB for persistence testing.
/// Returns (url, handle, shutdown_sender).
async fn spawn_persistent_server(
    audio_graph_db_path: PathBuf,
) -> Result<(String, tokio::task::JoinHandle<()>, tokio::sync::oneshot::Sender<()>)> {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);

    let base_url = format!("http://127.0.0.1:{}/mcp", port);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let handle = tokio::spawn(async move {
        // Ephemeral temp dir for everything except audio graph
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let state_dir = temp_dir.path();

        let cas_dir = state_dir.join("cas");
        std::fs::create_dir_all(&cas_dir).unwrap();
        let cas = CasFileStore::at_path(&cas_dir).unwrap();
        let local_models = Arc::new(LocalModels::new(cas.clone(), 2000));

        let artifact_store = Arc::new(std::sync::RwLock::new(
            FileStore::new(state_dir.join("artifacts.json")).unwrap()
        ));

        let job_store = Arc::new(JobStore::new());

        // File-backed audio graph DB (the thing we're testing persistence of)
        let audio_graph_db = Arc::new(AudioGraphDb::open(&audio_graph_db_path).unwrap());

        // Create artifact source for Trustfall
        let artifact_source = Arc::new(
            hootenanny::artifact_store::FileStoreSource::new(artifact_store.clone())
        );

        let graph_adapter = Arc::new(
            AudioGraphAdapter::new_with_artifacts(
                audio_graph_db.clone(),
                audio_graph_mcp::PipeWireSnapshot::default(),
                artifact_source,
            ).unwrap()
        );

        let gpu_monitor = Arc::new(hootenanny::gpu_monitor::GpuMonitor::new());

        let event_duality_server = Arc::new(EventDualityServer::new(
            local_models,
            artifact_store,
            job_store,
            audio_graph_db,
            graph_adapter,
            gpu_monitor,
        ));

        let hoot_handler = HootHandler::new(event_duality_server);
        let mcp_state = Arc::new(baton::McpState::new(
            hoot_handler,
            "hootenanny-test",
            env!("CARGO_PKG_VERSION"),
        ));
        let mcp_router = baton::router(mcp_state);
        let app = axum::Router::new().nest("/mcp", mcp_router);

        let addr = format!("127.0.0.1:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

        axum::serve(listener, app)
            .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok((base_url, handle, shutdown_tx))
}

/// The Zombie Session Test: Proves audio graph state persists across server restarts.
///
/// 1. Start server with file-backed audio graph DB
/// 2. Create an identity via graph_bind
/// 3. Shutdown server (graceful)
/// 4. Start NEW server with SAME audio graph DB file
/// 5. Query graph_find - identity should still exist
#[tokio::test]
async fn test_zombie_session_state_persists_across_restarts() -> Result<()> {
    // Shared audio graph DB file (SQLite - no lock issues)
    let temp_dir = tempfile::tempdir()?;
    let audio_graph_db_path = temp_dir.path().join("audio_graph.db");

    eprintln!("[ZOMBIE] Audio graph DB: {:?}", audio_graph_db_path);

    // === Run 1: Create an identity ===
    eprintln!("[ZOMBIE] Starting server run 1...");
    let (url1, mut handle1, shutdown_tx1) = spawn_persistent_server(audio_graph_db_path.clone()).await?;

    let client1 = timeout(
        Duration::from_secs(5),
        common::mcp_client::McpClient::connect(&url1)
    ).await.expect("Timeout connecting (run 1)")?;

    eprintln!("[ZOMBIE] Creating identity...");
    let bind_result = timeout(
        Duration::from_secs(5),
        client1.call_tool("graph_bind", serde_json::json!({
            "id": "zombie-synth",
            "name": "Zombie Synthesizer",
            "hints": []
        }))
    ).await.expect("Timeout calling graph_bind")?;

    assert!(bind_result.get("id").is_some(), "Should create identity");

    let find1 = timeout(
        Duration::from_secs(5),
        client1.call_tool("graph_find", serde_json::json!({}))
    ).await.expect("Timeout calling graph_find (run 1)")?;

    assert_eq!(find1.as_array().unwrap().len(), 1, "Should have 1 identity");

    // Shutdown server 1
    eprintln!("[ZOMBIE] Shutting down server 1...");
    drop(client1);
    shutdown_tx1.send(()).ok();

    // Wait for shutdown or abort
    if timeout(Duration::from_secs(2), &mut handle1).await.is_err() {
        handle1.abort();
        let _ = handle1.await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    // === Run 2: Verify persistence ===
    eprintln!("[ZOMBIE] Starting server run 2...");
    let (url2, _handle2, _shutdown2) = spawn_persistent_server(audio_graph_db_path).await?;

    let client2 = timeout(
        Duration::from_secs(5),
        common::mcp_client::McpClient::connect(&url2)
    ).await.expect("Timeout connecting (run 2)")?;

    let find2 = timeout(
        Duration::from_secs(5),
        client2.call_tool("graph_find", serde_json::json!({}))
    ).await.expect("Timeout calling graph_find (run 2)")?;

    let identities = find2.as_array().expect("Should be array");
    assert_eq!(identities.len(), 1, "Should still have 1 identity after restart");
    assert_eq!(identities[0]["id"], "zombie-synth");
    assert_eq!(identities[0]["name"], "Zombie Synthesizer");

    eprintln!("[ZOMBIE] ✅ Audio graph state persisted across restart!");
    Ok(())
}
