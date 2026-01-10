//! Stdio MCP transport for Claude Code and other stdio-based clients.
//!
//! This provides a simpler transport that doesn't require HTTP session management.
//! Both stdio and stateful HTTP support server-initiated notifications/push.

use anyhow::{Context, Result};
use rmcp::{transport::stdio, ServiceExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::backend::BackendPool;
use crate::handler::{new_tool_cache, refresh_tools_into, ZmqHandler};

/// Configuration for stdio MCP server
pub struct StdioConfig {
    /// Hootenanny ZMQ ROUTER endpoint
    pub hootenanny: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Only expose DAW tools (sample, extend, analyze, bridge, project, schedule)
    pub daw_only: bool,
}

/// Run MCP server over stdio (stdin/stdout).
///
/// This is designed for Claude Code and other clients that prefer stdio transport.
/// The server will:
/// 1. Connect to hootenanny via ZMQ (non-blocking, lazy)
/// 2. Start serving immediately (tools refresh in background when hootenanny responds)
/// 3. Serve MCP protocol over stdin/stdout until EOF
pub async fn run(config: StdioConfig) -> Result<()> {
    // Set up connection to hootenanny (ZMQ connect is non-blocking)
    let mut backends = BackendPool::new();
    backends
        .setup_hootenanny(&config.hootenanny, config.timeout_ms)
        .await;

    let backends = Arc::new(RwLock::new(backends));

    // Create shared tool cache (starts empty, populated when hootenanny responds)
    let tool_cache = new_tool_cache();

    // Create shutdown channel for health tasks
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // Create callback that triggers tool refresh when backend connects
    let cache_for_callback = tool_cache.clone();
    let backends_for_callback = Arc::clone(&backends);
    let on_connected: Box<dyn Fn() + Send + Sync + 'static> = Box::new(move || {
        let cache = cache_for_callback.clone();
        let backends = Arc::clone(&backends_for_callback);
        tokio::spawn(async move {
            let count = refresh_tools_into(&cache, &backends).await;
            info!("🔄 Backend connected - refreshed {} tools", count);
        });
    });

    // Spawn health task for hootenanny with connect callback
    {
        let backends_guard = backends.read().await;
        backends_guard.spawn_health_task(shutdown_tx.subscribe(), Some(on_connected));
    }

    // Spawn periodic recreation check - recovers Dead connections
    {
        let backends_for_recreation = Arc::clone(&backends);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                let needs_recreation = {
                    backends_for_recreation.read().await.needs_recreation()
                };
                if needs_recreation {
                    warn!("Backend marked dead, attempting recreation");
                    let mut backends_mut = backends_for_recreation.write().await;
                    if let Err(e) = backends_mut.recreate_hootenanny().await {
                        warn!("Failed to recreate backend: {}", e);
                    }
                }
            }
        });
    }

    // Block until hootenanny is reachable (MCP clients expect tools immediately)
    let connected = {
        let backends_guard = backends.read().await;
        if let Some(ref client) = backends_guard.hootenanny {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut connected = false;
            while std::time::Instant::now() < deadline {
                match client.heartbeat().await {
                    Ok(_) => {
                        info!("Hootenanny connected");
                        connected = true;
                        break;
                    }
                    Err(_) => {
                        // Retry silently - heartbeat failures during startup are expected
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
            if !connected {
                warn!("Timed out waiting for hootenanny after 10s");
            }
            connected
        } else {
            warn!("No hootenanny client configured");
            false
        }
    };

    // Load tools synchronously before serving
    let tool_count = refresh_tools_into(&tool_cache, &backends).await;
    if tool_count > 0 {
        info!("🔧 Loaded {} tools", tool_count);
    } else if !connected {
        eprintln!("Warning: No tools loaded from hootenanny - is it running?");
    }

    // Create handler with shared cache and daw_only filter
    let handler = ZmqHandler::with_shared_cache(Arc::clone(&backends), tool_cache, config.daw_only);

    // Serve via stdio - rmcp handles JSON-RPC framing
    let service = handler
        .serve(stdio())
        .await
        .context("Failed to start stdio MCP service")?;

    info!("Stdio MCP server running");

    // Wait for completion (EOF or error)
    service.waiting().await?;

    info!("Stdio MCP server shutdown");
    Ok(())
}
