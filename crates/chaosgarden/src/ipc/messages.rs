//! Message types for the chaosgarden IPC protocol
//!
//! All messages follow a Jupyter-inspired envelope format with header,
//! parent_header, metadata, and content.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::ipc::PROTOCOL_VERSION;

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
        artifact_id: String,
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

/// Query channel request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    pub query: String,
    pub variables: HashMap<String, serde_json::Value>,
}

/// Query channel reply
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryReply {
    Results { rows: Vec<serde_json::Value> },
    Error { error: String },
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
