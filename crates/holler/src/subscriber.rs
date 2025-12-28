//! ZMQ SUB socket for receiving backend broadcasts
//!
//! Subscribes to backend PUB sockets and forwards Broadcast messages
//! to the SSE broadcast channel.

use anyhow::{Context as AnyhowContext, Result};
use futures::StreamExt;
use hooteproto::socket_config::{create_subscriber_and_connect, ZmqContext};
use hooteproto::{broadcast_capnp, capnp_to_broadcast, Broadcast};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

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
    let context = ZmqContext::new();
    let mut socket =
        create_subscriber_and_connect(&context, &config.endpoint, &config.name)?;

    info!(
        "Subscribed to {} broadcasts at {}",
        config.name, config.endpoint
    );

    loop {
        match socket.next().await {
            Some(Ok(multipart)) => {
                // The multipart message should have one frame: the Cap'n Proto broadcast
                for msg in multipart {
                    let bytes: &[u8] = msg.as_ref();
                    if bytes.is_empty() {
                        continue;
                    }

                    // Parse Cap'n Proto broadcast
                    match parse_capnp_broadcast(bytes) {
                        Ok(broadcast) => {
                            debug!("Received {} broadcast: {:?}", config.name, broadcast);
                            if let Err(e) = broadcast_tx.send(broadcast) {
                                debug!("No SSE clients connected: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to parse broadcast from {}: {} ({} bytes)",
                                config.name,
                                e,
                                bytes.len()
                            );
                        }
                    }
                }
            }
            Some(Err(e)) => {
                error!("Error receiving from {} SUB socket: {}", config.name, e);
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            None => {
                warn!("SUB socket stream ended for {}", config.name);
                break;
            }
        }
    }

    Ok(())
}

/// Parse Cap'n Proto broadcast bytes into Broadcast enum
fn parse_capnp_broadcast(bytes: &[u8]) -> Result<Broadcast> {
    let words = capnp::serialize::read_message_from_flat_slice(
        &mut bytes.as_ref(),
        capnp::message::ReaderOptions::default(),
    )
    .context("Failed to read Cap'n Proto message")?;

    let reader = words
        .get_root::<broadcast_capnp::broadcast::Reader>()
        .context("Failed to get broadcast root")?;

    capnp_to_broadcast(reader).context("Failed to convert Cap'n Proto to Broadcast")
}

/// Spawn subscriber tasks for all configured backends
pub fn spawn_subscribers(
    broadcast_tx: broadcast::Sender<Broadcast>,
    hootenanny_pub: Option<String>,
    chaosgarden_pub: Option<String>,
) {
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
