//! ZMQ PUB socket for broadcasting events to holler
//!
//! Broadcasts Broadcast messages to subscribed clients (holler SUB sockets).
//! Messages are serialized using Cap'n Proto for cross-language compatibility.

use anyhow::{Context as AnyhowContext, Result};
use hooteproto::{broadcast_capnp, Broadcast};
use rzmq::{Context, Msg, SocketType};
use rzmq::socket::options::LINGER;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

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

    /// Publish a device connected event
    pub async fn device_connected(
        &self,
        pipewire_id: u32,
        name: &str,
        media_class: Option<&str>,
        identity_id: Option<&str>,
        identity_name: Option<&str>,
    ) -> Result<()> {
        self.publish(Broadcast::DeviceConnected {
            pipewire_id,
            name: name.to_string(),
            media_class: media_class.map(|s| s.to_string()),
            identity_id: identity_id.map(|s| s.to_string()),
            identity_name: identity_name.map(|s| s.to_string()),
        })
        .await
    }

    /// Publish a device disconnected event
    pub async fn device_disconnected(&self, pipewire_id: u32, name: Option<&str>) -> Result<()> {
        self.publish(Broadcast::DeviceDisconnected {
            pipewire_id,
            name: name.map(|s| s.to_string()),
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
        let context = Context::new()
            .with_context(|| "Failed to create ZMQ context")?;
        let socket = context
            .socket(SocketType::Pub)
            .with_context(|| "Failed to create PUB socket")?;

        // Set LINGER to 0 for immediate close
        if let Err(e) = socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await {
            warn!("Failed to set LINGER: {}", e);
        }

        socket
            .bind(&self.bind_address)
            .await
            .with_context(|| format!("Failed to bind PUB socket to {}", self.bind_address))?;

        info!("Hootenanny PUB socket listening on {}", self.bind_address);

        while let Some(broadcast) = self.rx.recv().await {
            // Serialize to Cap'n Proto
            let mut message = capnp::message::Builder::new_default();
            {
                let mut builder = message.init_root::<broadcast_capnp::broadcast::Builder>();
                if let Err(e) = broadcast_to_capnp(&broadcast, &mut builder) {
                    error!("Failed to serialize broadcast to capnp: {}", e);
                    continue;
                }
            }

            // Write to bytes
            let bytes = capnp::serialize::write_message_to_words(&message);
            debug!(
                "Publishing broadcast: {:?}",
                broadcast_variant_name(&broadcast)
            );
            let msg = Msg::from_vec(bytes);
            if let Err(e) = socket.send(msg).await {
                warn!("Failed to publish broadcast: {}", e);
            }
        }

        info!("Publisher shutting down");
        Ok(())
    }
}

/// Convert Broadcast enum to Cap'n Proto builder
fn broadcast_to_capnp(
    broadcast: &Broadcast,
    builder: &mut broadcast_capnp::broadcast::Builder,
) -> Result<()> {
    match broadcast {
        Broadcast::ConfigUpdate { key, value } => {
            let mut update = builder.reborrow().init_config_update();
            update.set_key(key);
            update.set_value(&serde_json::to_string(value)?);
        }
        Broadcast::Shutdown { reason } => {
            let mut shutdown = builder.reborrow().init_shutdown();
            shutdown.set_reason(reason);
        }
        Broadcast::ScriptInvalidate { hash } => {
            let mut invalidate = builder.reborrow().init_script_invalidate();
            invalidate.set_hash(hash);
        }
        Broadcast::JobStateChanged {
            job_id,
            state,
            result,
        } => {
            let mut job = builder.reborrow().init_job_state_changed();
            job.set_job_id(job_id);
            job.set_state(state);
            if let Some(ref res) = result {
                job.set_result(&serde_json::to_string(res)?);
            } else {
                job.set_result("");
            }
        }
        Broadcast::Progress {
            job_id,
            percent,
            message,
        } => {
            let mut prog = builder.reborrow().init_progress();
            prog.set_job_id(job_id);
            prog.set_percent(*percent);
            prog.set_message(message);
        }
        Broadcast::ArtifactCreated {
            artifact_id,
            content_hash,
            tags,
            creator,
        } => {
            let mut artifact = builder.reborrow().init_artifact_created();
            artifact.set_artifact_id(artifact_id);
            artifact.set_content_hash(content_hash);

            let mut tag_list = artifact.reborrow().init_tags(tags.len() as u32);
            for (i, tag) in tags.iter().enumerate() {
                tag_list.reborrow().set(i as u32, tag);
            }

            artifact.set_creator(creator.as_deref().unwrap_or(""));
        }
        Broadcast::TransportStateChanged {
            state,
            position_beats,
            tempo_bpm,
        } => {
            let mut transport = builder.reborrow().init_transport_state_changed();
            transport.set_state(state);
            transport.set_position_beats(*position_beats);
            transport.set_tempo_bpm(*tempo_bpm);
        }
        Broadcast::MarkerReached {
            position_beats,
            marker_type,
            metadata,
        } => {
            let mut marker = builder.reborrow().init_marker_reached();
            marker.set_position_beats(*position_beats);
            marker.set_marker_type(marker_type);
            marker.set_metadata(&serde_json::to_string(metadata)?);
        }
        Broadcast::BeatTick {
            beat,
            position_beats,
            tempo_bpm,
        } => {
            let mut tick = builder.reborrow().init_beat_tick();

            // Set timestamp to current time
            let mut ts = tick.reborrow().init_timestamp();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64;
            ts.set_nanos(now);

            tick.set_beat(*beat);
            tick.set_position_beats(*position_beats);
            tick.set_tempo_bpm(*tempo_bpm);
        }
        Broadcast::Log {
            level,
            message,
            source,
        } => {
            let mut log = builder.reborrow().init_log();
            log.set_level(level);
            log.set_message(message);
            log.set_source(source);
        }
        Broadcast::DeviceConnected {
            pipewire_id,
            name,
            media_class,
            identity_id,
            identity_name,
        } => {
            let mut device = builder.reborrow().init_device_connected();
            device.set_pipewire_id(*pipewire_id);
            device.set_name(name);
            device.set_media_class(media_class.as_deref().unwrap_or(""));
            device.set_identity_id(identity_id.as_deref().unwrap_or(""));
            device.set_identity_name(identity_name.as_deref().unwrap_or(""));
        }
        Broadcast::DeviceDisconnected { pipewire_id, name } => {
            let mut device = builder.reborrow().init_device_disconnected();
            device.set_pipewire_id(*pipewire_id);
            device.set_name(name.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

/// Get the variant name for logging
fn broadcast_variant_name(broadcast: &Broadcast) -> &'static str {
    match broadcast {
        Broadcast::ConfigUpdate { .. } => "ConfigUpdate",
        Broadcast::Shutdown { .. } => "Shutdown",
        Broadcast::ScriptInvalidate { .. } => "ScriptInvalidate",
        Broadcast::JobStateChanged { .. } => "JobStateChanged",
        Broadcast::Progress { .. } => "Progress",
        Broadcast::ArtifactCreated { .. } => "ArtifactCreated",
        Broadcast::TransportStateChanged { .. } => "TransportStateChanged",
        Broadcast::MarkerReached { .. } => "MarkerReached",
        Broadcast::BeatTick { .. } => "BeatTick",
        Broadcast::Log { .. } => "Log",
        Broadcast::DeviceConnected { .. } => "DeviceConnected",
        Broadcast::DeviceDisconnected { .. } => "DeviceDisconnected",
    }
}
