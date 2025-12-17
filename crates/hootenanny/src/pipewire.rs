//! PipeWire device event management
//!
//! Handles device hot-plug events from PipeWire, performs identity matching,
//! and broadcasts events to ZMQ subscribers.

use std::sync::Arc;

use audio_graph_mcp::{Database, DeviceFingerprint, IdentityMatcher, PipeWireSource};
use audio_graph_mcp::sources::DeviceEvent;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::zmq::BroadcastPublisher;

/// Manages device events from PipeWire, matches to identities, and broadcasts
pub struct DeviceEventManager {
    event_rx: mpsc::Receiver<DeviceEvent>,
    db: Arc<Database>,
    publisher: BroadcastPublisher,
}

impl DeviceEventManager {
    pub fn new(
        event_rx: mpsc::Receiver<DeviceEvent>,
        db: Arc<Database>,
        publisher: BroadcastPublisher,
    ) -> Self {
        Self {
            event_rx,
            db,
            publisher,
        }
    }

    /// Run the event manager, processing device events until the channel closes
    pub async fn run(mut self) {
        info!("🔌 DeviceEventManager started, watching for PipeWire device events");

        while let Some(event) = self.event_rx.recv().await {
            match event {
                DeviceEvent::NodeAdded(node) => {
                    // Extract fingerprints and try to match identity
                    let pw_source = PipeWireSource::new();
                    let fingerprints = pw_source.extract_fingerprints(&node);
                    let identity = self.match_identity(&fingerprints);

                    let identity_id = identity.as_ref().map(|(id, _)| id.as_str());
                    let identity_name = identity.as_ref().map(|(_, name)| name.as_str());

                    info!(
                        "🔌 Device connected: {} ({:?}) -> {:?}",
                        node.name,
                        node.media_class,
                        identity_name
                    );

                    // Broadcast to ZMQ subscribers
                    if let Err(e) = self
                        .publisher
                        .device_connected(
                            node.id,
                            &node.name,
                            node.media_class.as_deref(),
                            identity_id,
                            identity_name,
                        )
                        .await
                    {
                        warn!("Failed to broadcast device connected: {}", e);
                    }
                }

                DeviceEvent::NodeRemoved { id } => {
                    info!("🔌 Device disconnected: pipewire_id={}", id);

                    // Broadcast to ZMQ subscribers
                    if let Err(e) = self.publisher.device_disconnected(id, None).await {
                        warn!("Failed to broadcast device disconnected: {}", e);
                    }
                }

                DeviceEvent::PortAdded(port) => {
                    debug!(
                        "Port added: {} (node={}, direction={:?})",
                        port.name, port.node_id, port.direction
                    );
                }

                DeviceEvent::PortRemoved { id } => {
                    debug!("Port removed: id={}", id);
                }

                DeviceEvent::LinkAdded(link) => {
                    debug!(
                        "Link added: {} -> {} ({}:{})",
                        link.output_node_id,
                        link.input_node_id,
                        link.output_port_id,
                        link.input_port_id
                    );
                }

                DeviceEvent::LinkRemoved { id } => {
                    debug!("Link removed: id={}", id);
                }
            }
        }

        info!("DeviceEventManager shutting down");
    }

    /// Try to match device fingerprints to a known identity
    fn match_identity(&self, fingerprints: &[DeviceFingerprint]) -> Option<(String, String)> {
        let matcher = IdentityMatcher::new(&self.db);
        match matcher.best_match(fingerprints) {
            Ok(Some(result)) => Some((result.identity.id.0.clone(), result.identity.name.clone())),
            Ok(None) => None,
            Err(e) => {
                warn!("Identity matching error: {}", e);
                None
            }
        }
    }
}
