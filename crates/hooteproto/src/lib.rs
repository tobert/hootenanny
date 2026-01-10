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
//! ## Tool Schemas
//!
//! MCP tool schemas live in the `holler` crate (the MCP gateway) in
//! `tools_registry` and `manual_schemas`. This keeps hooteproto JSON-free.

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
pub mod garden_snapshot;
pub mod metadata;
pub mod request;
pub mod responses;
pub mod timing;

// Peer infrastructure - batteries included for building hootenanny peers
#[cfg(feature = "peer")]
pub mod client;

#[cfg(feature = "peer")]
pub mod lazy_pirate;

#[cfg(feature = "peer")]
pub mod socket_config;

#[cfg(feature = "peer")]
pub mod garden_peer;

#[cfg(feature = "peer")]
pub mod garden_listener;

#[cfg(feature = "peer")]
pub use client::{ClientConfig, ConnectionState, HealthTracker, HootClient, spawn_health_task};

#[cfg(feature = "peer")]
pub use lazy_pirate::{AttemptResult, LazyPirateClient, LazyPirateConfig};

#[cfg(feature = "peer")]
pub use garden_peer::GardenPeer;

#[cfg(feature = "peer")]
pub use garden_listener::{GardenListener, GardenSockets};

// Garden protocol types (always available, no feature gate)
pub use garden::GardenEndpoints;

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

// Garden state snapshot types for query evaluation in hootenanny
pub use garden_snapshot::{
    ApprovalInfo, AudioInput, AudioOutput, BehaviorType, GardenSnapshot, GraphEdge,
    GraphNode, IOPubEvent, IOPubMessage, LatentJob, LatentStatus, MediaType, MidiDeviceInfo,
    MidiDirection, Port, RegionSnapshot, SignalType, TempoChange, TempoMapSnapshot, TransportSnapshot,
};

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
    /// Zero-shot classification with custom labels
    ZeroShot { labels: Vec<String> },
}

/// Generative model space for sampling operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Space {
    /// Base Orpheus MIDI model
    Orpheus,
    /// Orpheus trained on children's music
    OrpheusChildren,
    /// Orpheus mono melodies
    OrpheusMonoMelodies,
    /// Orpheus loop generation
    OrpheusLoops,
    /// Orpheus section bridging
    OrpheusBridge,
    /// MusicGen audio model
    MusicGen,
    /// YuE lyrics-to-song
    Yue,
    /// ABC notation space
    Abc,
}

/// Inference parameters for generative models.
///
/// Not all parameters apply to all models:
/// - Orpheus: temperature, top_p, max_tokens, variant
/// - MusicGen: temperature, top_k, top_p, duration_seconds, guidance_scale
/// - YuE: variant (genre), max_tokens, seed
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InferenceContext {
    /// Sampling temperature (0.0 = deterministic, higher = more random)
    pub temperature: Option<f32>,
    /// Top-p (nucleus) sampling threshold
    pub top_p: Option<f32>,
    /// Top-k sampling (MusicGen)
    pub top_k: Option<u32>,
    /// Random seed for reproducibility
    pub seed: Option<u64>,
    /// Maximum tokens to generate (Orpheus, YuE)
    pub max_tokens: Option<u32>,
    /// Target duration in seconds (MusicGen)
    pub duration_seconds: Option<f32>,
    /// Classifier-free guidance scale (MusicGen)
    pub guidance_scale: Option<f32>,
    /// Model variant (e.g., "base", "children", genre for YuE)
    pub variant: Option<String>,
}

/// Target format for content projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectionTarget {
    /// Project to audio via SoundFont rendering
    Audio {
        /// SoundFont content hash for rendering
        soundfont_hash: String,
        /// Output sample rate (default: 44100)
        sample_rate: Option<u32>,
    },
    /// Project to MIDI (e.g., from ABC notation)
    Midi {
        /// MIDI channel (default: 0)
        channel: Option<u8>,
        /// Note velocity (default: 80)
        velocity: Option<u8>,
        /// MIDI program number (0-127). See General MIDI for standard mappings.
        /// E.g., 0=Piano, 33=Bass, 56=Trumpet, 52=Choir Aahs.
        program: Option<u8>,
    },
}

// =============================================================================
// Output Types and Impl Blocks for DAW Tools
// =============================================================================

/// Output format type produced by generative operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    /// MIDI format (symbolic music events)
    Midi,
    /// Audio format (PCM waveform)
    Audio,
    /// Symbolic notation (ABC, MusicXML, etc.)
    Symbolic,
}

/// Validation error for inference parameters.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Return type for Orpheus model parameters: (variant, temperature, top_p, max_tokens)
pub type OrpheusParams = (Option<String>, Option<f32>, Option<f32>, Option<u32>);

/// Return type for MusicGen parameters: (temperature, top_p, top_k, guidance_scale, duration_seconds)
pub type MusicGenParams = (Option<f32>, Option<f32>, Option<u32>, Option<f32>, Option<f32>);

impl Space {
    /// Returns the output type produced by this space.
    pub fn output_type(&self) -> OutputType {
        match self {
            Space::Orpheus
            | Space::OrpheusChildren
            | Space::OrpheusMonoMelodies
            | Space::OrpheusLoops
            | Space::OrpheusBridge => OutputType::Midi,
            Space::MusicGen | Space::Yue => OutputType::Audio,
            Space::Abc => OutputType::Symbolic,
        }
    }

    /// Returns true if this space supports continuation/extension operations.
    pub fn supports_continuation(&self) -> bool {
        match self {
            Space::Orpheus
            | Space::OrpheusChildren
            | Space::OrpheusMonoMelodies
            | Space::OrpheusLoops => true,
            Space::OrpheusBridge | Space::MusicGen | Space::Yue | Space::Abc => false,
        }
    }

    /// Returns the underlying model variant string used by the generative backend.
    pub fn model_variant(&self) -> Option<&str> {
        match self {
            Space::Orpheus => Some("base"),
            Space::OrpheusChildren => Some("children"),
            Space::OrpheusMonoMelodies => Some("mono_melodies"),
            Space::OrpheusLoops => None, // Uses dedicated loops endpoint
            Space::OrpheusBridge => Some("bridge"),
            Space::MusicGen => None, // MusicGen has its own model selection
            Space::Yue => None,      // YuE doesn't expose model variants
            Space::Abc => None,      // ABC is symbolic, not model-based
        }
    }
}

impl InferenceContext {
    /// Validate parameter ranges
    pub fn validate(&self) -> Result<(), ValidationError> {
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(ValidationError {
                    field: "temperature".to_string(),
                    message: format!("must be between 0.0 and 2.0, got {}", temp),
                });
            }
        }

        if let Some(top_p) = self.top_p {
            if !(0.0..=1.0).contains(&top_p) {
                return Err(ValidationError {
                    field: "top_p".to_string(),
                    message: format!("must be between 0.0 and 1.0, got {}", top_p),
                });
            }
        }

        if let Some(duration) = self.duration_seconds {
            if duration <= 0.0 {
                return Err(ValidationError {
                    field: "duration_seconds".to_string(),
                    message: format!("must be greater than 0.0, got {}", duration),
                });
            }
        }

        if let Some(guidance) = self.guidance_scale {
            if guidance < 0.0 {
                return Err(ValidationError {
                    field: "guidance_scale".to_string(),
                    message: format!("must be non-negative, got {}", guidance),
                });
            }
        }

        Ok(())
    }

    /// Merge with defaults for orpheus models
    fn with_orpheus_defaults(&self) -> Self {
        Self {
            temperature: self.temperature.or(Some(1.0)),
            top_p: self.top_p.or(Some(0.95)),
            top_k: self.top_k,
            seed: self.seed,
            max_tokens: self.max_tokens.or(Some(1024)),
            duration_seconds: self.duration_seconds,
            guidance_scale: self.guidance_scale,
            variant: self.variant.clone(),
        }
    }

    /// Merge with defaults for musicgen
    fn with_musicgen_defaults(&self) -> Self {
        Self {
            temperature: self.temperature.or(Some(1.0)),
            top_p: self.top_p.or(Some(0.9)),
            top_k: self.top_k.or(Some(250)),
            seed: self.seed,
            max_tokens: self.max_tokens,
            duration_seconds: self.duration_seconds.or(Some(10.0)),
            guidance_scale: self.guidance_scale.or(Some(3.0)),
            variant: self.variant.clone(),
        }
    }

    /// Convert to parameters for orpheus tools
    ///
    /// Returns: (variant, temperature, top_p, max_tokens)
    pub fn to_orpheus_params(&self) -> OrpheusParams {
        let defaults = self.with_orpheus_defaults();
        (
            defaults.variant,
            defaults.temperature,
            defaults.top_p,
            defaults.max_tokens,
        )
    }

    /// Convert to parameters for musicgen
    ///
    /// Returns: (temperature, top_p, top_k, guidance_scale, duration_seconds)
    pub fn to_musicgen_params(&self) -> MusicGenParams {
        let defaults = self.with_musicgen_defaults();
        (
            defaults.temperature,
            defaults.top_p,
            defaults.top_k,
            defaults.guidance_scale,
            defaults.duration_seconds,
        )
    }
}

impl Encoding {
    /// Returns the output type of this encoding.
    pub fn output_type(&self) -> OutputType {
        match self {
            Encoding::Midi { .. } => OutputType::Midi,
            Encoding::Audio { .. } => OutputType::Audio,
            Encoding::Abc { .. } => OutputType::Symbolic,
            Encoding::Hash { format, .. } => {
                if format.contains("midi") {
                    OutputType::Midi
                } else if format.contains("audio") || format.contains("wav") {
                    OutputType::Audio
                } else {
                    OutputType::Symbolic
                }
            }
        }
    }

    /// Returns the artifact ID if this encoding references one.
    pub fn artifact_id(&self) -> Option<&str> {
        match self {
            Encoding::Midi { artifact_id } | Encoding::Audio { artifact_id } => {
                Some(artifact_id.as_str())
            }
            Encoding::Abc { .. } | Encoding::Hash { .. } => None,
        }
    }

    /// Returns the content hash if this encoding is a hash reference.
    pub fn content_hash(&self) -> Option<&str> {
        match self {
            Encoding::Hash { content_hash, .. } => Some(content_hash.as_str()),
            _ => None,
        }
    }
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

/// Parse a Cap'n Proto broadcast message into the Rust Broadcast enum
pub fn capnp_to_broadcast(
    reader: broadcast_capnp::broadcast::Reader,
) -> capnp::Result<Broadcast> {
    use broadcast_capnp::broadcast::Which;

    match reader.which()? {
        Which::ConfigUpdate(config) => {
            let config = config?;
            let key = config.get_key()?.to_string()?;
            let value_str = config.get_value()?.to_string()?;
            let value = serde_json::from_str(&value_str).unwrap_or(serde_json::Value::Null);
            Ok(Broadcast::ConfigUpdate { key, value })
        }
        Which::Shutdown(shutdown) => {
            let shutdown = shutdown?;
            let reason = shutdown.get_reason()?.to_string()?;
            Ok(Broadcast::Shutdown { reason })
        }
        Which::ScriptInvalidate(script) => {
            let script = script?;
            let hash = script.get_hash()?.to_string()?;
            Ok(Broadcast::ScriptInvalidate { hash })
        }
        Which::JobStateChanged(job) => {
            let job = job?;
            let job_id = job.get_job_id()?.to_string()?;
            let state = job.get_state()?.to_string()?;
            let result_str = job.get_result()?.to_string()?;
            let result = if result_str.is_empty() {
                None
            } else {
                serde_json::from_str(&result_str).ok()
            };
            Ok(Broadcast::JobStateChanged {
                job_id,
                state,
                result,
            })
        }
        Which::Progress(prog) => {
            let prog = prog?;
            let job_id = prog.get_job_id()?.to_string()?;
            let percent = prog.get_percent();
            let message = prog.get_message()?.to_string()?;
            Ok(Broadcast::Progress {
                job_id,
                percent,
                message,
            })
        }
        Which::ArtifactCreated(artifact) => {
            let artifact = artifact?;
            let artifact_id = artifact.get_artifact_id()?.to_string()?;
            let content_hash = artifact.get_content_hash()?.to_string()?;
            let tags: Vec<String> = artifact
                .get_tags()?
                .iter()
                .filter_map(|t| t.ok().map(|s| s.to_string().ok()).flatten())
                .collect();
            let creator_str = artifact.get_creator()?.to_string()?;
            let creator = if creator_str.is_empty() {
                None
            } else {
                Some(creator_str)
            };
            Ok(Broadcast::ArtifactCreated {
                artifact_id,
                content_hash,
                tags,
                creator,
            })
        }
        Which::TransportStateChanged(transport) => {
            let transport = transport?;
            let state = transport.get_state()?.to_string()?;
            let position_beats = transport.get_position_beats();
            let tempo_bpm = transport.get_tempo_bpm();
            Ok(Broadcast::TransportStateChanged {
                state,
                position_beats,
                tempo_bpm,
            })
        }
        Which::MarkerReached(marker) => {
            let marker = marker?;
            let position_beats = marker.get_position_beats();
            let marker_type = marker.get_marker_type()?.to_string()?;
            let metadata_str = marker.get_metadata()?.to_string()?;
            let metadata =
                serde_json::from_str(&metadata_str).unwrap_or(serde_json::Value::Null);
            Ok(Broadcast::MarkerReached {
                position_beats,
                marker_type,
                metadata,
            })
        }
        Which::BeatTick(tick) => {
            let tick = tick?;
            let beat = tick.get_beat();
            let position_beats = tick.get_position_beats();
            let tempo_bpm = tick.get_tempo_bpm();
            Ok(Broadcast::BeatTick {
                beat,
                position_beats,
                tempo_bpm,
            })
        }
        Which::Log(log) => {
            let log = log?;
            let level = log.get_level()?.to_string()?;
            let message = log.get_message()?.to_string()?;
            let source = log.get_source()?.to_string()?;
            Ok(Broadcast::Log {
                level,
                message,
                source,
            })
        }
        Which::DeviceConnected(device) => {
            let device = device?;
            let pipewire_id = device.get_pipewire_id();
            let name = device.get_name()?.to_string()?;
            let media_class_str = device.get_media_class()?.to_string()?;
            let media_class = if media_class_str.is_empty() {
                None
            } else {
                Some(media_class_str)
            };
            let identity_id_str = device.get_identity_id()?.to_string()?;
            let identity_id = if identity_id_str.is_empty() {
                None
            } else {
                Some(identity_id_str)
            };
            let identity_name_str = device.get_identity_name()?.to_string()?;
            let identity_name = if identity_name_str.is_empty() {
                None
            } else {
                Some(identity_name_str)
            };
            Ok(Broadcast::DeviceConnected {
                pipewire_id,
                name,
                media_class,
                identity_id,
                identity_name,
            })
        }
        Which::DeviceDisconnected(device) => {
            let device = device?;
            let pipewire_id = device.get_pipewire_id();
            let name_str = device.get_name()?.to_string()?;
            let name = if name_str.is_empty() {
                None
            } else {
                Some(name_str)
            };
            Ok(Broadcast::DeviceDisconnected { pipewire_id, name })
        }
        // Stream events are handled separately by chaosgarden, not needed here
        Which::StreamHeadPosition(_)
        | Which::StreamChunkFull(_)
        | Which::StreamError(_)
        | Which::AudioAttached(_)
        | Which::AudioDetached(_)
        | Which::AudioUnderrun(_) => {
            // These are internal stream events, skip for broadcast forwarding
            Err(capnp::Error::failed("Stream events not broadcast to SSE".to_string()))
        }
    }
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
