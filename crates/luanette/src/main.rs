mod clients;
mod dispatch;
mod error;
mod handler;
mod job_system;
mod otel_bridge;
mod runtime;
mod schema;
mod stdlib;
mod telemetry;
mod tool_bridge;
mod zmq_server;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dispatch::Dispatcher;
use job_system::JobStore;
use runtime::{LuaRuntime, SandboxConfig};
use std::sync::Arc;
use std::time::Duration;
use zmq_server::{Server, ServerConfig};

/// Luanette - Lua Scripting Server for Hootenanny
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// OTLP gRPC endpoint for OpenTelemetry
    #[arg(long, default_value = "127.0.0.1:35991", global = true)]
    otlp_endpoint: String,

    /// Script execution timeout in seconds
    #[arg(long, default_value = "30", global = true)]
    timeout: u64,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run as ZMQ server (new mode)
    Zmq {
        /// ZMQ bind address
        #[arg(short, long, default_value = "tcp://0.0.0.0:5570")]
        bind: String,

        /// Worker name for identification
        #[arg(long, default_value = "luanette")]
        name: String,
    },

    /// Run as HTTP/MCP server (legacy mode)
    Http {
        /// HTTP port to listen on
        #[arg(short, long, default_value = "8081")]
        port: u16,

        /// Upstream Hootenanny MCP server URL
        #[arg(long, default_value = "http://127.0.0.1:8080/mcp")]
        hootenanny_url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize OpenTelemetry with OTLP exporter
    telemetry::init(&cli.otlp_endpoint)
        .context("Failed to initialize OpenTelemetry")?;

    // Default to ZMQ mode if no subcommand specified
    let command = cli.command.unwrap_or(Commands::Zmq {
        bind: "tcp://0.0.0.0:5570".to_string(),
        name: "luanette".to_string(),
    });

    match command {
        Commands::Zmq { bind, name } => {
            run_zmq_server(&bind, &name, cli.timeout).await?;
        }
        Commands::Http { port, hootenanny_url } => {
            run_http_server(port, &hootenanny_url, cli.timeout, &cli.otlp_endpoint).await?;
        }
    }

    // Shutdown OpenTelemetry and flush remaining spans
    telemetry::shutdown()?;

    Ok(())
}

async fn run_zmq_server(bind: &str, name: &str, timeout_secs: u64) -> Result<()> {
    tracing::info!("🌙 Luanette ZMQ server starting");
    tracing::info!("   Bind: {}", bind);
    tracing::info!("   Name: {}", name);
    tracing::info!("   Timeout: {}s", timeout_secs);

    // Create Lua runtime (no MCP bridge needed for ZMQ mode)
    let sandbox_config = SandboxConfig {
        timeout: Duration::from_secs(timeout_secs),
    };
    let runtime = Arc::new(LuaRuntime::new(sandbox_config));

    // Create job store
    let job_store = Arc::new(JobStore::new());

    // Create dispatcher
    let dispatcher = Dispatcher::new(runtime, job_store);

    // Create server config
    let config = ServerConfig {
        bind_address: bind.to_string(),
        _worker_name: name.to_string(),
    };

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

    // Spawn signal handler
    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received SIGINT, shutting down...");
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
                tracing::info!("Received SIGTERM, shutting down...");
            }
        }
        let _ = shutdown_tx.send(());
    });

    // Run server
    let server = Server::new(config, dispatcher);
    server.run(shutdown_rx).await?;

    tracing::info!("Shutdown complete");
    Ok(())
}

async fn run_http_server(port: u16, hootenanny_url: &str, timeout_secs: u64, _otlp: &str) -> Result<()> {
    use clients::{ClientManager, UpstreamConfig};
    use handler::LuanetteHandler;
    use std::time::Instant;
    use tokio_util::sync::CancellationToken;

    // Create client manager for upstream MCP servers
    let client_manager = Arc::new(ClientManager::new());

    // Connect to hootenanny upstream
    tracing::info!("Connecting to hootenanny at {}", hootenanny_url);
    client_manager
        .add_upstream(UpstreamConfig {
            namespace: "hootenanny".to_string(),
            url: hootenanny_url.to_string(),
        })
        .await
        .context("Failed to connect to hootenanny")?;

    tracing::info!("Connected to hootenanny");

    // Create Lua runtime with MCP bridge enabled
    let sandbox_config = SandboxConfig {
        timeout: Duration::from_secs(timeout_secs),
    };
    let runtime = Arc::new(LuaRuntime::with_mcp_bridge(sandbox_config, client_manager.clone()));

    // Create job store
    let job_store = Arc::new(JobStore::new());

    // Create the handler
    let handler = LuanetteHandler::new(runtime, client_manager.clone(), job_store);

    // Create baton MCP state
    let mcp_state = Arc::new(baton::McpState::new(
        handler,
        "luanette",
        env!("CARGO_PKG_VERSION"),
    ));

    let shutdown_token = CancellationToken::new();

    // Create routers
    let mcp_router = baton::dual_router(mcp_state.clone());

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

    async fn no_oauth() -> impl axum::response::IntoResponse {
        (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            r#"{"error": "not_found", "error_description": "No OAuth required"}"#
        )
    }

    let health_router = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .with_state(health_state);

    let app_router = axum::Router::new()
        .merge(health_router)
        .route("/mcp/.well-known/oauth-authorization-server", axum::routing::get(no_oauth))
        .route("/mcp/.well-known/oauth-protected-resource", axum::routing::get(no_oauth))
        .route("/.well-known/oauth-authorization-server", axum::routing::get(no_oauth))
        .route("/.well-known/oauth-protected-resource", axum::routing::get(no_oauth))
        .nest("/mcp", mcp_router);

    let addr = format!("0.0.0.0:{}", port);

    tracing::info!("🌙 Luanette HTTP/MCP Server starting on http://{}", addr);
    tracing::info!("   MCP Streamable HTTP: POST http://{}/mcp", addr);
    tracing::info!("   Health: GET http://{}/health", addr);

    let bind_addr: std::net::SocketAddr = addr.parse().context("Failed to parse bind address")?;
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    let shutdown_token_srv = shutdown_token.clone();
    let server = axum::serve(listener, app_router).with_graceful_shutdown(async move {
        shutdown_token_srv.cancelled().await;
    });

    tokio::spawn(async move {
        if let Err(e) = server.await {
            tracing::error!("Server error: {:?}", e);
        }
    });

    tracing::info!("🌙 Luanette ready!");

    baton::spawn_cleanup_task(
        mcp_state.sessions.clone(),
        Duration::from_secs(30),
        Duration::from_secs(1800),
        shutdown_token.clone(),
    );

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT, shutting down...");
            shutdown_token.cancel();
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
            tracing::info!("Received SIGTERM, shutting down...");
            shutdown_token.cancel();
        }
    }

    tracing::info!("Shutdown complete");
    Ok(())
}
