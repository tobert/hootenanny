//! hooteproto - Protocol types for the Hootenanny ZMQ message bus
//!
//! This crate defines the message types exchanged between Hootenanny services
//! over ZMQ. All messages are wrapped in an Envelope for tracing and routing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Envelope wraps all ZMQ messages with routing and tracing metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope {
    /// Unique message ID for correlation
    pub id: Uuid,
    /// W3C traceparent for distributed tracing
    pub traceparent: Option<String>,
    /// The actual message payload
    pub payload: Payload,
}

impl Envelope {
    pub fn new(payload: Payload) -> Self {
        Self {
            id: Uuid::new_v4(),
            traceparent: None,
            payload,
        }
    }

    pub fn with_traceparent(mut self, traceparent: impl Into<String>) -> Self {
        self.traceparent = Some(traceparent.into());
        self
    }
}

/// All message types in the Hootenanny system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Payload {
    // === Worker Management ===
    Register(WorkerRegistration),
    Ping,
    Pong {
        worker_id: Uuid,
        uptime_secs: u64,
    },
    Shutdown {
        reason: String,
    },

    // === Lua Tools (Holler → Luanette) ===
    LuaEval {
        code: String,
        params: Option<serde_json::Value>,
    },
    LuaDescribe {
        script_hash: String,
    },
    ScriptStore {
        content: String,
        tags: Option<Vec<String>>,
        creator: Option<String>,
    },
    ScriptSearch {
        tag: Option<String>,
        creator: Option<String>,
        vibe: Option<String>,
    },

    // === Job System (any → Luanette) ===
    JobExecute {
        script_hash: String,
        params: serde_json::Value,
        tags: Option<Vec<String>>,
    },
    JobStatus {
        job_id: String,
    },
    JobPoll {
        job_ids: Vec<String>,
        timeout_ms: u64,
        mode: PollMode,
    },
    JobCancel {
        job_id: String,
    },
    JobList {
        status: Option<String>,
    },

    // === MCP Resources ===
    ReadResource {
        uri: String,
    },
    ListResources,

    // === MCP Prompts ===
    GetPrompt {
        name: String,
        arguments: HashMap<String, String>,
    },
    ListPrompts,

    // === MCP Completions ===
    Complete {
        context: String,
        partial: String,
    },

    // === Chaosgarden Events (Chaosgarden → Luanette) ===
    TimelineEvent {
        event_type: TimelineEventType,
        position_beats: f64,
        tempo: f64,
        metadata: serde_json::Value,
    },

    // === CAS Tools (Holler → Hootenanny) ===
    CasStore {
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
        mime_type: Option<String>,
    },
    CasInspect {
        hash: String,
    },
    CasGet {
        hash: String,
    },

    // === Artifact Tools (Holler → Hootenanny) ===
    ArtifactGet {
        id: String,
    },
    ArtifactList {
        tag: Option<String>,
        creator: Option<String>,
    },
    ArtifactCreate {
        cas_hash: String,
        tags: Vec<String>,
        creator: Option<String>,
        metadata: serde_json::Value,
    },

    // === Graph Tools (Holler → Hootenanny) ===
    GraphQuery {
        query: String,
        variables: serde_json::Value,
    },
    GraphBind {
        identity: String,
        hints: Vec<String>,
    },

    // === Transport Tools (Holler → Chaosgarden) ===
    TransportPlay,
    TransportStop,
    TransportSeek {
        position_beats: f64,
    },
    TransportStatus,

    // === Timeline Tools (Holler → Chaosgarden) ===
    TimelineQuery {
        from_beats: Option<f64>,
        to_beats: Option<f64>,
    },
    TimelineAddMarker {
        position_beats: f64,
        marker_type: String,
        metadata: serde_json::Value,
    },

    // === Tool Discovery (Holler → any backend) ===
    ListTools,
    ToolList {
        tools: Vec<ToolInfo>,
    },

    // === Responses ===
    Success {
        result: serde_json::Value,
    },
    Error {
        code: String,
        message: String,
        details: Option<serde_json::Value>,
    },
}

/// Worker registration announcement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerRegistration {
    pub worker_id: Uuid,
    pub worker_type: WorkerType,
    pub worker_name: String,
    pub capabilities: Vec<String>,
    pub hostname: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerType {
    Luanette,
    Hootenanny,
    Chaosgarden,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PollMode {
    Any,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimelineEventType {
    SectionChange,
    BeatMarker,
    CuePoint,
    GenerateTransition,
}

/// MCP tool information for discovery
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Broadcast messages via PUB/SUB
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Broadcast {
    ConfigUpdate {
        key: String,
        value: serde_json::Value,
    },
    Shutdown {
        reason: String,
    },
    ScriptInvalidate {
        hash: String,
    },
}

/// Base64 encoding for binary data in JSON
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        STANDARD.encode(bytes).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn envelope_roundtrip() {
        let envelope = Envelope::new(Payload::Ping);
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn lua_eval_roundtrip() {
        let envelope = Envelope::new(Payload::LuaEval {
            code: "return 1 + 1".to_string(),
            params: Some(serde_json::json!({"x": 42})),
        });
        let json = serde_json::to_string_pretty(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn success_response_roundtrip() {
        let envelope = Envelope::new(Payload::Success {
            result: serde_json::json!({"answer": 42}),
        });
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn error_response_roundtrip() {
        let envelope = Envelope::new(Payload::Error {
            code: "lua_error".to_string(),
            message: "syntax error near 'end'".to_string(),
            details: Some(serde_json::json!({"line": 42})),
        });
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn cas_store_with_binary_data() {
        let envelope = Envelope::new(Payload::CasStore {
            data: vec![0x4d, 0x54, 0x68, 0x64], // MIDI header
            mime_type: Some("audio/midi".to_string()),
        });
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("TVRoZA==")); // base64 of MThd
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn worker_registration_roundtrip() {
        let reg = WorkerRegistration {
            worker_id: Uuid::new_v4(),
            worker_type: WorkerType::Luanette,
            worker_name: "lua-orchestrator".to_string(),
            capabilities: vec!["lua".to_string(), "orpheus".to_string()],
            hostname: "localhost".to_string(),
            version: "0.1.0".to_string(),
        };
        let envelope = Envelope::new(Payload::Register(reg.clone()));
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        if let Payload::Register(parsed_reg) = parsed.payload {
            assert_eq!(reg.worker_name, parsed_reg.worker_name);
            assert_eq!(reg.capabilities, parsed_reg.capabilities);
        } else {
            panic!("Expected Register payload");
        }
    }

    #[test]
    fn broadcast_roundtrip() {
        let broadcast = Broadcast::Shutdown {
            reason: "maintenance".to_string(),
        };
        let json = serde_json::to_string(&broadcast).unwrap();
        let parsed: Broadcast = serde_json::from_str(&json).unwrap();
        assert_eq!(broadcast, parsed);
    }

    #[test]
    fn tool_list_roundtrip() {
        let envelope = Envelope::new(Payload::ToolList {
            tools: vec![
                ToolInfo {
                    name: "lua_eval".to_string(),
                    description: "Evaluate Lua code".to_string(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "code": {"type": "string"}
                        }
                    }),
                },
            ],
        });
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn timeline_event_roundtrip() {
        let envelope = Envelope::new(Payload::TimelineEvent {
            event_type: TimelineEventType::GenerateTransition,
            position_beats: 32.0,
            tempo: 120.0,
            metadata: serde_json::json!({
                "current_section": "cas:abc123",
                "next_section": "cas:def456"
            }),
        });
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }
}
