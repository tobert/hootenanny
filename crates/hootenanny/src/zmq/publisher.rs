//! ZMQ PUB socket for broadcasting events to holler
//!
//! Broadcasts Broadcast messages to subscribed clients (holler SUB sockets).

use anyhow::{Context, Result};
use hooteproto::Broadcast;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use zeromq::{PubSocket, Socket, SocketSend, ZmqMessage};

/// Handle for sending broadcasts
#[derive(Clone)]
pub struct BroadcastPublisher {
    tx: mpsc::Sender<Broadcast>,
}

impl BroadcastPublisher {
    /// Publish a broadcast message to all subscribers
    pub async fn publish(&self, broadcast: Broadcast) -> Result<()> {
        self.tx
            .send(broadcast)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send broadcast: {}", e))
    }

    /// Publish a job state change
    pub async fn job_state_changed(
        &self,
        job_id: &str,
        state: &str,
        result: Option<serde_json::Value>,
    ) -> Result<()> {
        self.publish(Broadcast::JobStateChanged {
            job_id: job_id.to_string(),
            state: state.to_string(),
            result,
        })
        .await
    }

    /// Publish an artifact creation event
    pub async fn artifact_created(
        &self,
        artifact_id: &str,
        content_hash: &str,
        tags: Vec<String>,
        creator: Option<String>,
    ) -> Result<()> {
        self.publish(Broadcast::ArtifactCreated {
            artifact_id: artifact_id.to_string(),
            content_hash: content_hash.to_string(),
            tags,
            creator,
        })
        .await
    }

    /// Publish a log message
    pub async fn log(&self, level: &str, message: &str, source: &str) -> Result<()> {
        self.publish(Broadcast::Log {
            level: level.to_string(),
            message: message.to_string(),
            source: source.to_string(),
        })
        .await
    }
}

/// ZMQ PUB socket server
pub struct PublisherServer {
    bind_address: String,
    rx: mpsc::Receiver<Broadcast>,
}

impl PublisherServer {
    /// Create a new publisher server and return the handle for sending broadcasts
    pub fn new(bind_address: String, buffer_size: usize) -> (Self, BroadcastPublisher) {
        let (tx, rx) = mpsc::channel(buffer_size);
        let server = Self { bind_address, rx };
        let publisher = BroadcastPublisher { tx };
        (server, publisher)
    }

    /// Run the publisher until the channel closes
    pub async fn run(mut self) -> Result<()> {
        let mut socket = PubSocket::new();
        socket
            .bind(&self.bind_address)
            .await
            .with_context(|| format!("Failed to bind PUB socket to {}", self.bind_address))?;

        info!("Hootenanny PUB socket listening on {}", self.bind_address);

        while let Some(broadcast) = self.rx.recv().await {
            match serde_json::to_string(&broadcast) {
                Ok(json) => {
                    debug!("Publishing broadcast: {}", json);
                    let msg = ZmqMessage::from(json.into_bytes());
                    if let Err(e) = socket.send(msg).await {
                        warn!("Failed to publish broadcast: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to serialize broadcast: {}", e);
                }
            }
        }

        info!("Publisher shutting down");
        Ok(())
    }
}
