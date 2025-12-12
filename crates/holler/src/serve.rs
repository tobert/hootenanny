//! MCP gateway server implementation
//!
//! Uses baton for MCP protocol handling, forwarding tool calls to ZMQ backends.

use anyhow::{Context, Result};
use axum::{routing::get, Router};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::backend::BackendPool;
use crate::handler::ZmqHandler;
use crate::subscriber::spawn_subscribers;

/// Server configuration
pub struct ServeConfig {
    pub port: u16,
    /// ROUTER endpoints for request/response
    pub luanette: Option<String>,
    pub hootenanny: Option<String>,
    pub chaosgarden: Option<String>,
    /// PUB endpoints for broadcasts
    pub luanette_pub: Option<String>,
    pub hootenanny_pub: Option<String>,
    pub chaosgarden_pub: Option<String>,
}

/// Server state for health endpoint
#[derive(Clone)]
pub struct HealthState {
    pub backends: Arc<BackendPool>,
    pub start_time: Instant,
}

/// Health check endpoint
pub async fn handle_health(
    axum::extract::State(state): axum::extract::State<HealthState>,
) -> axum::Json<serde_json::Value> {
    let uptime = state.start_time.elapsed();

    axum::Json(serde_json::json!({
        "status": "healthy",
        "uptime_secs": uptime.as_secs(),
        "version": env!("CARGO_PKG_VERSION"),
        "backends": {
            "luanette": state.backends.luanette.is_some(),
            "hootenanny": state.backends.hootenanny.is_some(),
            "chaosgarden": state.backends.chaosgarden.is_some(),
        }
    }))
}

/// Run the MCP gateway server
pub async fn run(config: ServeConfig) -> Result<()> {
    info!("🎺 Holler MCP gateway starting");
    info!("   Port: {}", config.port);

    // Create backend pool
    let mut backends = BackendPool::new();

    // Connect to backends - Luanette is optional (may not be running yet due to circular dep)
    if let Some(ref endpoint) = config.luanette {
        info!("   Connecting to Luanette at {}", endpoint);
        match backends.connect_luanette(endpoint, 30000).await {
            Ok(()) => info!("   ✅ Connected to Luanette"),
            Err(e) => {
                tracing::warn!("   ⚠️  Luanette not available (will work without Lua scripting): {}", e);
            }
        }
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

    let backends = Arc::new(backends);

    // Spawn ZMQ SUB subscribers for backend broadcasts (TODO: wire to baton notifications)
    let has_subs = config.luanette_pub.is_some()
        || config.hootenanny_pub.is_some()
        || config.chaosgarden_pub.is_some();

    if has_subs {
        info!("   Subscribing to backend broadcasts...");
        if let Some(ref ep) = config.luanette_pub {
            info!("      Luanette PUB: {}", ep);
        }
        if let Some(ref ep) = config.hootenanny_pub {
            info!("      Hootenanny PUB: {}", ep);
        }
        if let Some(ref ep) = config.chaosgarden_pub {
            info!("      Chaosgarden PUB: {}", ep);
        }

        // Create a dummy broadcast channel for now - subscribers will be updated
        // to use baton's notification system in a future iteration
        let (broadcast_tx, _) = tokio::sync::broadcast::channel::<hooteproto::Broadcast>(256);
        spawn_subscribers(
            broadcast_tx,
            config.luanette_pub,
            config.hootenanny_pub,
            config.chaosgarden_pub,
        );
    }

    // Create baton MCP state with our ZMQ handler
    let handler = ZmqHandler::new(Arc::clone(&backends));
    let mcp_state = Arc::new(baton::McpState::new(
        handler,
        "holler",
        env!("CARGO_PKG_VERSION"),
    ));

    // Spawn session cleanup task
    let cancel_token = CancellationToken::new();
    let _cleanup_handle = baton::spawn_cleanup_task(
        Arc::clone(&mcp_state.sessions),
        Duration::from_secs(60),
        Duration::from_secs(300),
        cancel_token.clone(),
    );

    // Health check state
    let health_state = HealthState {
        backends: Arc::clone(&backends),
        start_time: Instant::now(),
    };

    // Build router - use baton's dual_router for MCP, add health endpoint
    let mcp_router = baton::dual_router(mcp_state);

    // Health endpoint needs its own router merged in
    let health_router = Router::new()
        .route("/health", get(handle_health))
        .with_state(health_state);

    let app = Router::new()
        .nest("/mcp", mcp_router)
        .merge(health_router);

    // Bind and serve
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;

    info!("🎺 Holler ready!");
    info!("   MCP (Streamable): POST http://{}/mcp", addr);
    info!("   MCP (SSE): GET http://{}/mcp/sse + POST http://{}/mcp/message", addr, addr);
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
