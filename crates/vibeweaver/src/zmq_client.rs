//! ZMQ client for hootenanny communication
//!
//! Uses hooteproto::HootClient for connection management with HOOT01 protocol.

use anyhow::Result;
use bytes::Bytes;
use futures::StreamExt;
use hooteproto::socket_config::{ZmqContext, Multipart};
use hooteproto::{broadcast_capnp, capnp_to_broadcast, Broadcast as HootBroadcast};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tmq::subscribe;

pub use hooteproto::{ClientConfig, HootClient, Payload};

/// Type alias for vibeweaver's hootenanny client
pub type ZmqClient = HootClient;

/// Create a client config for hootenanny connection
pub fn hootenanny_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    ClientConfig::new("hootenanny", endpoint).with_timeout(timeout_ms)
}

/// Connect to hootenanny (lazy - always succeeds, ZMQ connects when peer available)
pub async fn connect(endpoint: &str, timeout_ms: u64) -> Arc<ZmqClient> {
    let config = hootenanny_config(endpoint, timeout_ms);
    HootClient::new(config).await
}

/// Parsed broadcast message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Broadcast {
    JobStateChanged {
        job_id: String,
        state: String,
        artifact_id: Option<String>,
    },
    ArtifactCreated {
        artifact_id: String,
        content_hash: String,
        tags: Vec<String>,
    },
    TransportStateChanged {
        state: String,
        position_beats: f64,
    },
    BeatTick {
        beat: f64,
        tempo_bpm: f64,
    },
    MarkerReached {
        name: String,
        beat: f64,
    },
    Unknown {
        topic: String,
        data: Vec<u8>,
    },
}

/// Broadcast receiver (separate from client for ownership)
pub struct BroadcastReceiver {
    #[allow(dead_code)]
    context: ZmqContext,
    sub: Box<dyn futures::Stream<Item = Result<Multipart, tmq::TmqError>> + Unpin + Send>,
}

impl BroadcastReceiver {
    /// Connect SUB socket and subscribe to all relevant topics
    pub fn connect(endpoint: &str) -> Result<Self> {
        let context = ZmqContext::new();

        // Subscribe to all messages - filtering done on receive
        // tmq's subscribe() API returns Result<()>, so we can only subscribe once
        // during the builder chain. Subscribe to "" for all messages.
        let sub = subscribe(&context)
            .set_linger(0)
            .set_reconnect_ivl(1000)
            .set_reconnect_ivl_max(60000)
            .connect(endpoint)?
            .subscribe(b"")?;

        Ok(Self {
            context,
            sub: Box::new(sub),
        })
    }

    /// Receive next broadcast (blocking)
    ///
    /// Parses Cap'n Proto serialized broadcasts from hootenanny's PUB socket.
    pub async fn recv(&mut self) -> Result<Broadcast> {
        let mp = self.sub.next().await
            .ok_or_else(|| anyhow::anyhow!("Socket stream ended"))?
            .map_err(|e| anyhow::anyhow!("Failed to receive: {}", e))?;

        let frames: Vec<Bytes> = mp
            .into_iter()
            .map(|m| Bytes::from(m.to_vec()))
            .collect();

        if frames.is_empty() {
            anyhow::bail!("Empty broadcast message");
        }

        // Parse Cap'n Proto message (single frame containing serialized broadcast)
        let data = &frames[0];
        let reader = capnp::serialize::read_message_from_flat_slice(
            &mut data.as_ref(),
            capnp::message::ReaderOptions::default(),
        )?;

        let broadcast_reader = reader.get_root::<broadcast_capnp::broadcast::Reader>()?;
        let hoot_broadcast = capnp_to_broadcast(broadcast_reader)?;

        // Convert hooteproto::Broadcast to vibeweaver's simplified Broadcast
        Ok(hoot_broadcast_to_vibeweaver(hoot_broadcast))
    }
}

/// Convert hooteproto::Broadcast to vibeweaver's simplified Broadcast enum
fn hoot_broadcast_to_vibeweaver(broadcast: HootBroadcast) -> Broadcast {
    match broadcast {
        HootBroadcast::JobStateChanged { job_id, state, result } => {
            // Extract artifact_id from result JSON if present
            let artifact_id = result
                .as_ref()
                .and_then(|r| r.get("artifact_id"))
                .and_then(|v| v.as_str())
                .map(String::from);
            Broadcast::JobStateChanged {
                job_id,
                state,
                artifact_id,
            }
        }
        HootBroadcast::ArtifactCreated {
            artifact_id,
            content_hash,
            tags,
            ..
        } => Broadcast::ArtifactCreated {
            artifact_id,
            content_hash,
            tags,
        },
        HootBroadcast::TransportStateChanged {
            state,
            position_beats,
            ..
        } => Broadcast::TransportStateChanged {
            state,
            position_beats,
        },
        HootBroadcast::BeatTick {
            position_beats,
            tempo_bpm,
            ..
        } => Broadcast::BeatTick {
            beat: position_beats,
            tempo_bpm,
        },
        HootBroadcast::MarkerReached {
            position_beats,
            marker_type,
            ..
        } => Broadcast::MarkerReached {
            name: marker_type,
            beat: position_beats,
        },
        // All other broadcast types become Unknown
        other => Broadcast::Unknown {
            topic: format!("{:?}", other).split('{').next().unwrap_or("Unknown").trim().to_string(),
            data: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hoot_broadcast_to_vibeweaver_job() {
        let hoot = HootBroadcast::JobStateChanged {
            job_id: "job123".to_string(),
            state: "complete".to_string(),
            result: Some(serde_json::json!({"artifact_id": "art456"})),
        };

        let vibe = hoot_broadcast_to_vibeweaver(hoot);
        match vibe {
            Broadcast::JobStateChanged {
                job_id,
                state,
                artifact_id,
            } => {
                assert_eq!(job_id, "job123");
                assert_eq!(state, "complete");
                assert_eq!(artifact_id, Some("art456".to_string()));
            }
            _ => panic!("Wrong broadcast type"),
        }
    }

    #[test]
    fn test_hoot_broadcast_to_vibeweaver_beat() {
        let hoot = HootBroadcast::BeatTick {
            beat: 4,
            position_beats: 4.5,
            tempo_bpm: 130.0,
        };

        let vibe = hoot_broadcast_to_vibeweaver(hoot);
        match vibe {
            Broadcast::BeatTick { beat, tempo_bpm } => {
                assert_eq!(beat, 4.5);
                assert_eq!(tempo_bpm, 130.0);
            }
            _ => panic!("Wrong broadcast type"),
        }
    }
}
