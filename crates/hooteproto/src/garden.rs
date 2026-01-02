//! Message types for the chaosgarden IPC protocol
//!
//! All messages follow a Jupyter-inspired envelope format with header,
//! parent_header, metadata, and content.
//!
//! These types are shared between chaosgarden (the daemon) and holler/hootenanny (the peers).
//!
//! ## GardenEndpoints
//!
//! The 4-socket protocol is inspired by Jupyter's kernel architecture:
//! - **control**: Priority commands (shutdown, interrupt)
//! - **shell**: Normal request/reply
//! - **iopub**: Event broadcasts (publish/subscribe)
//! - **heartbeat**: Liveness detection

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Protocol version
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Default heartbeat interval
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);

/// Default heartbeat timeout (miss 3 beats = dead)
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);

// ============================================================================
// Endpoint Configuration
// ============================================================================

/// Endpoint configuration for the 4-socket garden protocol.
///
/// Used by both peers (connect) and listeners (bind).
///
/// The 4-socket protocol is inspired by Jupyter's kernel architecture:
/// - control: Priority commands (shutdown, interrupt)
/// - shell: Normal request/reply (GetSnapshot, transport controls, etc.)
/// - iopub: Event broadcasts (pub/sub for state changes)
/// - heartbeat: Liveness detection
#[derive(Debug, Clone)]
pub struct GardenEndpoints {
    /// Control channel (DEALER/ROUTER) - urgent commands
    pub control: String,
    /// Shell channel (DEALER/ROUTER) - normal commands
    pub shell: String,
    /// IOPub channel (SUB/PUB) - event broadcasts
    pub iopub: String,
    /// Heartbeat channel (REQ/REP) - liveness detection
    pub heartbeat: String,
}

impl GardenEndpoints {
    /// IPC endpoints in a specific directory.
    ///
    /// Use this with `config.infra.paths.socket_dir`:
    /// ```ignore
    /// let socket_dir = config.infra.paths.require_socket_dir()?;
    /// let endpoints = GardenEndpoints::from_socket_dir(&socket_dir.to_string_lossy());
    /// ```
    pub fn from_socket_dir(dir: &str) -> Self {
        Self {
            control: format!("ipc://{}/chaosgarden-control", dir),
            shell: format!("ipc://{}/chaosgarden-shell", dir),
            iopub: format!("ipc://{}/chaosgarden-iopub", dir),
            heartbeat: format!("ipc://{}/chaosgarden-hb", dir),
        }
    }

    /// TCP endpoints for remote daemon.
    ///
    /// Ports are allocated sequentially from `base_port`:
    /// - control: base_port
    /// - shell: base_port + 1
    /// - iopub: base_port + 2
    /// - heartbeat: base_port + 3
    pub fn tcp(host: &str, base_port: u16) -> Self {
        Self {
            control: format!("tcp://{}:{}", host, base_port),
            shell: format!("tcp://{}:{}", host, base_port + 1),
            iopub: format!("tcp://{}:{}", host, base_port + 2),
            heartbeat: format!("tcp://{}:{}", host, base_port + 3),
        }
    }

    /// In-process endpoints for testing.
    pub fn inproc(prefix: &str) -> Self {
        Self {
            control: format!("inproc://{}-control", prefix),
            shell: format!("inproc://{}-shell", prefix),
            iopub: format!("inproc://{}-iopub", prefix),
            heartbeat: format!("inproc://{}-hb", prefix),
        }
    }

    /// Create endpoints from HootConfig.
    ///
    /// Uses `infra.paths.socket_dir` for IPC mode (the default).
    /// Falls back to TCP mode if `services.chaosgarden.zmq_router` is configured
    /// with a non-default value.
    ///
    /// Returns error if socket_dir is required but not configured.
    #[cfg(feature = "peer")]
    pub fn from_config(config: &hooteconf::HootConfig) -> anyhow::Result<Self> {
        let zmq_router = &config.infra.services.chaosgarden.zmq_router;

        // TCP mode if zmq_router is explicitly configured (not the placeholder default)
        if zmq_router.starts_with("tcp://") && zmq_router != "tcp://0.0.0.0:5585" {
            if let Some(port_str) = zmq_router.rsplit(':').next() {
                if let Ok(port) = port_str.parse::<u16>() {
                    return Ok(Self::tcp("localhost", port));
                }
            }
        }

        // IPC mode - require socket_dir
        let socket_dir = config.infra.paths.require_socket_dir()?;
        Ok(Self::from_socket_dir(&socket_dir.to_string_lossy()))
    }
}

impl Default for GardenEndpoints {
    /// Default to IPC in /tmp (for development/testing).
    ///
    /// Production should use `from_config()` or `from_socket_dir()`.
    fn default() -> Self {
        Self::from_socket_dir("/tmp")
    }
}

// ============================================================================
// Message Types
// ============================================================================

/// Message header - present on every message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    /// Unique message ID for correlation
    pub msg_id: Uuid,
    /// Session ID (identifies the client connection)
    pub session: Uuid,
    /// Message type (e.g., "shell_request", "iopub_event")
    pub msg_type: String,
    /// Protocol version
    pub version: String,
    /// Timestamp when message was created
    pub timestamp: DateTime<Utc>,
}

impl MessageHeader {
    pub fn new(session: Uuid, msg_type: impl Into<String>) -> Self {
        Self {
            msg_id: Uuid::new_v4(),
            session,
            msg_type: msg_type.into(),
            version: PROTOCOL_VERSION.to_string(),
            timestamp: Utc::now(),
        }
    }
}

/// Generic message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message<T> {
    pub header: MessageHeader,
    /// Reference to the message this is replying to (if any)
    pub parent_header: Option<MessageHeader>,
    /// Arbitrary metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// The actual content
    pub content: T,
}

impl<T> Message<T> {
    pub fn new(session: Uuid, msg_type: impl Into<String>, content: T) -> Self {
        Self {
            header: MessageHeader::new(session, msg_type),
            parent_header: None,
            metadata: HashMap::new(),
            content,
        }
    }

    pub fn reply(parent: &MessageHeader, msg_type: impl Into<String>, content: T) -> Self {
        Self {
            header: MessageHeader::new(parent.session, msg_type),
            parent_header: Some(parent.clone()),
            metadata: HashMap::new(),
            content,
        }
    }
}

/// Musical time in beats
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Beat(pub f64);

impl Beat {
    pub fn zero() -> Self {
        Self(0.0)
    }
}

/// Content type for artifacts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Audio,
    Midi,
    Control,
}

/// Port reference for graph connections
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRef {
    pub node_id: Uuid,
    pub port_name: String,
}

/// Summary of a region for queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionSummary {
    pub region_id: Uuid,
    pub position: Beat,
    pub duration: Beat,
    pub is_latent: bool,
    pub artifact_id: Option<String>,
}

/// Pending approval information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub region_id: Uuid,
    pub artifact_id: String,
    pub content_hash: String,
    pub content_type: ContentType,
    pub resolved_at: DateTime<Utc>,
}

/// Region behavior specification (for creation)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Behavior {
    PlayContent {
        /// CAS content hash - chaosgarden resolves this to load audio
        content_hash: String,
    },
    Latent {
        job_id: String,
    },
    ApplyProcessing {
        parameter: String,
        curve: Vec<CurvePoint>,
    },
    EmitTrigger {
        event_type: String,
    },
}

/// Curve point for automation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurvePoint {
    pub beat: Beat,
    pub value: f64,
}

/// Node descriptor for graph operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDescriptor {
    pub name: String,
    pub node_type: String,
    pub config: serde_json::Value,
}

/// Participant information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: Uuid,
    pub name: String,
    pub capabilities: Vec<String>,
}

/// Updates to a participant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantUpdate {
    pub name: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

/// Shell channel requests (hootenanny -> chaosgarden)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellRequest {
    // Region operations
    CreateRegion {
        position: Beat,
        duration: Beat,
        behavior: Behavior,
    },
    DeleteRegion {
        region_id: Uuid,
    },
    MoveRegion {
        region_id: Uuid,
        new_position: Beat,
    },

    // Latent state updates
    UpdateLatentStarted {
        region_id: Uuid,
        job_id: String,
    },
    UpdateLatentProgress {
        region_id: Uuid,
        progress: f32,
    },
    UpdateLatentResolved {
        region_id: Uuid,
        artifact_id: String,
        content_hash: String,
        content_type: ContentType,
    },
    UpdateLatentFailed {
        region_id: Uuid,
        error: String,
    },

    // Approval operations
    ApproveLatent {
        region_id: Uuid,
        decided_by: Uuid,
    },
    RejectLatent {
        region_id: Uuid,
        decided_by: Uuid,
        reason: Option<String>,
    },

    // Playback control
    Play,
    Pause,
    Stop,
    Seek {
        beat: Beat,
    },
    SetTempo {
        bpm: f64,
    },

    // Graph operations
    AddNode {
        node: NodeDescriptor,
    },
    RemoveNode {
        node_id: Uuid,
    },
    Connect {
        source: PortRef,
        dest: PortRef,
    },
    Disconnect {
        source: PortRef,
        dest: PortRef,
    },

    // Participant operations
    RegisterParticipant {
        participant: Participant,
    },
    UpdateParticipant {
        participant_id: Uuid,
        updates: ParticipantUpdate,
    },

    // State queries
    GetTransportState,
    GetRegions {
        range: Option<(Beat, Beat)>,
    },
    GetPendingApprovals,
    /// Full state snapshot for Trustfall query evaluation in hootenanny.
    /// Returns GardenSnapshot via Cap'n Proto (not JSON).
    GetSnapshot,
    /// Get audio graph nodes and edges.
    GetGraph,
    /// Get I/O devices (outputs, inputs, MIDI).
    GetIOState,

    // Stream capture commands
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

    // Audio output attachment (dynamic PipeWire output)
    AttachAudio {
        /// Device name hint (None = default output)
        device_name: Option<String>,
        /// Sample rate (None = 48000)
        sample_rate: Option<u32>,
        /// Latency in frames (None = 256)
        latency_frames: Option<u32>,
    },
    DetachAudio,
    GetAudioStatus,

    // Audio input attachment (monitor input)
    AttachInput {
        /// Device name hint (None = default input)
        device_name: Option<String>,
        /// Sample rate (None = 48000, should match output)
        sample_rate: Option<u32>,
    },
    DetachInput,
    GetInputStatus,

    // Monitor control (input -> output passthrough)
    SetMonitor {
        /// Enable/disable monitor (None = don't change)
        enabled: Option<bool>,
        /// Monitor gain 0.0-1.0 (None = don't change)
        gain: Option<f32>,
    },
}

/// Stream format definition for audio/MIDI capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamDefinition {
    pub device_identity: String,
    pub format: StreamFormat,
    pub chunk_size_bytes: u64,
}

/// Stream format (Audio or MIDI)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamFormat {
    Audio {
        sample_rate: u32,
        channels: u16,
        sample_format: SampleFormat,
    },
    Midi,
}

/// Audio sample format
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleFormat {
    F32Le,
    S16Le,
    S24Le,
    S32Le,
}

/// Shell channel replies (chaosgarden -> hootenanny)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellReply {
    Ok {
        #[serde(default)]
        result: serde_json::Value,
    },
    Error {
        error: String,
        traceback: Option<String>,
    },
    RegionCreated {
        region_id: Uuid,
    },
    NodeAdded {
        node_id: Uuid,
    },
    TransportState {
        playing: bool,
        position: Beat,
        tempo: f64,
    },
    Regions {
        regions: Vec<RegionSummary>,
    },
    PendingApprovals {
        approvals: Vec<PendingApproval>,
    },
    AudioStatus {
        attached: bool,
        device_name: Option<String>,
        sample_rate: Option<u32>,
        latency_frames: Option<u32>,
        callbacks: u64,
        samples_written: u64,
        underruns: u64,
        // Debug counters for RT mixer
        #[serde(default)]
        monitor_reads: u64,
        #[serde(default)]
        monitor_samples: u64,
    },
    InputStatus {
        attached: bool,
        device_name: Option<String>,
        sample_rate: Option<u32>,
        channels: Option<u32>,
        monitor_enabled: bool,
        monitor_gain: f32,
        callbacks: u64,
        samples_captured: u64,
        overruns: u64,
    },
    /// Full state snapshot for Trustfall query evaluation in hootenanny
    Snapshot {
        snapshot: crate::garden_snapshot::GardenSnapshot,
    },
    /// Just the audio graph (nodes + edges) for lightweight queries
    GraphSnapshot {
        nodes: Vec<crate::garden_snapshot::GraphNode>,
        edges: Vec<crate::garden_snapshot::GraphEdge>,
    },
    /// Just I/O device state
    IOState {
        outputs: Vec<crate::garden_snapshot::AudioOutput>,
        inputs: Vec<crate::garden_snapshot::AudioInput>,
        midi_devices: Vec<crate::garden_snapshot::MidiDeviceInfo>,
    },
}

/// Control channel requests (priority)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlRequest {
    Shutdown,
    Interrupt,
    EmergencyPause,
    DebugDump,
}

/// Control channel replies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlReply {
    Ok,
    ShuttingDown,
    Interrupted { was_running: String },
    DebugDump { state: serde_json::Value },
}

/// IOPub channel events (broadcast to all subscribers)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IOPubEvent {
    // Status
    Status {
        execution_state: ExecutionState,
    },

    // Latent lifecycle
    LatentSubmitted {
        region_id: Uuid,
        job_id: String,
    },
    LatentProgress {
        region_id: Uuid,
        progress: f32,
    },
    LatentResolved {
        region_id: Uuid,
        artifact_id: String,
        content_hash: String,
    },
    LatentFailed {
        region_id: Uuid,
        error: String,
    },
    LatentApproved {
        region_id: Uuid,
    },
    LatentRejected {
        region_id: Uuid,
        reason: Option<String>,
    },

    // Playback
    PlaybackStarted,
    PlaybackStopped,
    PlaybackPosition {
        beat: Beat,
        second: f64,
    },
    MixedIn {
        region_id: Uuid,
        at_beat: Beat,
    },

    // Graph changes
    NodeAdded {
        node_id: Uuid,
        name: String,
    },
    NodeRemoved {
        node_id: Uuid,
    },
    ConnectionMade {
        source: PortRef,
        dest: PortRef,
    },
    ConnectionBroken {
        source: PortRef,
        dest: PortRef,
    },

    // Participant changes
    ParticipantOnline {
        participant_id: Uuid,
        name: String,
    },
    ParticipantOffline {
        participant_id: Uuid,
    },
    CapabilityChanged {
        participant_id: Uuid,
        capability: String,
        available: bool,
    },

    // Errors and warnings
    Error {
        error: String,
        context: Option<String>,
    },
    Warning {
        message: String,
    },

    // Audio output attachment events
    AudioAttached {
        device_name: String,
        sample_rate: u32,
        latency_frames: u32,
    },
    AudioDetached,
    AudioUnderrun {
        count: u64,
    },

    // PipeWire device hot-plug events
    DeviceConnected {
        /// PipeWire node ID
        pipewire_id: u32,
        /// Device name from PipeWire
        name: String,
        /// Media class (e.g., "Midi/Bridge", "Audio/Sink")
        media_class: Option<String>,
        /// Matched identity ID (if device was recognized)
        identity_id: Option<String>,
        /// Matched identity name (if device was recognized)
        identity_name: Option<String>,
    },
    DeviceDisconnected {
        /// PipeWire node ID that was removed
        pipewire_id: u32,
        /// Device name (if known from previous state)
        name: Option<String>,
    },

    // Stream capture events
    StreamHeadPosition {
        stream_uri: String,
        sample_position: u64,
        byte_position: u64,
        wall_clock: DateTime<Utc>,
    },
    StreamChunkFull {
        stream_uri: String,
        path: String,
        bytes_written: u64,
        samples_written: u64,
        wall_clock: DateTime<Utc>,
    },
    StreamError {
        stream_uri: String,
        error: String,
        recoverable: bool,
    },
}

/// Execution state for status events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Idle,
    Busy,
    Starting,
    ShuttingDown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_request_serialization() {
        let req = ShellRequest::CreateRegion {
            position: Beat(4.0),
            duration: Beat(8.0),
            behavior: Behavior::Latent {
                job_id: "job_123".to_string(),
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: ShellRequest = serde_json::from_str(&json).unwrap();

        match decoded {
            ShellRequest::CreateRegion {
                position,
                duration,
                behavior,
            } => {
                assert_eq!(position.0, 4.0);
                assert_eq!(duration.0, 8.0);
                match behavior {
                    Behavior::Latent { job_id } => assert_eq!(job_id, "job_123"),
                    _ => panic!("wrong behavior"),
                }
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_iopub_event_serialization() {
        let event = IOPubEvent::LatentProgress {
            region_id: Uuid::new_v4(),
            progress: 0.75,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("latent_progress"));
        assert!(json.contains("0.75"));
    }

    #[test]
    fn test_message_envelope() {
        let session = Uuid::new_v4();
        let msg = Message::new(session, "shell_request", ShellRequest::Play);

        assert_eq!(msg.header.session, session);
        assert_eq!(msg.header.msg_type, "shell_request");
        assert!(msg.parent_header.is_none());
    }

    #[test]
    fn test_message_reply() {
        let session = Uuid::new_v4();
        let original = Message::new(session, "shell_request", ShellRequest::Play);
        let reply = Message::reply(
            &original.header,
            "shell_reply",
            ShellReply::Ok {
                result: serde_json::Value::Null,
            },
        );

        assert_eq!(reply.header.session, session);
        assert!(reply.parent_header.is_some());
        assert_eq!(
            reply.parent_header.as_ref().unwrap().msg_id,
            original.header.msg_id
        );
    }
}
