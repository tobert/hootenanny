//! Vibeweaver binary - standalone Python kernel process

use anyhow::Result;
use clap::Parser;
use tracing::info;
use vibeweaver::{Kernel, Server, ServerConfig};

/// Vibeweaver - Python Kernel for AI Music Agents
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// ZMQ bind address for receiving requests from hootenanny
    #[arg(long, default_value = "tcp://0.0.0.0:5575")]
    bind: String,

    /// Hootenanny ZMQ endpoint for tool calls (unused for now)
    #[arg(long, default_value = "tcp://localhost:5580")]
    hootenanny: String,

    /// Hootenanny ZMQ PUB endpoint for broadcasts (unused for now)
    #[arg(long, default_value = "tcp://localhost:5581")]
    broadcasts: String,

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
    info!("Vibeweaver starting");
    info!("  name: {}", args.name);
    info!("  bind: {}", args.bind);
    info!("  hootenanny: {}", args.hootenanny);
    info!("  broadcasts: {}", args.broadcasts);
    info!("  db: {}", args.db);
    if let Some(ref session) = args.session {
        info!("  session: {}", session);
    }

    // Initialize Python kernel
    info!("Initializing Python kernel...");
    let kernel = Kernel::new()?;
    info!("  Python kernel ready");

    // Create server config
    let config = ServerConfig {
        bind_address: args.bind.clone(),
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
    let server = Server::new(config, kernel);
    server.run(shutdown_rx).await?;

    info!("Vibeweaver shutdown complete");
    Ok(())
}
