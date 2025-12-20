//! ZMQ client for hootenanny communication

use anyhow::{Context, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, SubSocket, ZmqMessage};

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

/// ZMQ client for request/reply to hootenanny
pub struct ZmqClient {
    dealer: DealerSocket,
    identity: String,
}

impl ZmqClient {
    /// Connect DEALER socket to hootenanny router
    pub async fn connect(endpoint: &str, identity: &str) -> Result<Self> {
        let mut dealer = DealerSocket::new();
        dealer
            .connect(endpoint)
            .await
            .with_context(|| format!("Failed to connect DEALER to {}", endpoint))?;

        Ok(Self {
            dealer,
            identity: identity.to_string(),
        })
    }

    /// Send tool call request, await response
    pub async fn call_tool(
        &mut self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        // Build request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": args
            },
            "id": uuid::Uuid::new_v4().to_string()
        });

        let request_bytes = serde_json::to_vec(&request)?;

        // Send
        let msg = ZmqMessage::from(request_bytes);
        self.dealer.send(msg).await?;

        // Receive response
        let response_msg = self.dealer.recv().await?;
        let response_bytes = response_msg.into_vec();

        // Combine frames if multiple
        let data: Vec<u8> = response_bytes
            .into_iter()
            .flat_map(|b| b.to_vec())
            .collect();

        let response: serde_json::Value = serde_json::from_slice(&data)?;

        // Extract result
        if let Some(result) = response.get("result") {
            Ok(result.clone())
        } else if let Some(error) = response.get("error") {
            anyhow::bail!("Tool call error: {}", error)
        } else {
            Ok(response)
        }
    }

    /// Identity for this client
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// Broadcast receiver (separate from client for ownership)
pub struct BroadcastReceiver {
    sub: SubSocket,
}

impl BroadcastReceiver {
    /// Connect SUB socket and subscribe to all relevant topics
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let mut sub = SubSocket::new();
        sub.connect(endpoint)
            .await
            .with_context(|| format!("Failed to connect SUB to {}", endpoint))?;

        // Subscribe to relevant topics
        sub.subscribe("job.").await?;
        sub.subscribe("artifact.").await?;
        sub.subscribe("transport.").await?;
        sub.subscribe("beat.").await?;
        sub.subscribe("marker.").await?;

        Ok(Self { sub })
    }

    /// Subscribe to specific topic prefix
    pub async fn subscribe(&mut self, topic: &str) -> Result<()> {
        self.sub.subscribe(topic).await?;
        Ok(())
    }

    /// Receive next broadcast (blocking)
    pub async fn recv(&mut self) -> Result<Broadcast> {
        let msg = self.sub.recv().await?;
        let frames: Vec<Bytes> = msg.into_vec();

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
