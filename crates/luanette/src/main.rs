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
use cas::{CasConfig, FileStore};
use clap::{Parser, Subcommand};
use dispatch::Dispatcher;
use job_system::JobStore;
use runtime::{LuaRuntime, SandboxConfig};
use std::sync::Arc;
use std::time::Duration;
use zmq_server::{Server, ServerConfig};
use clients::{ClientManager, UpstreamConfig};

/// Parse a key=value parameter
fn parse_param(s: &str) -> Result<(String, String), String> {
    let pos = s.find('=').ok_or_else(|| format!("invalid param, expected key=value: {}", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

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

    /// Run a Lua script directly (for testing)
    Run {
        /// Path to Lua script file
        script: String,

        /// Hootenanny ZMQ endpoint for tool calls
        #[arg(long, default_value = "tcp://localhost:5580")]
        hootenanny: String,

        /// Script parameters as key=value pairs
        #[arg(short, long, value_parser = parse_param)]
        param: Vec<(String, String)>,
    },

    /// Evaluate Lua code directly
    Eval {
        /// Lua code to evaluate
        code: String,
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
        Commands::Run { script, hootenanny, param } => {
            run_script(&script, cli.timeout, &hootenanny, param).await?;
        }
        Commands::Eval { code } => {
            eval_code(&code, cli.timeout).await?;
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

    // Create CAS client
    let cas_config = CasConfig::from_env().context("Failed to load CAS configuration")?;
    let cas = Arc::new(FileStore::new(cas_config).context("Failed to create CAS client")?);
    tracing::info!("CAS initialized at {:?}", cas.config().base_path);

    // Create dispatcher
    let dispatcher = Dispatcher::new(runtime, job_store, cas);

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

/// Run a Lua script file directly (for testing)
async fn run_script(path: &str, timeout_secs: u64, hootenanny_endpoint: &str, params: Vec<(String, String)>) -> Result<()> {
    use std::fs;

    println!("🌙 Running script: {}", path);

    // Read script file
    let script = fs::read_to_string(path).context("Failed to read script file")?;

    // Create client manager and connect to hootenanny
    let mut client_manager = ClientManager::new();
    client_manager
        .add_upstream(UpstreamConfig {
            namespace: "hootenanny".to_string(),
            endpoint: hootenanny_endpoint.to_string(),
            timeout_ms: timeout_secs * 1000,
        })
        .await
        .context("Failed to connect to hootenanny")?;

    println!("   Connected to hootenanny at {}", hootenanny_endpoint);

    let client_manager = Arc::new(client_manager);

    // Create Lua runtime WITH MCP bridge
    let sandbox_config = SandboxConfig {
        timeout: Duration::from_secs(timeout_secs),
    };
    let runtime = LuaRuntime::with_mcp_bridge(sandbox_config, client_manager);

    // Build params as JSON
    let mut params_json = serde_json::Map::new();
    for (key, value) in params {
        // Try to parse as number first
        if let Ok(n) = value.parse::<f64>() {
            params_json.insert(key, serde_json::json!(n));
        } else {
            params_json.insert(key, serde_json::json!(value));
        }
    }

    // Execute script using async execute() method
    let result = runtime.execute(&script, serde_json::Value::Object(params_json)).await?;

    // Print result
    match &result.result {
        serde_json::Value::Object(obj) => {
            println!("\n📦 Result:");
            for (k, v) in obj {
                println!("   {} = {}", k, v);
            }
        }
        serde_json::Value::Null => {
            println!("\n✅ Script completed (no return value)");
        }
        other => {
            println!("\n📦 Result: {}", other);
        }
    }

    println!("   Duration: {:?}", result.duration);

    Ok(())
}

/// Evaluate Lua code directly (simple REPL mode)
async fn eval_code(code: &str, timeout_secs: u64) -> Result<()> {
    // Create Lua runtime WITHOUT MCP bridge (for quick eval)
    let sandbox_config = SandboxConfig {
        timeout: Duration::from_secs(timeout_secs),
    };
    let runtime = LuaRuntime::new(sandbox_config);

    let result = runtime.eval(code).await?;

    match &result.result {
        serde_json::Value::Null => {}
        other => println!("{}", other),
    }

    Ok(())
}