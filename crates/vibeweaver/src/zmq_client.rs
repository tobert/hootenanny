//! ZMQ client for hootenanny communication
//!
//! Uses hooteproto::HootClient for connection management with HOOT01 protocol.

use anyhow::Result;
use bytes::Bytes;
use futures::StreamExt;
use hooteproto::socket_config::{ZmqContext, Multipart};
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

        // First frame is topic
        let topic = String::from_utf8_lossy(&frames[0]).to_string();

        // Second frame (if present) is data
        let data = if frames.len() > 1 {
            frames[1].to_vec()
        } else {
            vec![]
        };

        parse_broadcast(&topic, &data)
    }
}

/// Parse raw broadcast into typed Broadcast
fn parse_broadcast(topic: &str, data: &[u8]) -> Result<Broadcast> {
    // Try to parse data as JSON
    let json: serde_json::Value = if data.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(data).unwrap_or(serde_json::Value::Null)
    };

    match topic.split('.').next() {
        Some("job") => {
            let job_id = json["job_id"].as_str().unwrap_or("").to_string();
            let state = json["state"].as_str().unwrap_or("unknown").to_string();
            let artifact_id = json["artifact_id"].as_str().map(String::from);
            Ok(Broadcast::JobStateChanged {
                job_id,
                state,
                artifact_id,
            })
        }
        Some("artifact") => {
            let artifact_id = json["artifact_id"].as_str().unwrap_or("").to_string();
            let content_hash = json["content_hash"].as_str().unwrap_or("").to_string();
            let tags = json["tags"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Ok(Broadcast::ArtifactCreated {
                artifact_id,
                content_hash,
                tags,
            })
        }
        Some("transport") => {
            let state = json["state"].as_str().unwrap_or("stopped").to_string();
            let position_beats = json["position_beats"].as_f64().unwrap_or(0.0);
            Ok(Broadcast::TransportStateChanged {
                state,
                position_beats,
            })
        }
        Some("beat") => {
            let beat = json["beat"].as_f64().unwrap_or(0.0);
            let tempo_bpm = json["tempo_bpm"].as_f64().unwrap_or(120.0);
            Ok(Broadcast::BeatTick { beat, tempo_bpm })
        }
        Some("marker") => {
            let name = json["name"].as_str().unwrap_or("").to_string();
            let beat = json["beat"].as_f64().unwrap_or(0.0);
            Ok(Broadcast::MarkerReached { name, beat })
        }
        _ => Ok(Broadcast::Unknown {
            topic: topic.to_string(),
            data: data.to_vec(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_job_broadcast() {
        let data = br#"{"job_id": "abc123", "state": "complete", "artifact_id": "art456"}"#;
        let broadcast = parse_broadcast("job.state_changed", data).unwrap();

        match broadcast {
            Broadcast::JobStateChanged {
                job_id,
                state,
                artifact_id,
            } => {
                assert_eq!(job_id, "abc123");
                assert_eq!(state, "complete");
                assert_eq!(artifact_id, Some("art456".to_string()));
            }
            _ => panic!("Wrong broadcast type"),
        }
    }

    #[test]
    fn test_parse_beat_broadcast() {
        let data = br#"{"beat": 4.0, "tempo_bpm": 130.0}"#;
        let broadcast = parse_broadcast("beat.tick", data).unwrap();

        match broadcast {
            Broadcast::BeatTick { beat, tempo_bpm } => {
                assert_eq!(beat, 4.0);
                assert_eq!(tempo_bpm, 130.0);
            }
            _ => panic!("Wrong broadcast type"),
        }
    }
}
