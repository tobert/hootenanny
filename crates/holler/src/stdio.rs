//! Stdio MCP transport for Claude Code and other stdio-based clients.
//!
//! This provides a simpler transport that doesn't require HTTP session management.
//! Both stdio and stateful HTTP support server-initiated notifications/push.

use anyhow::{Context, Result};
use rmcp::{transport::stdio, ServiceExt};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::backend::BackendPool;
use crate::handler::{new_tool_cache, refresh_tools_into, ZmqHandler};

/// Configuration for stdio MCP server
pub struct StdioConfig {
    /// Hootenanny ZMQ ROUTER endpoint
    pub hootenanny: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
}

/// Run MCP server over stdio (stdin/stdout).
///
/// This is designed for Claude Code and other clients that prefer stdio transport.
/// The server will:
/// 1. Connect to hootenanny via ZMQ (non-blocking, lazy)
/// 2. Refresh tools when hootenanny becomes responsive
/// 3. Serve MCP protocol over stdin/stdout until EOF
pub async fn run(config: StdioConfig) -> Result<()> {
    // Set up connection to hootenanny (ZMQ connect is non-blocking)
    let mut backends = BackendPool::new();
    backends
        .setup_hootenanny(&config.hootenanny, config.timeout_ms)
        .await;

    let backends = Arc::new(RwLock::new(backends));

    // Create shared tool cache
    let tool_cache = new_tool_cache();

    // Eagerly refresh tools (blocking wait for first response)
    // This ensures tools are available before we start serving
    let tool_count = refresh_tools_into(&tool_cache, &backends).await;
    if tool_count == 0 {
        // Log to stderr since stdout is for MCP protocol
        eprintln!("Warning: No tools loaded from hootenanny - is it running?");
    }

    // Create handler with shared cache
    let handler = ZmqHandler::with_shared_cache(Arc::clone(&backends), tool_cache);

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
