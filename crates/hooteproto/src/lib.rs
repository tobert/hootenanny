//! hooteproto - Protocol types for the Hootenanny ZMQ message bus
//!
//! This crate defines the message types exchanged between Hootenanny services
//! over ZMQ. All messages are wrapped in an Envelope for tracing and routing.
//!
//! ## MCP Alignment
//!
//! hooteproto is **aligned to MCP** but specialized for our internal workflows:
//! - Tool naming follows MCP conventions (snake_case, domain prefixes)
//! - Request/response patterns match MCP tool call semantics
//! - Resources and completions follow MCP patterns
//!
//! However, hooteproto is **loosely coupled** to MCP:
//! - Internal transport uses Cap'n Proto over ZMQ (not JSON-RPC)
//! - Typed dispatch avoids JSON in the core path
//! - Timing semantics (sync/async/fire-and-forget) are richer than MCP
//!
//! The `holler` crate **exposes hooteproto over MCP** - it's the bridge between
//! the MCP world (Claude, other LLM tools) and the Hootenanny internal protocol.
//!
//! ## HOOT01 Frame Protocol
//!
//! The `frame` module implements the HOOT01 wire protocol - a hybrid frame-based
//! format inspired by MDP (Majordomo Protocol). This enables:
//! - Routing without deserialization (fixed-width routing fields)
//! - Efficient heartbeats (minimal overhead)
//! - Native binary payloads (no base64 encoding)
//!
//! ## Domain Types
//!
//! Domain types are defined in Cap'n Proto schemas for cross-language compatibility:
//! - `JobId` - Rust newtype wrapper for type safety (Text on wire)
//! - `JobStatus` - Enum defined in common.capnp
//! - `JobInfo` - Struct with Rust ergonomics, backed by jobs.capnp
//! - `JobStoreStats` - Direct capnp type re-export
//!
//! See the `domain` module for Rust wrappers and helpers.
//!
//! ## Python/Lua Clients
//!
//! Non-Rust clients use the generated Cap'n Proto types directly:
//! ```python
//! import capnp
//! common = capnp.load('hooteproto/schemas/common.capnp')
//!
//! # Use generated enums/structs
//! status = common.JobStatus.running
//! ```
//!
//! ## Tool Parameter Types
//!
//! The `params` module contains types with JsonSchema derives for MCP compatibility.
//! Use with `baton::schema_for::<ParamType>()` to generate tool input schemas.

// Cap'n Proto generated modules (must be at crate root for cross-references)
#[allow(clippy::all)]
#[allow(dead_code)]
pub mod common_capnp {
    include!(concat!(env!("OUT_DIR"), "/common_capnp.rs"));
}

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod jobs_capnp {
    include!(concat!(env!("OUT_DIR"), "/jobs_capnp.rs"));
}

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod tools_capnp {
    include!(concat!(env!("OUT_DIR"), "/tools_capnp.rs"));
}

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod streams_capnp {
    include!(concat!(env!("OUT_DIR"), "/streams_capnp.rs"));
}

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod envelope_capnp {
    include!(concat!(env!("OUT_DIR"), "/envelope_capnp.rs"));
}

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod garden_capnp {
    include!(concat!(env!("OUT_DIR"), "/garden_capnp.rs"));
}

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod broadcast_capnp {
    include!(concat!(env!("OUT_DIR"), "/broadcast_capnp.rs"));
}

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod vibeweaver_capnp {
    include!(concat!(env!("OUT_DIR"), "/vibeweaver_capnp.rs"));
}

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod responses_capnp {
    include!(concat!(env!("OUT_DIR"), "/responses_capnp.rs"));
}

pub mod conversion;
pub mod domain;
pub mod envelope;
pub mod frame;
pub mod garden;
pub mod metadata;
pub mod params;
pub mod request;
pub mod responses;
pub mod schema_helpers;
pub mod timing;

#[cfg(feature = "client")]
pub mod client;

#[cfg(feature = "client")]
pub mod lazy_pirate;

#[cfg(feature = "client")]
pub use client::{ClientConfig, ConnectionState, HealthTracker, HootClient, spawn_health_task};

#[cfg(feature = "client")]
pub use lazy_pirate::{AttemptResult, LazyPirateClient, LazyPirateConfig};

pub use conversion::{
    capnp_envelope_to_payload, envelope_to_payload, payload_to_capnp_envelope,
    payload_to_request,
};
pub use domain::{JobId, JobInfo, JobStatus, JobStoreStats};
pub use envelope::{ResponseEnvelope, ToolError};
pub use frame::{Command, ContentType, FrameError, HootFrame, ReadyPayload, PROTOCOL_VERSION};
pub use metadata::{GenerationParams, Metrics, StoredMetadata};
pub use request::ToolRequest;
pub use responses::ToolResponse;
pub use timing::ToolTiming;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Artifact Lineage Types (shared across tools)
// ============================================================================

/// Common artifact lineage tracking fields.
///
/// Embed this in tool variants that create artifacts to maintain lineage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ArtifactMetadata {
    pub variation_set_id: Option<String>,
    pub parent_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
}

// ============================================================================
// Tool Result Types (used by hootenanny, returned over ZMQ)
// ============================================================================

/// Successful tool output
///
/// Contains both a human-readable text summary and structured data for programmatic use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Human-readable summary of what the tool did
    pub text: String,
    /// Structured data for programmatic use
    pub data: serde_json::Value,
}

impl ToolOutput {
    /// Create a new tool output with text and structured data
    pub fn new(text: impl Into<String>, data: impl serde::Serialize) -> Self {
        Self {
            text: text.into(),
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Create a tool output with only text (no structured data)
    pub fn text_only(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            data: serde_json::Value::Null,
        }
    }
}

/// Result type for tool execution (uses typed ToolError from envelope module)
pub type ToolResult = Result<ToolOutput, ToolError>;

// ============================================================================
// ZMQ Message Types
// ============================================================================

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
///
/// Organized by domain:
/// - **Core Protocol**: Worker lifecycle, job management, responses
/// - **Content Ops**: CAS, artifacts, graph queries
/// - **Music Generation**: AI models (Orpheus, Musicgen, YuE)
/// - **Music Processing**: ABC, MIDI conversion, soundfonts
/// - **Analysis**: Audio analysis (BeatThis, CLAP)
/// - **Playback**: Chaosgarden timeline and transport
/// - **Gateway**: Resources, completions
/// - **Lua**: Script execution and storage
///
/// Many variants include artifact tracking fields. See `ArtifactMetadata`.
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

    // === Universal Tool Request ===
    /// A typed request for a specific tool.
    /// This replaces all the individual tool variants in Payload.
    ToolRequest(ToolRequest),

    // === Responses ===
    /// Typed response envelope for structured tool responses.
    TypedResponse(ResponseEnvelope),
    
    Error {
        code: String,
        message: String,
        details: Option<serde_json::Value>,
    },
    
    /// Response for ListTools
    ToolList {
        tools: Vec<ToolInfo>,
    },

    // === Direct Protocol Messages (Not Tools) ===
    
    // Chaosgarden Events
    TimelineEvent {
        event_type: TimelineEventType,
        position_beats: f64,
        tempo: f64,
        metadata: serde_json::Value,
    },

    // Stream Capture (Hootenanny → Chaosgarden)
    StreamStart {
        uri: String,
        definition: StreamDefinition,
        chunk_path: String,
    },
    StreamSwitchChunk {
        uri: String,
        new_chunk_path: String,
    },
    StreamStop {
        uri: String,
    },

    // Transport Tools (Holler → Chaosgarden) - Protocol commands
    TransportPlay,
    TransportStop,
    TransportSeek {
        position_beats: f64,
    },
    TransportStatus,

    // Timeline Tools (Holler → Chaosgarden) - Protocol commands
    TimelineQuery {
        from_beats: Option<f64>,
        to_beats: Option<f64>,
    },
    TimelineAddMarker {
        position_beats: f64,
        marker_type: String,
        metadata: serde_json::Value,
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
    Hootenanny,
    Chaosgarden,
    Vibeweaver,
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

/// Tool information for discovery and schema generation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Graph hint for identity binding (aligned with hootenanny schema)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphHint {
    pub kind: String,
    pub value: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    1.0
}

/// Stream definition for audio/MIDI capture
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamDefinition {
    pub uri: String,
    pub device_identity: String,
    pub format: StreamFormat,
    pub chunk_size_bytes: u64,
}

/// Stream format (Audio or MIDI)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamFormat {
    Audio {
        sample_rate: u32,
        channels: u8,
        sample_format: SampleFormat,
    },
    Midi,
}

/// Audio sample format
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SampleFormat {
    F32,
    I16,
    I24,
}

// =============================================================================
// Rich Tool Help (beyond MCP's basic description)
// =============================================================================

/// Rich help information for a tool, accessible via ZMQ.
///
/// Goes beyond MCP's basic tool description to include:
/// - Detailed usage instructions
/// - Examples
/// - Related tools
/// - Metadata for categorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolHelp {
    /// Tool name
    pub name: String,
    /// Brief one-line description (same as MCP)
    pub summary: String,
    /// Detailed usage instructions (markdown)
    pub instructions: String,
    /// Usage examples (markdown code blocks)
    pub examples: String,
    /// Related tools to explore
    pub related_tools: Vec<String>,
    /// Category for grouping (e.g., "audio", "graph", "generation")
    pub category: String,
    /// Timing hint: sync, async_short, async_medium, async_long
    pub timing: String,
}

// =============================================================================
// Content Encoding (for schedule/analyze)
// =============================================================================

/// Content reference encoding - how to locate content for playback or analysis.
///
/// This is a typed alternative to JSON-based encoding, enabling:
/// - Type-safe ZMQ transport without JSON
/// - Cap'n Proto serialization
/// - Validation at protocol boundaries
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Encoding {
    /// MIDI content via artifact ID
    Midi { artifact_id: String },
    /// Audio content via artifact ID
    Audio { artifact_id: String },
    /// ABC notation as raw string
    Abc { notation: String },
    /// Raw content via CAS hash with format hint
    Hash { content_hash: String, format: String },
}

/// Analysis task for the analyze tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisTask {
    /// Classify content type/characteristics
    Classify,
    /// Detect beats and downbeats
    Beats,
    /// Generate embeddings for similarity search
    Embeddings,
    /// Detect genre
    Genre,
    /// Detect mood/energy
    Mood,
}

/// Broadcast messages via PUB/SUB
///
/// These are pushed from backends to holler, which forwards them to SSE clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Broadcast {
    /// Configuration value changed
    ConfigUpdate {
        key: String,
        value: serde_json::Value,
    },

    /// Backend is shutting down
    Shutdown {
        reason: String,
    },

    /// Script cache invalidated
    ScriptInvalidate {
        hash: String,
    },

    /// Job state changed (queued, running, completed, failed)
    JobStateChanged {
        job_id: String,
        state: String,
        result: Option<serde_json::Value>,
    },

    /// Progress update for long-running operations
    Progress {
        job_id: String,
        /// Progress percentage from 0.0 to 1.0
        percent: f32,
        /// Human-readable progress message
        message: String,
    },

    /// New artifact created
    ArtifactCreated {
        artifact_id: String,
        content_hash: String,
        tags: Vec<String>,
        creator: Option<String>,
    },

    /// Timeline transport state changed (play/stop/seek)
    TransportStateChanged {
        state: String,
        position_beats: f64,
        tempo_bpm: f64,
    },

    /// Timeline marker reached during playback
    MarkerReached {
        position_beats: f64,
        marker_type: String,
        metadata: serde_json::Value,
    },

    /// Beat tick (for sync, sent at configurable interval)
    BeatTick {
        beat: u64,
        position_beats: f64,
        tempo_bpm: f64,
    },

    /// Log message from backend
    Log {
        level: String,
        message: String,
        source: String,
    },

    /// PipeWire device connected (hot-plug)
    DeviceConnected {
        /// PipeWire node ID
        pipewire_id: u32,
        /// Device name from PipeWire
        name: String,
        /// Media class (e.g., "Midi/Bridge", "Audio/Sink")
        media_class: Option<String>,
        /// Matched identity ID (if recognized)
        identity_id: Option<String>,
        /// Matched identity name (if recognized)
        identity_name: Option<String>,
    },

    /// PipeWire device disconnected
    DeviceDisconnected {
        /// PipeWire node ID that was removed
        pipewire_id: u32,
        /// Device name (if known)
        name: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::WeaveEvalRequest;
    use pretty_assertions::assert_eq;

    #[test]
    fn envelope_roundtrip() {
        let envelope = Envelope::new(Payload::Ping);
        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn weave_eval_roundtrip() {
        let envelope = Envelope::new(Payload::ToolRequest(ToolRequest::WeaveEval(WeaveEvalRequest {
            code: "print('hello')".to_string(),
        })));
        let json = serde_json::to_string_pretty(&envelope).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn typed_response_roundtrip() {
        use crate::responses::ToolResponse;

        let envelope = Envelope::new(Payload::TypedResponse(ResponseEnvelope::success(
            ToolResponse::ack("test response"),
        )));
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
        use crate::request::CasStoreRequest;
        let envelope = Envelope::new(Payload::ToolRequest(ToolRequest::CasStore(CasStoreRequest {
            data: vec![0x4d, 0x54, 0x68, 0x64], // MIDI header
            mime_type: "audio/midi".to_string(),
        })));
        let json = serde_json::to_string(&envelope).unwrap();
        // Since CasStoreRequest uses default serde, it's just an array, not base64
        // assert!(json.contains("TVRoZA==")); 
        assert!(json.contains("[77,84,104,100]"));
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.payload, parsed.payload);
    }

    #[test]
    fn worker_registration_roundtrip() {
        let reg = WorkerRegistration {
            worker_id: Uuid::new_v4(),
            worker_type: WorkerType::Vibeweaver,
            worker_name: "python-kernel".to_string(),
            capabilities: vec!["python".to_string(), "weave".to_string()],
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
