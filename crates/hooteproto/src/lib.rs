//! hooteproto - Protocol types for the Hootenanny ZMQ message bus
//!
//! This crate defines the message types exchanged between Hootenanny services
//! over ZMQ. All messages are wrapped in an Envelope for tracing and routing.
//!
//! ## HOOT01 Frame Protocol
//!
//! The `frame` module implements the HOOT01 wire protocol - a hybrid frame-based
//! format inspired by MDP (Majordomo Protocol). This enables:
//! - Routing without deserialization (fixed-width routing fields)
//! - Efficient heartbeats (no MsgPack overhead)
//! - Native binary payloads (no base64 encoding)
//!
//! ## Job System Types
//!
//! The canonical job types live here and are used by both hootenanny and luanette:
//! - `JobId` - Unique identifier for background jobs
//! - `JobStatus` - State machine for job lifecycle
//! - `JobInfo` - Complete job metadata and results
//! - `JobStoreStats` - Aggregate statistics
//!
//! ## Tool Parameter Types
//!
//! The `params` module contains types with JsonSchema derives for functionality generation.
//! Use with `baton::schema_for::<ParamType>()` to generate tool input schemas.

pub mod conversion;
pub mod frame;
pub mod garden;
pub mod params;
pub mod schema_helpers;

pub use conversion::tool_to_payload;
pub use frame::{Command, ContentType, FrameError, HootFrame, ReadyPayload, PROTOCOL_VERSION};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Job System Types (canonical, shared by hootenanny + luanette)
// ============================================================================

/// Unique identifier for a background job
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(String);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for JobId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for JobId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Current status of a background job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// Job is queued but not yet started
    Pending,
    /// Job is currently executing
    Running,
    /// Job completed successfully
    Complete,
    /// Job failed with an error
    Failed,
    /// Job was cancelled
    Cancelled,
}

/// Information about a job and its result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub job_id: JobId,
    pub status: JobStatus,
    /// Source identifier (tool name in hootenanny, script hash in luanette)
    pub source: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
}

impl JobInfo {
    pub fn new(job_id: JobId, source: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            job_id,
            status: JobStatus::Pending,
            source,
            result: None,
            error: None,
            created_at: now,
            started_at: None,
            completed_at: None,
        }
    }

    pub fn mark_running(&mut self) {
        self.status = JobStatus::Running;
        self.started_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    pub fn mark_complete(&mut self, result: serde_json::Value) {
        self.status = JobStatus::Complete;
        self.result = Some(result);
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    pub fn mark_failed(&mut self, error: String) {
        self.status = JobStatus::Failed;
        self.error = Some(error);
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    pub fn mark_cancelled(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    /// Duration in seconds if job has started
    pub fn duration_secs(&self) -> Option<u64> {
        self.started_at.map(|started| {
            let end = self.completed_at.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            });
            end.saturating_sub(started)
        })
    }
}

/// Statistics about job store state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobStoreStats {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
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

/// Tool execution error
///
/// Represents an error that occurred during tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolError {
    /// Error code for programmatic handling
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Optional additional error details
    pub details: Option<serde_json::Value>,
}

impl ToolError {
    /// Create an invalid parameters error
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: "invalid_params".into(),
            message: msg.into(),
            details: None,
        }
    }

    /// Create a tool not found error
    pub fn not_found(tool: &str) -> Self {
        Self {
            code: "tool_not_found".into(),
            message: format!("Unknown tool: {}", tool),
            details: None,
        }
    }

    /// Create an internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: "internal_error".into(),
            message: msg.into(),
            details: None,
        }
    }

    /// Create an error with a custom code
    pub fn with_code(code: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: msg.into(),
            details: None,
        }
    }

    /// Add details to this error
    pub fn with_details(mut self, details: impl serde::Serialize) -> Self {
        self.details = serde_json::to_value(details).ok();
        self
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ToolError {}

/// Result type for tool execution
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
    // CasStore uses binary encoding for efficiency over ZMQ
    // Schema's CasStoreRequest uses content_base64 String
    CasStore {
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
        mime_type: String,
    },
    CasInspect {
        hash: String,
    },
    CasGet {
        hash: String,
    },
    CasUploadFile {
        file_path: String,
        mime_type: String,
    },

    // === Orpheus Tools (Holler → Hootenanny) ===
    // Aligned with hootenanny api::schema types
    OrpheusGenerate {
        model: Option<String>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
    OrpheusGenerateSeeded {
        seed_hash: String,
        model: Option<String>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
    OrpheusContinue {
        input_hash: String,
        model: Option<String>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
    OrpheusBridge {
        section_a_hash: String,
        section_b_hash: Option<String>,
        model: Option<String>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
    OrpheusLoops {
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
        num_variations: Option<u32>,
        seed_hash: Option<String>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
    OrpheusClassify {
        midi_hash: String,
    },

    // === MIDI/Audio Tools (Holler → Hootenanny) ===
    // Aligned with hootenanny api::schema types
    ConvertMidiToWav {
        input_hash: String,
        soundfont_hash: String,
        sample_rate: Option<u32>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
    SoundfontInspect {
        soundfont_hash: String,
        include_drum_map: bool,
    },
    SoundfontPresetInspect {
        soundfont_hash: String,
        bank: i32,
        program: i32,
    },

    // === ABC Notation Tools (Holler → Hootenanny) ===
    // Aligned with hootenanny api::schema types
    AbcParse {
        abc: String,
    },
    AbcToMidi {
        abc: String,
        tempo_override: Option<u16>,
        transpose: Option<i8>,
        velocity: Option<u8>,
        channel: Option<u8>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
    AbcValidate {
        abc: String,
    },
    AbcTranspose {
        abc: String,
        semitones: Option<i8>,
        target_key: Option<String>,
    },

    // === Analysis Tools (Holler → Hootenanny) ===
    // Aligned with hootenanny api::schema types
    BeatthisAnalyze {
        audio_path: Option<String>,
        audio_hash: Option<String>,
        include_frames: bool,
    },
    ClapAnalyze {
        audio_hash: String,
        tasks: Vec<String>,
        audio_b_hash: Option<String>,
        text_candidates: Vec<String>,
        parent_id: Option<String>,
        creator: Option<String>,
    },

    // === Generation Tools (Holler → Hootenanny) ===
    // Aligned with hootenanny api::schema types
    MusicgenGenerate {
        prompt: Option<String>,
        duration: Option<f32>,
        temperature: Option<f32>,
        top_k: Option<u32>,
        top_p: Option<f32>,
        guidance_scale: Option<f32>,
        do_sample: Option<bool>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
    YueGenerate {
        lyrics: String,
        genre: Option<String>,
        max_new_tokens: Option<u32>,
        run_n_segments: Option<u32>,
        seed: Option<u64>,
        // Artifact tracking
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },

    // === Garden Tools (Holler → Hootenanny → Chaosgarden) ===
    GardenStatus,
    GardenPlay,
    GardenPause,
    GardenStop,
    GardenSeek {
        beat: f64,
    },
    GardenSetTempo {
        bpm: f64,
    },
    GardenQuery {
        query: String,
        variables: Option<serde_json::Value>,
    },
    GardenEmergencyPause,

    // === Misc Tools ===
    // Aligned with hootenanny api::schema types
    JobSleep {
        milliseconds: u64,
    },
    SampleLlm {
        prompt: String,
        max_tokens: Option<u32>,
        temperature: Option<f64>,
        system_prompt: Option<String>,
    },

    // === Artifact Tools (Holler → Hootenanny) ===
    // Aligned with hootenanny api::schema types
    ArtifactUpload {
        file_path: String,
        mime_type: String,
        variation_set_id: Option<String>,
        parent_id: Option<String>,
        tags: Vec<String>,
        creator: Option<String>,
    },
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
    // Aligned with hootenanny api::schema types
    GraphQuery {
        query: String,
        variables: serde_json::Value,
        limit: Option<usize>,
    },
    GraphBind {
        id: String,
        name: String,
        hints: Vec<GraphHint>,
    },
    GraphTag {
        identity_id: String,
        namespace: String,
        value: String,
    },
    GraphConnect {
        from_identity: String,
        from_port: String,
        to_identity: String,
        to_port: String,
        transport: Option<String>,
    },
    GraphFind {
        name: Option<String>,
        tag_namespace: Option<String>,
        tag_value: Option<String>,
    },
    GraphContext {
        tag: Option<String>,
        vibe_search: Option<String>,
        creator: Option<String>,
        limit: Option<usize>,
        include_metadata: bool,
        include_annotations: bool,
    },
    AddAnnotation {
        artifact_id: String,
        message: String,
        vibe: Option<String>,
        source: Option<String>,
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

    // === Tool Discovery ===
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
}

/// Base64 encoding for binary data in JSON
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            STANDARD.encode(bytes).serialize(serializer)
        } else {
            serializer.serialize_bytes(bytes)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            STANDARD.decode(&s).map_err(serde::de::Error::custom)
        } else {
            serde_bytes::deserialize(deserializer)
        }
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
            mime_type: "audio/midi".to_string(),
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