//! Vibeweaver binary - standalone Python kernel process

use anyhow::{Context, Result};
use clap::Parser;
use hooteconf::HootConfig;
use std::path::PathBuf;
use tokio::runtime::Handle;
use tracing::info;
use vibeweaver::{tool_bridge::ToolBridge, zmq_client, Kernel, Server, ServerConfig};

/// Vibeweaver - Python Kernel for AI Music Agents
///
/// Configuration is loaded from (in order, later wins):
/// 1. Compiled defaults
/// 2. /etc/hootenanny/config.toml
/// 3. ~/.config/hootenanny/config.toml
/// 4. ./hootenanny.toml (or --config path)
/// 5. Environment variables (HOOTENANNY_*)
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to config file (overrides ./hootenanny.toml)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Show loaded configuration and exit
    #[arg(long)]
    show_config: bool,

    /// Database path
    #[arg(long, default_value = "~/.hootenanny/vibeweaver.db")]
    db: String,

    /// Session name (creates new or loads existing)
    #[arg(long)]
    session: Option<String>,

    /// Worker name for identification
    #[arg(long, default_value = "vibeweaver")]
    name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Load configuration from files + env
    let (config, sources) = HootConfig::load_with_sources_from(args.config.as_deref())
        .context("Failed to load configuration")?;

    // Show config and exit if requested
    if args.show_config {
        println!("# Configuration sources:");
        for path in &sources.files {
            println!("#   - {}", path.display());
        }
        if !sources.env_overrides.is_empty() {
            println!("# Environment overrides:");
            for var in &sources.env_overrides {
                println!("#   - {}", var);
            }
        }
        println!();
        println!("{}", config.to_toml());
        return Ok(());
    }

    // Get vibeweaver-specific config
    let vibeweaver_config = &config.infra.services.vibeweaver;

    info!("Vibeweaver starting");
    info!("  name: {}", args.name);
    info!("  bind: {}", vibeweaver_config.zmq_router);
    info!("  hootenanny: {}", vibeweaver_config.hootenanny);
    info!("  broadcasts: {}", vibeweaver_config.hootenanny_pub);
    info!("  db: {}", args.db);
    if let Some(ref session) = args.session {
        info!("  session: {}", session);
    }

    // Connect to hootenanny for tool calls (lazy - ZMQ connects when peer available)
    info!("Connecting to hootenanny for tool calls (lazy)...");
    let zmq_client = zmq_client::connect(
        &vibeweaver_config.hootenanny,
        vibeweaver_config.timeout_ms,
    )
    .await;
    info!("  Configured hootenanny connection at {}", vibeweaver_config.hootenanny);

    // Initialize tool bridge (makes tools available to Python API)
    let bridge = ToolBridge::new(zmq_client, Handle::current());
    ToolBridge::init_global(bridge)?;
    info!("  Tool bridge initialized");

    // Initialize Python kernel
    info!("Initializing Python kernel...");
    let kernel = Kernel::new()?;
    info!("  Python kernel ready");

    // Create server config
    let server_config = ServerConfig {
        bind_address: vibeweaver_config.zmq_router.clone(),
        worker_name: args.name.clone(),
    };

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Handle SIGINT/SIGTERM
    let shutdown_tx_signal = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received shutdown signal");
        let _ = shutdown_tx_signal.send(());
    });

    // Run the server
    let server = Server::new(server_config, kernel);
    server.run(shutdown_rx).await?;

    info!("Vibeweaver shutdown complete");
    Ok(())
}
