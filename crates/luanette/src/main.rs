mod clients;
mod error;
mod handler;
mod job_system;
mod otel_bridge;
mod runtime;
mod schema;
mod stdlib;
mod telemetry;
mod tool_bridge;

use anyhow::{Context, Result};
use clap::Parser;
use clients::{ClientManager, UpstreamConfig};
use handler::LuanetteHandler;
use job_system::JobStore;
use runtime::{LuaRuntime, SandboxConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Luanette - Lua Scripting MCP Server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value = "8081")]
    port: u16,

    /// Script execution timeout in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,

    /// Upstream Hootenanny MCP server URL
    #[arg(long, default_value = "http://127.0.0.1:8080/mcp")]
    hootenanny_url: String,

    /// OTLP gRPC endpoint for OpenTelemetry (e.g., "127.0.0.1:35991")
    #[arg(long, default_value = "127.0.0.1:35991")]
    otlp_endpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize OpenTelemetry with OTLP exporter
    telemetry::init(&cli.otlp_endpoint)
        .context("Failed to initialize OpenTelemetry")?;

    // Create client manager for upstream MCP servers
    let client_manager = Arc::new(ClientManager::new());

    // Connect to hootenanny upstream (must happen before creating runtime)
    tracing::info!("Connecting to hootenanny at {}", cli.hootenanny_url);
    client_manager
        .add_upstream(UpstreamConfig {
            namespace: "hootenanny".to_string(),
            url: cli.hootenanny_url.clone(),
        })
        .await
        .context("Failed to connect to hootenanny")?;

    tracing::info!("Connected to hootenanny");

    // Create Lua runtime with MCP bridge enabled
    // This allows Lua scripts to call upstream tools via mcp.* globals
    let sandbox_config = SandboxConfig {
        timeout: Duration::from_secs(cli.timeout),
    };
    let runtime = Arc::new(LuaRuntime::with_mcp_bridge(sandbox_config, client_manager.clone()));

    // Create job store for async script execution
    let job_store = Arc::new(JobStore::new());

    // Create the handler with runtime, client manager, and job store
    let handler = LuanetteHandler::new(runtime, client_manager.clone(), job_store);

    // Create baton MCP state
    let mcp_state = Arc::new(baton::McpState::new(
        handler,
        "luanette",
        env!("CARGO_PKG_VERSION"),
    ));

    let shutdown_token = CancellationToken::new();

    // Create routers
    // dual_router supports both Streamable HTTP (POST /) and SSE (GET /sse + POST /message)
    let mcp_router = baton::dual_router(mcp_state.clone());

    // Track server start time for uptime
    let server_start = Instant::now();

    // Health endpoint
    #[derive(Clone)]
    struct HealthState {
        sessions: Arc<dyn baton::SessionStore>,
        start_time: Instant,
    }

    async fn health_handler(
        axum::extract::State(state): axum::extract::State<HealthState>,
    ) -> axum::Json<serde_json::Value> {
        let session_stats = state.sessions.stats();
        let uptime = state.start_time.elapsed();

        axum::Json(serde_json::json!({
            "status": "healthy",
            "uptime_secs": uptime.as_secs(),
            "version": env!("CARGO_PKG_VERSION"),
            "sessions": {
                "total": session_stats.total,
                "connected": session_stats.connected,
            }
        }))
    }

    let health_state = HealthState {
        sessions: mcp_state.sessions.clone(),
        start_time: server_start,
    };

    // Handler for OAuth discovery - return 404 to indicate no OAuth required
    async fn no_oauth() -> impl axum::response::IntoResponse {
        (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            r#"{"error": "not_found", "error_description": "This MCP server does not require authentication"}"#
        )
    }

    // Build the main application router
    let health_router = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .with_state(health_state);

    let app_router = axum::Router::new()
        .merge(health_router)
        .route(
            "/mcp/.well-known/oauth-authorization-server",
            axum::routing::get(no_oauth),
        )
        .route(
            "/mcp/.well-known/oauth-protected-resource",
            axum::routing::get(no_oauth),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            axum::routing::get(no_oauth),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            axum::routing::get(no_oauth),
        )
        .nest("/mcp", mcp_router);

    let addr = format!("0.0.0.0:{}", cli.port);

    tracing::info!("🌙 Luanette MCP Server starting on http://{}", addr);
    tracing::info!("   MCP Streamable HTTP: POST http://{}/mcp", addr);
    tracing::info!("   MCP SSE (legacy): GET http://{}/mcp/sse", addr);
    tracing::info!("   Health: GET http://{}/health", addr);
    tracing::info!("   Script timeout: {}s", cli.timeout);

    let bind_addr: std::net::SocketAddr = addr.parse().context("Failed to parse bind address")?;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    let shutdown_token_srv = shutdown_token.clone();
    let server = axum::serve(listener, app_router).with_graceful_shutdown(async move {
        shutdown_token_srv.cancelled().await;
        tracing::info!("Server shutdown signal received");
    });

    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!("Server shutdown with error: {:?}", e);
        }
    });

    tracing::info!("🌙 Luanette ready!");

    // Spawn background task for session cleanup
    baton::spawn_cleanup_task(
        mcp_state.sessions.clone(),
        Duration::from_secs(30),   // cleanup interval
        Duration::from_secs(1800), // 30 min max idle
        shutdown_token.clone(),
    );

    // Handle both SIGINT (Ctrl+C) and SIGTERM
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT (Ctrl+C), shutting down gracefully...");
            shutdown_token.cancel();
        }
        _ = async {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sigterm = signal(SignalKind::terminate()).expect("Failed to setup SIGTERM handler");
                sigterm.recv().await;
            }
            #[cfg(not(unix))]
            {
                std::future::pending::<()>().await;
            }
        } => {
            tracing::info!("Received SIGTERM, shutting down gracefully...");
            shutdown_token.cancel();
        }
    }

    tracing::info!("Shutdown complete");

    // Shutdown OpenTelemetry and flush remaining spans
    telemetry::shutdown()?;

    Ok(())
}
