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
use chaosgarden::ipc::capnp_server::CapnpGardenServer;
use chaosgarden::nodes::FileCasClient;
use hooteconf::HootConfig;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("chaosgarden {} starting (Cap'n Proto)", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let hoote_config = HootConfig::load()?;
    info!("Using socket_dir: {:?}", hoote_config.infra.paths.socket_dir);

    let server = CapnpGardenServer::new(hoote_config.clone());

    // Create real daemon with state management
    let daemon_config = DaemonConfig::default();
    let mut daemon = GardenDaemon::with_config(daemon_config);

    // Initialize content resolver for timeline playback (loads audio from CAS)
    let cas_path = hoote_config.infra.paths.cas_dir.to_string_lossy().to_string();
    match FileCasClient::new(&cas_path) {
        Ok(client) => {
            daemon.set_content_resolver(Arc::new(client));
            info!("Content resolver initialized: {}", cas_path);
        }
        Err(e) => {
            info!("Warning: Could not initialize CAS at {}: {} (timeline playback disabled)", cas_path, e);
        }
    }

    let handler = Arc::new(daemon);
    info!("GardenDaemon initialized");

    // Spawn tick loop to advance position based on wall time
    // Tick interval matches buffer processing time: 256 samples at 48kHz = 5.33ms
    // Use 5ms for a slight margin to avoid ring buffer overflow
    let tick_handler = Arc::clone(&handler);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(5));
        loop {
            interval.tick().await;
            tick_handler.tick();
        }
    });
    info!("Tick loop started (5ms interval, matches 256-sample buffer at 48kHz)");

    server.run(handler).await?;

    info!("chaosgarden shutdown complete");
    Ok(())
}
