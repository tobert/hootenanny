//! MCP gateway server implementation
//!
//! Uses rmcp for MCP protocol handling, forwarding tool calls to ZMQ backends.
//! Uses hooteproto's HootClient for connection management with built-in heartbeat.
//!
//! Startup is lazy following zguide patterns:
//! - ZMQ connect() is non-blocking, peer doesn't need to exist
//! - Health task monitors peer responsiveness and triggers tool refresh on connect
//! - Services can start in any order

use anyhow::{Context, Result};
use axum::{routing::get, Router};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
    session::local::LocalSessionManager,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::backend::BackendPool;
use crate::handler::{new_tool_cache, refresh_tools_into, ZmqHandler};
use crate::subscriber::spawn_subscribers;

/// Server configuration
///
/// Holler connects only to hootenanny, which proxies to vibeweaver and chaosgarden.
pub struct ServeConfig {
    pub port: u16,
    /// Hootenanny ZMQ ROUTER endpoint (required - handles all tools)
    pub hootenanny: String,
    /// Hootenanny ZMQ PUB endpoint (optional - for broadcasts/SSE)
    pub hootenanny_pub: Option<String>,
    /// Request timeout in milliseconds (should be > inner service timeouts)
    pub timeout_ms: u64,
    /// Only expose native tools (sample, extend, analyze, bridge, project, schedule)
    pub native_only: bool,
}

/// Server state for health endpoint
#[derive(Clone)]
pub struct HealthState {
    pub backends: Arc<RwLock<BackendPool>>,
    pub start_time: Instant,
}

/// Health check endpoint
pub async fn handle_health(
    axum::extract::State(state): axum::extract::State<HealthState>,
) -> axum::Json<serde_json::Value> {
    let uptime = state.start_time.elapsed();
    let backends = state.backends.read().await;
    let backends_health = backends.health().await;
    let all_alive = backends.all_alive();

    axum::Json(serde_json::json!({
        "status": if all_alive { "healthy" } else { "degraded" },
        "uptime_secs": uptime.as_secs(),
        "version": env!("CARGO_PKG_VERSION"),
        "backends": backends_health,
    }))
}

/// Run the MCP gateway server
pub async fn run(config: ServeConfig) -> Result<()> {
    info!("🎺 Holler MCP gateway starting");
    info!("   Port: {}", config.port);

    // Set up connection to hootenanny (ZMQ connect is non-blocking)
    // The health task will monitor peer responsiveness and trigger tool refresh
    info!(
        "   Connecting to Hootenanny at {} (ZMQ lazy connect)",
        config.hootenanny
    );
    let mut backends = BackendPool::new();
    backends
        .setup_hootenanny(&config.hootenanny, config.timeout_ms)
        .await;

    let backends = Arc::new(RwLock::new(backends));

    // Create shared tool cache for dynamic refresh
    // Tools will be loaded when on_connected callback fires after first heartbeat success
    let tool_cache = new_tool_cache();
    info!("   📋 Tool cache initialized (will load on first heartbeat success)");

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
                        error!("Failed to recreate backend: {}", e);
                    }
                }
            }
        });
    }

    // Also do an immediate tool refresh (in case backend is already up)
    {
        let cache = tool_cache.clone();
        let backends = Arc::clone(&backends);
        tokio::spawn(async move {
            // Small delay to let ZMQ connection establish
            tokio::time::sleep(Duration::from_millis(500)).await;
            let count = refresh_tools_into(&cache, &backends).await;
            if count > 0 {
                info!("🔧 Initial tool refresh: {} tools loaded", count);
            }
        });
    }

    // Spawn ZMQ SUB subscriber for hootenanny broadcasts
    if let Some(ref hootenanny_pub) = config.hootenanny_pub {
        info!(
            "   Subscribing to Hootenanny broadcasts at {}",
            hootenanny_pub
        );
        let (broadcast_tx, _) = tokio::sync::broadcast::channel::<hooteproto::Broadcast>(256);
        spawn_subscribers(
            broadcast_tx,
            Some(hootenanny_pub.clone()),
            None, // chaosgarden_pub - direct connection removed
        );
    }

    // Create cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();

    // Create MCP service using rmcp's StreamableHttpService
    // The service factory creates a fresh handler for each session
    let backends_for_factory = Arc::clone(&backends);
    let cache_for_factory = tool_cache.clone();
    let native_only = config.native_only;
    let service = StreamableHttpService::new(
        move || Ok(ZmqHandler::with_shared_cache(
            Arc::clone(&backends_for_factory),
            cache_for_factory.clone(),
            native_only,
        )),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig {
            cancellation_token: cancel_token.child_token(),
            stateful_mode: true, // Full MCP protocol for clients that support it
            ..Default::default()
        },
    );

    // Health check state
    let health_state = HealthState {
        backends: Arc::clone(&backends),
        start_time: Instant::now(),
    };

    // Build router - nest MCP service at /mcp, add health endpoint
    let health_router = Router::new()
        .route("/health", get(handle_health))
        .with_state(health_state);

    let app = Router::new()
        .nest_service("/mcp", service)
        .merge(health_router);

    // Bind and serve
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;

    info!("🎺 Holler ready!");
    info!("   MCP (Streamable): POST http://{}/mcp", addr);
    info!("   Health: GET http://{}/health", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(cancel_token))
        .await
        .context("Server error")?;

    info!("Shutdown complete");
    Ok(())
}

async fn shutdown_signal(cancel_token: CancellationToken) {
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
    cancel_token.cancel();
}
