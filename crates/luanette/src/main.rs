mod clients;
mod dispatch;
mod error;
// mod handler; // Removed: HTTP handler factored out
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
use clients::{ClientManager, UpstreamConfig};

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
    /// Run as ZMQ server
    Zmq {
        /// ZMQ bind address
        #[arg(short, long, default_value = "tcp://0.0.0.0:5570")]
        bind: String,

        /// Worker name for identification
        #[arg(long, default_value = "luanette")]
        name: String,

        /// Hootenanny ZMQ endpoint for tool calls
        #[arg(long, default_value = "tcp://localhost:5580")]
        hootenanny: String,
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
        hootenanny: "tcp://localhost:5580".to_string(),
    });

    match command {
        Commands::Zmq { bind, name, hootenanny } => {
            run_zmq_server(&bind, &name, cli.timeout, &hootenanny).await?;
        }
    }

    // Shutdown OpenTelemetry and flush remaining spans
    telemetry::shutdown()?;

    Ok(())
}

async fn run_zmq_server(bind: &str, name: &str, timeout_secs: u64, hootenanny_endpoint: &str) -> Result<()> {
    tracing::info!("🌙 Luanette ZMQ server starting");
    tracing::info!("   Bind: {}", bind);
    tracing::info!("   Name: {}", name);
    tracing::info!("   Timeout: {}s", timeout_secs);
    tracing::info!("   Hootenanny: {}", hootenanny_endpoint);

    // Create client manager and connect to hootenanny directly via ZMQ
    let mut client_manager = ClientManager::new();

    tracing::info!("Connecting to hootenanny at {}", hootenanny_endpoint);
    client_manager
        .add_upstream(UpstreamConfig {
            namespace: "hootenanny".to_string(),
            endpoint: hootenanny_endpoint.to_string(),
            timeout_ms: timeout_secs * 1000,
        })
        .await
        .context("Failed to connect to hootenanny")?;

    tracing::info!("Connected to hootenanny");
    let client_manager = Arc::new(client_manager);

    // Create Lua runtime WITH MCP bridge
    let sandbox_config = SandboxConfig {
        timeout: Duration::from_secs(timeout_secs),
    };
    let runtime = Arc::new(LuaRuntime::with_mcp_bridge(sandbox_config, client_manager));

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