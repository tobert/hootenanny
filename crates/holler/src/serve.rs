//! MCP gateway server implementation
//!
//! Uses baton for MCP protocol handling, forwarding tool calls to ZMQ backends.
//! Implements Phase 6 of HOOT01 protocol: tool refresh on backend recovery.

use anyhow::{Context, Result};
use axum::{routing::get, Router};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::backend::{Backend, BackendPool};
use crate::handler::{new_tool_cache, refresh_tools_into, ZmqHandler};
use crate::heartbeat::{BackendState, HeartbeatConfig, HeartbeatResult};
use crate::subscriber::spawn_subscribers;

/// Callback type for backend recovery events
pub type RecoveryCallback = Arc<dyn Fn() + Send + Sync + 'static>;

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

    // Set up lazy connection to hootenanny (non-blocking startup)
    // The heartbeat task will handle initial connection and retries
    info!("   Will connect to Hootenanny at {} (lazy)", config.hootenanny);
    let mut backends = BackendPool::new();
    backends.setup_hootenanny_lazy(&config.hootenanny, 5000);

    let backends = Arc::new(backends);

    // Create shared tool cache for dynamic refresh
    let tool_cache = new_tool_cache();

    // Try initial tool refresh, but don't fail if backend isn't ready yet
    let initial_tools = refresh_tools_into(&tool_cache, &backends).await;
    if initial_tools > 0 {
        info!("   📋 Loaded {} tools from hootenanny", initial_tools);
    } else {
        info!("   📋 No tools loaded yet (backend connecting in background)");
    }

    // Create shutdown channel for heartbeat tasks
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // Create recovery callback that triggers tool refresh into shared cache
    let cache_for_recovery = tool_cache.clone();
    let backends_for_recovery = Arc::clone(&backends);
    let recovery_callback: RecoveryCallback = Arc::new(move || {
        let cache = cache_for_recovery.clone();
        let backends = Arc::clone(&backends_for_recovery);
        tokio::spawn(async move {
            let count = refresh_tools_into(&cache, &backends).await;
            info!("🔄 Backend recovered - refreshed {} tools", count);
        });
    });

    // Spawn heartbeat task for hootenanny with recovery callback
    let heartbeat_config = HeartbeatConfig::default();
    spawn_heartbeat_tasks(&backends, heartbeat_config, shutdown_tx.subscribe(), Some(recovery_callback));

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

    // Create MCP state with handler using the shared cache
    let handler = ZmqHandler::with_shared_cache(Arc::clone(&backends), tool_cache);
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
    recovery_callback: Option<RecoveryCallback>,
) {
    // Spawn heartbeat for hootenanny (the only backend)
    if let Some(ref backend) = backends.hootenanny {
        spawn_backend_heartbeat("hootenanny", Arc::clone(backend), config, shutdown, recovery_callback);
    }
}

/// Spawn a heartbeat task for a single backend
///
/// Implements the Paranoid Pirate pattern from ZMQ Guide Chapter 4:
/// - Periodic heartbeat probes
/// - Exponential backoff reconnection on failure
/// - Socket close/reopen (not just ZMQ auto-reconnect)
/// - Tool refresh on Dead → Ready transition
fn spawn_backend_heartbeat(
    name: &'static str,
    backend: Arc<Backend>,
    config: HeartbeatConfig,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
    recovery_callback: Option<RecoveryCallback>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        info!("💓 Heartbeat monitoring started for {}", name);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Capture state before heartbeat attempt
                    let state_before = backend.health.get_state();

                    // If we're dead/disconnected, try to reconnect first
                    if state_before == BackendState::Dead || !backend.is_connected().await {
                        info!("{}: attempting reconnection...", name);
                        match backend.reconnect().await {
                            Ok(true) => {
                                info!("{}: reconnection successful", name);
                                // Continue to send heartbeat to verify connection
                            }
                            Ok(false) => {
                                debug!("{}: reconnection failed, will retry", name);
                                continue; // Skip heartbeat, try again next interval
                            }
                            Err(e) => {
                                warn!("{}: reconnection error: {}", name, e);
                                continue;
                            }
                        }
                    }

                    let result = backend.send_heartbeat(config.timeout).await;

                    match result {
                        HeartbeatResult::Success => {
                            backend.health.record_message_received().await;
                            debug!("{}: heartbeat OK", name);

                            // Detect Dead → Ready transition (Phase 6: tool refresh)
                            if state_before == BackendState::Dead {
                                info!("🔄 {}: recovered from DEAD state", name);
                                if let Some(ref callback) = recovery_callback {
                                    callback();
                                }
                            }
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
                        HeartbeatResult::Disconnected => {
                            // Socket is gone - mark dead and trigger reconnection on next tick
                            warn!("{}: socket disconnected, marking as DEAD", name);
                            backend.health.set_state(BackendState::Dead);
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
