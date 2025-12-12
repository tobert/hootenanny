//! MCP gateway server implementation

use anyhow::{Context, Result};
use axum::{routing::{get, post}, Router};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

use crate::backend::BackendPool;
use crate::mcp::{handle_health, handle_mcp, AppState};
use crate::sse::{create_broadcast_channel, sse_handler};

/// Server configuration
pub struct ServeConfig {
    pub port: u16,
    pub luanette: Option<String>,
    pub hootenanny: Option<String>,
    pub chaosgarden: Option<String>,
}

/// Run the MCP gateway server
pub async fn run(config: ServeConfig) -> Result<()> {
    info!("🎺 Holler MCP gateway starting");
    info!("   Port: {}", config.port);

    // Create backend pool
    let mut backends = BackendPool::new();

    // Connect to backends
    if let Some(ref endpoint) = config.luanette {
        info!("   Connecting to Luanette at {}", endpoint);
        backends
            .connect_luanette(endpoint, 30000)
            .await
            .context("Failed to connect to Luanette")?;
    }

    if let Some(ref endpoint) = config.hootenanny {
        info!("   Connecting to Hootenanny at {}", endpoint);
        backends
            .connect_hootenanny(endpoint, 5000)
            .await
            .context("Failed to connect to Hootenanny")?;
    }

    if let Some(ref endpoint) = config.chaosgarden {
        info!("   Connecting to Chaosgarden at {}", endpoint);
        backends
            .connect_chaosgarden(endpoint, 1000)
            .await
            .context("Failed to connect to Chaosgarden")?;
    }

    // Create broadcast channel for SSE events
    let (broadcast_tx, _broadcast_rx) = create_broadcast_channel();

    // Create shared state
    let state = AppState {
        backends: Arc::new(backends),
        start_time: Instant::now(),
        broadcast_tx,
    };

    // Build router
    let app = Router::new()
        .route("/mcp", post(handle_mcp))
        .route("/sse", get(sse_handler))
        .route("/health", get(handle_health))
        .with_state(state);

    // Bind and serve
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;

    info!("🎺 Holler ready!");
    info!("   MCP: POST http://{}/mcp", addr);
    info!("   SSE: GET http://{}/sse", addr);
    info!("   Health: GET http://{}/health", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    info!("Shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down...");
        }
        _ = async {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm = signal(SignalKind::terminate()).expect("Failed to setup SIGTERM");
                sigterm.recv().await;
            }
            #[cfg(not(unix))]
            {
                std::future::pending::<()>().await;
            }
        } => {
            info!("Received SIGTERM, shutting down...");
        }
    }
}
