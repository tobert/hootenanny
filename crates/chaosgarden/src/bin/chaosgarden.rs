//! Chaosgarden daemon binary
//!
//! Realtime audio daemon that communicates with hootenanny via ZMQ.
//!
//! Uses GardenDaemon for real state management:
//! - Transport control (play/pause/stop/seek)
//! - Timeline with regions
//! - Trustfall queries over graph state
//! - Latent region lifecycle
//!
//! A background tick loop advances the transport position based on wall time.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chaosgarden::{GardenDaemon, DaemonConfig};
use chaosgarden::ipc::{capnp_server::CapnpGardenServer, GardenEndpoints};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("chaosgarden {} starting (Cap'n Proto)", env!("CARGO_PKG_VERSION"));

    let endpoints = GardenEndpoints::local();
    info!("binding to endpoints: {:?}", endpoints);

    let server = CapnpGardenServer::new(endpoints);

    // Create real daemon with state management
    let config = DaemonConfig::default();
    let handler = Arc::new(GardenDaemon::with_config(config));
    info!("GardenDaemon initialized");

    // Spawn tick loop to advance position based on wall time
    let tick_handler = Arc::clone(&handler);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1));
        loop {
            interval.tick().await;
            tick_handler.tick();
        }
    });
    info!("Tick loop started (1ms interval)");

    server.run(handler).await?;

    info!("chaosgarden shutdown complete");
    Ok(())
}
