//! ZMQ SUB socket for receiving backend broadcasts
//!
//! Subscribes to backend PUB sockets and forwards Broadcast messages
//! to the SSE broadcast channel.

use anyhow::{Context, Result};
use hooteproto::Broadcast;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use zeromq::{Socket, SocketRecv, SubSocket};

/// Configuration for a PUB/SUB subscription
#[derive(Debug, Clone)]
pub struct SubscriberConfig {
    /// Name of the backend (for logging)
    pub name: String,
    /// ZMQ PUB endpoint to subscribe to
    pub endpoint: String,
}

/// Subscribe to a backend's PUB socket and forward broadcasts
pub async fn subscribe_to_backend(
    config: SubscriberConfig,
    broadcast_tx: broadcast::Sender<Broadcast>,
) -> Result<()> {
    let mut socket = SubSocket::new();

    // Subscribe to all messages (empty prefix)
    socket.subscribe("").await
        .context("Failed to set subscription")?;

    socket.connect(&config.endpoint).await
        .with_context(|| format!("Failed to connect SUB socket to {}", config.endpoint))?;

    info!("Subscribed to {} broadcasts at {}", config.name, config.endpoint);

    loop {
        match socket.recv().await {
            Ok(msg) => {
                if let Some(bytes) = msg.get(0) {
                    match std::str::from_utf8(bytes) {
                        Ok(json) => {
                            debug!("Received broadcast from {}: {}", config.name, json);
                            match serde_json::from_str::<Broadcast>(json) {
                                Ok(broadcast) => {
                                    // Forward to SSE clients
                                    if let Err(e) = broadcast_tx.send(broadcast) {
                                        debug!("No SSE clients connected: {}", e);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse broadcast from {}: {} - {}", config.name, e, json);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Invalid UTF-8 in broadcast from {}: {}", config.name, e);
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error receiving from {} SUB socket: {}", config.name, e);
                // Brief pause before retrying
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Spawn subscriber tasks for all configured backends
pub fn spawn_subscribers(
    broadcast_tx: broadcast::Sender<Broadcast>,
    luanette_pub: Option<String>,
    hootenanny_pub: Option<String>,
    chaosgarden_pub: Option<String>,
) {
    if let Some(endpoint) = luanette_pub {
        let tx = broadcast_tx.clone();
        tokio::spawn(async move {
            let config = SubscriberConfig {
                name: "luanette".to_string(),
                endpoint,
            };
            if let Err(e) = subscribe_to_backend(config, tx).await {
                error!("Luanette subscriber failed: {}", e);
            }
        });
    }

    if let Some(endpoint) = hootenanny_pub {
        let tx = broadcast_tx.clone();
        tokio::spawn(async move {
            let config = SubscriberConfig {
                name: "hootenanny".to_string(),
                endpoint,
            };
            if let Err(e) = subscribe_to_backend(config, tx).await {
                error!("Hootenanny subscriber failed: {}", e);
            }
        });
    }

    if let Some(endpoint) = chaosgarden_pub {
        let tx = broadcast_tx.clone();
        tokio::spawn(async move {
            let config = SubscriberConfig {
                name: "chaosgarden".to_string(),
                endpoint,
            };
            if let Err(e) = subscribe_to_backend(config, tx).await {
                error!("Chaosgarden subscriber failed: {}", e);
            }
        });
    }
}
