//! MCP gateway server implementation
//!
//! Uses baton for MCP protocol handling, forwarding tool calls to ZMQ backends.

use anyhow::{Context, Result};
use axum::{routing::get, Router};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::backend::{Backend, BackendPool};
use crate::handler::ZmqHandler;
use crate::heartbeat::{BackendState, HeartbeatConfig, HeartbeatResult};
use crate::subscriber::spawn_subscribers;

/// Server configuration
///
/// Holler now connects only to hootenanny, which proxies to luanette and chaosgarden.
pub struct ServeConfig {
    pub port: u16,
    /// Hootenanny ZMQ ROUTER endpoint (required - handles all tools)
    pub hootenanny: String,
    /// Hootenanny ZMQ PUB endpoint (optional - for broadcasts/SSE)
    pub hootenanny_pub: Option<String>,
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
    let backends_health = state.backends.health().await;
    let all_alive = state.backends.all_alive();

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

    // Connect to hootenanny (the unified backend that proxies to luanette and chaosgarden)
    info!("   Connecting to Hootenanny at {}", config.hootenanny);
    let mut backends = BackendPool::new();
    backends
        .connect_hootenanny(&config.hootenanny, 5000)
        .await
        .context("Failed to connect to Hootenanny")?;
    info!("   ✅ Connected to Hootenanny");

    let backends = Arc::new(backends);

    // Create shutdown channel for heartbeat tasks
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // Spawn heartbeat task for hootenanny
    let heartbeat_config = HeartbeatConfig::default();
    spawn_heartbeat_tasks(&backends, heartbeat_config, shutdown_tx.subscribe());

    // Spawn ZMQ SUB subscriber for hootenanny broadcasts
    if let Some(ref hootenanny_pub) = config.hootenanny_pub {
        info!("   Subscribing to Hootenanny broadcasts at {}", hootenanny_pub);
        let (broadcast_tx, _) = tokio::sync::broadcast::channel::<hooteproto::Broadcast>(256);
        spawn_subscribers(
            broadcast_tx,
            None, // luanette_pub - removed
            Some(hootenanny_pub.clone()),
            None, // chaosgarden_pub - removed
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

/// Spawn heartbeat monitoring task for hootenanny
fn spawn_heartbeat_tasks(
    backends: &Arc<BackendPool>,
    config: HeartbeatConfig,
    shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    // Spawn heartbeat for hootenanny (the only backend)
    if let Some(ref backend) = backends.hootenanny {
        spawn_backend_heartbeat("hootenanny", Arc::clone(backend), config, shutdown);
    }
}

/// Spawn a heartbeat task for a single backend
fn spawn_backend_heartbeat(
    name: &'static str,
    backend: Arc<Backend>,
    config: HeartbeatConfig,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        info!("💓 Heartbeat monitoring started for {}", name);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let result = backend.send_heartbeat(config.timeout).await;

                    match result {
                        HeartbeatResult::Success => {
                            backend.health.record_message_received().await;
                            debug!("{}: heartbeat OK", name);
                        }
                        HeartbeatResult::Timeout => {
                            let failures = backend.health.record_failure();
                            warn!("{}: heartbeat timeout ({}/{})", name, failures, config.max_failures);

                            if failures >= config.max_failures {
                                let prev = backend.health.set_state(BackendState::Dead);
                                if prev != BackendState::Dead {
                                    warn!("{}: marked as DEAD after {} consecutive failures", name, failures);
                                }
                            }
                        }
                        HeartbeatResult::Error(e) => {
                            let failures = backend.health.record_failure();
                            warn!("{}: heartbeat error: {} ({}/{})", name, e, failures, config.max_failures);

                            if failures >= config.max_failures {
                                let prev = backend.health.set_state(BackendState::Dead);
                                if prev != BackendState::Dead {
                                    warn!("{}: marked as DEAD after {} consecutive failures", name, failures);
                                }
                            }
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("{}: heartbeat task shutting down", name);
                    break;
                }
            }
        }
    });
}
