//! Garden state snapshot types for cross-process query evaluation.
//!
//! These types mirror the Cap'n Proto schema in `garden.capnp` and enable
//! hootenanny to fetch chaosgarden state for local Trustfall evaluation,
//! keeping allocation-heavy query processing out of the real-time audio process.

use serde::{Deserialize, Serialize};

/// Full garden state snapshot - everything needed for Trustfall queries.
///
/// Fetched from chaosgarden via Cap'n Proto RPC. Contains:
/// - Transport state (playing, position, tempo)
/// - Timeline regions
/// - Audio graph (nodes + edges)
/// - Latent job tracking
/// - Audio I/O devices
/// - Tempo map for time conversions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenSnapshot {
    /// Monotonic version for cache invalidation
    pub version: u64,
    /// Current playback state
    pub transport: TransportSnapshot,
    /// All timeline regions
    pub regions: Vec<RegionSnapshot>,
    /// Audio graph nodes
    pub nodes: Vec<GraphNode>,
    /// Audio graph edges (connections)
    pub edges: Vec<GraphEdge>,
    /// Currently running latent jobs
    pub latent_jobs: Vec<LatentJob>,
    /// Pending content approvals
    pub pending_approvals: Vec<ApprovalInfo>,
    /// Audio outputs
    pub outputs: Vec<AudioOutput>,
    /// Audio inputs
    pub inputs: Vec<AudioInput>,
    /// MIDI devices
    pub midi_devices: Vec<MidiDeviceInfo>,
    /// Tempo map for time conversions
    pub tempo_map: TempoMapSnapshot,
}

/// Transport state snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportSnapshot {
    pub playing: bool,
    pub position: f64, // Beat position
    pub tempo: f64,    // BPM
}

/// Region with all queryable fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionSnapshot {
    pub id: String, // UUID as string
    pub position: f64,
    pub duration: f64,
    pub behavior_type: BehaviorType,
    pub name: Option<String>,
    pub tags: Vec<String>,

    // PlayContent behavior
    pub content_hash: Option<String>,
    pub content_type: Option<MediaType>,

    // Latent behavior
    pub latent_status: Option<LatentStatus>,
    pub latent_progress: f32,
    pub job_id: Option<String>,
    pub generation_tool: Option<String>,

    // Computed/lifecycle flags
    pub is_resolved: bool,
    pub is_approved: bool,
    pub is_playable: bool,
    pub is_alive: bool,
    pub is_tombstoned: bool,
}

/// Region behavior type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorType {
    PlayContent,
    Latent,
    ApplyProcessing,
    EmitTrigger,
    Custom,
}

/// Media content type for artifacts (audio, MIDI, control data).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Audio,
    Midi,
    Control,
}

/// Latent region status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatentStatus {
    Pending,
    Running,
    Resolved,
    Approved,
    Rejected,
    Failed,
}

/// Audio graph node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String, // UUID as string
    pub name: String,
    pub type_id: String,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub latency_samples: u32,
    pub can_realtime: bool,
    pub can_offline: bool,
}

/// Node port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub signal_type: SignalType,
}

/// Signal type for ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    Audio,
    Midi,
    Control,
    Trigger,
}

/// Audio graph edge (connection between nodes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source_id: String,
    pub source_port: String,
    pub dest_id: String,
    pub dest_port: String,
}

/// Running latent job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatentJob {
    pub id: String,
    pub region_id: String,
    pub tool: String,
    pub progress: f32,
}

/// Pending content approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalInfo {
    pub region_id: String,
    pub content_hash: String,
    pub content_type: MediaType,
}

/// Audio output device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioOutput {
    pub id: String,
    pub name: String,
    pub channels: u8,
    pub pw_node_id: Option<u32>,
}

/// Audio input device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioInput {
    pub id: String,
    pub name: String,
    pub channels: u8,
    pub port_pattern: Option<String>,
    pub pw_node_id: Option<u32>,
}

/// MIDI device info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiDeviceInfo {
    pub id: String,
    pub name: String,
    pub direction: MidiDirection,
    pub pw_node_id: Option<u32>,
}

/// MIDI device direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiDirection {
    Input,
    Output,
}

/// Tempo map for time conversions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoMapSnapshot {
    pub default_tempo: f64,
    pub ticks_per_beat: u32,
    pub changes: Vec<TempoChange>,
}

/// Tempo change event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempoChange {
    pub tick: i64,
    pub tempo: f64,
}

// ============================================================================
// IOPub Events (Cap'n Proto version)
// ============================================================================

/// IOPub message with version for cache invalidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IOPubMessage {
    /// State version after this event
    pub version: u64,
    /// Unix timestamp in milliseconds
    pub timestamp: u64,
    /// The event
    pub event: IOPubEvent,
}

/// IOPub event types - replaces JSON IOPubEvent for efficient notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IOPubEvent {
    /// Generic state change - invalidates entire cache
    StateChanged,

    // Transport
    PlaybackStarted,
    PlaybackStopped,
    PlaybackPosition { beat: f64, second: f64 },

    // Regions
    RegionCreated { region_id: String },
    RegionDeleted { region_id: String },
    RegionMoved { region_id: String, new_position: f64 },

    // Latent lifecycle
    LatentStarted { region_id: String, job_id: String },
    LatentProgress { region_id: String, progress: f32 },
    LatentResolved { region_id: String, artifact_id: String, content_hash: String },
    LatentFailed { region_id: String, error: String },
    LatentApproved { region_id: String },
    LatentRejected { region_id: String, reason: Option<String> },

    // Graph changes
    NodeAdded { node_id: String, name: String },
    NodeRemoved { node_id: String },
    ConnectionMade { source_id: String, source_port: String, dest_id: String, dest_port: String },
    ConnectionBroken { source_id: String, source_port: String, dest_id: String, dest_port: String },

    // Audio I/O
    AudioAttached { device_name: String, sample_rate: u32, latency_frames: u32 },
    AudioDetached,
    AudioUnderrun { count: u64 },

    // Errors
    Error { error: String, context: Option<String> },
    Warning { message: String },
}

impl IOPubEvent {
    /// Returns true if this event should invalidate the cached snapshot.
    pub fn invalidates_cache(&self) -> bool {
        match self {
            // These always invalidate
            IOPubEvent::StateChanged
            | IOPubEvent::RegionCreated { .. }
            | IOPubEvent::RegionDeleted { .. }
            | IOPubEvent::RegionMoved { .. }
            | IOPubEvent::LatentStarted { .. }
            | IOPubEvent::LatentResolved { .. }
            | IOPubEvent::LatentFailed { .. }
            | IOPubEvent::LatentApproved { .. }
            | IOPubEvent::LatentRejected { .. }
            | IOPubEvent::NodeAdded { .. }
            | IOPubEvent::NodeRemoved { .. }
            | IOPubEvent::ConnectionMade { .. }
            | IOPubEvent::ConnectionBroken { .. }
            | IOPubEvent::AudioAttached { .. }
            | IOPubEvent::AudioDetached => true,

            // These don't change queryable state
            IOPubEvent::PlaybackStarted
            | IOPubEvent::PlaybackStopped
            | IOPubEvent::PlaybackPosition { .. }
            | IOPubEvent::LatentProgress { .. }
            | IOPubEvent::AudioUnderrun { .. }
            | IOPubEvent::Error { .. }
            | IOPubEvent::Warning { .. } => false,
        }
    }
}

// ============================================================================
// Cap'n Proto Conversion (garden_capnp -> Rust types)
// ============================================================================

use crate::garden_capnp;

impl GardenSnapshot {
    /// Read from Cap'n Proto message.
    pub fn from_capnp(reader: garden_capnp::garden_snapshot::Reader) -> capnp::Result<Self> {
        let transport_reader = reader.get_transport()?;
        let transport = TransportSnapshot {
            playing: transport_reader.get_playing(),
            position: transport_reader.get_position()?.get_value(),
            tempo: transport_reader.get_tempo(),
        };

        let mut regions = Vec::new();
        for region_reader in reader.get_regions()? {
            regions.push(RegionSnapshot::from_capnp(region_reader)?);
        }

        let mut nodes = Vec::new();
        for node_reader in reader.get_nodes()? {
            nodes.push(GraphNode::from_capnp(node_reader)?);
        }

        let mut edges = Vec::new();
        for edge_reader in reader.get_edges()? {
            edges.push(GraphEdge::from_capnp(edge_reader)?);
        }

        let mut latent_jobs = Vec::new();
        for job_reader in reader.get_latent_jobs()? {
            latent_jobs.push(LatentJob::from_capnp(job_reader)?);
        }

        let mut pending_approvals = Vec::new();
        for approval_reader in reader.get_pending_approvals()? {
            pending_approvals.push(ApprovalInfo::from_capnp(approval_reader)?);
        }

        let mut outputs = Vec::new();
        for output_reader in reader.get_outputs()? {
            outputs.push(AudioOutput::from_capnp(output_reader)?);
        }

        let mut inputs = Vec::new();
        for input_reader in reader.get_inputs()? {
            inputs.push(AudioInput::from_capnp(input_reader)?);
        }

        let mut midi_devices = Vec::new();
        for device_reader in reader.get_midi_devices()? {
            midi_devices.push(MidiDeviceInfo::from_capnp(device_reader)?);
        }

        let tempo_map = TempoMapSnapshot::from_capnp(reader.get_tempo_map()?)?;

        Ok(Self {
            version: reader.get_version(),
            transport,
            regions,
            nodes,
            edges,
            latent_jobs,
            pending_approvals,
            outputs,
            inputs,
            midi_devices,
            tempo_map,
        })
    }
}

impl RegionSnapshot {
    pub fn from_capnp(reader: garden_capnp::region_snapshot::Reader) -> capnp::Result<Self> {
        let behavior_type = match reader.get_behavior_type()? {
            garden_capnp::BehaviorType::PlayContent => BehaviorType::PlayContent,
            garden_capnp::BehaviorType::Latent => BehaviorType::Latent,
            garden_capnp::BehaviorType::ApplyProcessing => BehaviorType::ApplyProcessing,
            garden_capnp::BehaviorType::EmitTrigger => BehaviorType::EmitTrigger,
            garden_capnp::BehaviorType::Custom => BehaviorType::Custom,
        };

        let content_type = match reader.get_content_type()? {
            garden_capnp::ContentTypeEnum::Audio => Some(MediaType::Audio),
            garden_capnp::ContentTypeEnum::Midi => Some(MediaType::Midi),
            garden_capnp::ContentTypeEnum::Control => Some(MediaType::Control),
        };

        let latent_status = match reader.get_latent_status()? {
            garden_capnp::LatentStatusEnum::None => None,
            garden_capnp::LatentStatusEnum::Pending => Some(LatentStatus::Pending),
            garden_capnp::LatentStatusEnum::Running => Some(LatentStatus::Running),
            garden_capnp::LatentStatusEnum::Resolved => Some(LatentStatus::Resolved),
            garden_capnp::LatentStatusEnum::Approved => Some(LatentStatus::Approved),
            garden_capnp::LatentStatusEnum::Rejected => Some(LatentStatus::Rejected),
            garden_capnp::LatentStatusEnum::Failed => Some(LatentStatus::Failed),
        };

        let name_str = reader.get_name()?.to_str()?;
        let name = if name_str.is_empty() { None } else { Some(name_str.to_string()) };

        let content_hash_str = reader.get_content_hash()?.to_str()?;
        let content_hash = if content_hash_str.is_empty() { None } else { Some(content_hash_str.to_string()) };

        let job_id_str = reader.get_job_id()?.to_str()?;
        let job_id = if job_id_str.is_empty() { None } else { Some(job_id_str.to_string()) };

        let generation_tool_str = reader.get_generation_tool()?.to_str()?;
        let generation_tool = if generation_tool_str.is_empty() { None } else { Some(generation_tool_str.to_string()) };

        let mut tags = Vec::new();
        for tag_reader in reader.get_tags()? {
            tags.push(tag_reader?.to_string()?);
        }

        Ok(Self {
            id: reader.get_id()?.to_string()?,
            position: reader.get_position(),
            duration: reader.get_duration(),
            behavior_type,
            name,
            tags,
            content_hash,
            content_type,
            latent_status,
            latent_progress: reader.get_latent_progress(),
            job_id,
            generation_tool,
            is_resolved: reader.get_is_resolved(),
            is_approved: reader.get_is_approved(),
            is_playable: reader.get_is_playable(),
            is_alive: reader.get_is_alive(),
            is_tombstoned: reader.get_is_tombstoned(),
        })
    }
}

impl GraphNode {
    pub fn from_capnp(reader: garden_capnp::graph_node::Reader) -> capnp::Result<Self> {
        let mut inputs = Vec::new();
        for port_reader in reader.get_inputs()? {
            inputs.push(Port::from_capnp(port_reader)?);
        }

        let mut outputs = Vec::new();
        for port_reader in reader.get_outputs()? {
            outputs.push(Port::from_capnp(port_reader)?);
        }

        Ok(Self {
            id: reader.get_id()?.to_string()?,
            name: reader.get_name()?.to_string()?,
            type_id: reader.get_type_id()?.to_string()?,
            inputs,
            outputs,
            latency_samples: reader.get_latency_samples(),
            can_realtime: reader.get_can_realtime(),
            can_offline: reader.get_can_offline(),
        })
    }
}

impl Port {
    pub fn from_capnp(reader: garden_capnp::port::Reader) -> capnp::Result<Self> {
        let signal_type = match reader.get_signal_type()? {
            garden_capnp::SignalTypeEnum::Audio => SignalType::Audio,
            garden_capnp::SignalTypeEnum::Midi => SignalType::Midi,
            garden_capnp::SignalTypeEnum::Control => SignalType::Control,
            garden_capnp::SignalTypeEnum::Trigger => SignalType::Trigger,
        };

        Ok(Self {
            name: reader.get_name()?.to_string()?,
            signal_type,
        })
    }
}

impl GraphEdge {
    pub fn from_capnp(reader: garden_capnp::graph_edge::Reader) -> capnp::Result<Self> {
        Ok(Self {
            source_id: reader.get_source_id()?.to_string()?,
            source_port: reader.get_source_port()?.to_string()?,
            dest_id: reader.get_dest_id()?.to_string()?,
            dest_port: reader.get_dest_port()?.to_string()?,
        })
    }
}

impl LatentJob {
    pub fn from_capnp(reader: garden_capnp::latent_job::Reader) -> capnp::Result<Self> {
        Ok(Self {
            id: reader.get_id()?.to_string()?,
            region_id: reader.get_region_id()?.to_string()?,
            tool: reader.get_tool()?.to_string()?,
            progress: reader.get_progress(),
        })
    }
}

impl ApprovalInfo {
    pub fn from_capnp(reader: garden_capnp::approval_info::Reader) -> capnp::Result<Self> {
        let content_type = match reader.get_content_type()? {
            garden_capnp::ContentTypeEnum::Audio => MediaType::Audio,
            garden_capnp::ContentTypeEnum::Midi => MediaType::Midi,
            garden_capnp::ContentTypeEnum::Control => MediaType::Control,
        };

        Ok(Self {
            region_id: reader.get_region_id()?.to_string()?,
            content_hash: reader.get_content_hash()?.to_string()?,
            content_type,
        })
    }
}

impl AudioOutput {
    pub fn from_capnp(reader: garden_capnp::audio_output::Reader) -> capnp::Result<Self> {
        let pw_node_id = if reader.get_has_pw_node_id() {
            Some(reader.get_pw_node_id())
        } else {
            None
        };

        Ok(Self {
            id: reader.get_id()?.to_string()?,
            name: reader.get_name()?.to_string()?,
            channels: reader.get_channels(),
            pw_node_id,
        })
    }
}

impl AudioInput {
    pub fn from_capnp(reader: garden_capnp::audio_input::Reader) -> capnp::Result<Self> {
        let pw_node_id = if reader.get_has_pw_node_id() {
            Some(reader.get_pw_node_id())
        } else {
            None
        };

        let port_pattern_str = reader.get_port_pattern()?.to_str()?;
        let port_pattern = if port_pattern_str.is_empty() { None } else { Some(port_pattern_str.to_string()) };

        Ok(Self {
            id: reader.get_id()?.to_string()?,
            name: reader.get_name()?.to_string()?,
            channels: reader.get_channels(),
            port_pattern,
            pw_node_id,
        })
    }
}

impl MidiDeviceInfo {
    pub fn from_capnp(reader: garden_capnp::midi_device_info::Reader) -> capnp::Result<Self> {
        let direction = match reader.get_direction()? {
            garden_capnp::MidiDirection::Input => MidiDirection::Input,
            garden_capnp::MidiDirection::Output => MidiDirection::Output,
        };

        let pw_node_id = if reader.get_has_pw_node_id() {
            Some(reader.get_pw_node_id())
        } else {
            None
        };

        Ok(Self {
            id: reader.get_id()?.to_string()?,
            name: reader.get_name()?.to_string()?,
            direction,
            pw_node_id,
        })
    }
}

impl TempoMapSnapshot {
    pub fn from_capnp(reader: garden_capnp::tempo_map_snapshot::Reader) -> capnp::Result<Self> {
        let mut changes = Vec::new();
        for change_reader in reader.get_changes()? {
            changes.push(TempoChange {
                tick: change_reader.get_tick(),
                tempo: change_reader.get_tempo(),
            });
        }

        Ok(Self {
            default_tempo: reader.get_default_tempo(),
            ticks_per_beat: reader.get_ticks_per_beat(),
            changes,
        })
    }
}

impl IOPubMessage {
    pub fn from_capnp(reader: garden_capnp::i_o_pub_message::Reader) -> capnp::Result<Self> {
        let event = IOPubEvent::from_capnp(reader.get_event()?)?;

        Ok(Self {
            version: reader.get_version(),
            timestamp: reader.get_timestamp(),
            event,
        })
    }
}

impl IOPubEvent {
    pub fn from_capnp(reader: garden_capnp::i_o_pub_event_union::Reader) -> capnp::Result<Self> {
        use garden_capnp::i_o_pub_event_union::Which;

        match reader.which()? {
            Which::StateChanged(()) => Ok(IOPubEvent::StateChanged),
            Which::PlaybackStarted(()) => Ok(IOPubEvent::PlaybackStarted),
            Which::PlaybackStopped(()) => Ok(IOPubEvent::PlaybackStopped),
            Which::PlaybackPosition(pos) => {
                let pos = pos?;
                Ok(IOPubEvent::PlaybackPosition {
                    beat: pos.get_beat(),
                    second: pos.get_second(),
                })
            }
            Which::RegionCreated(id) => Ok(IOPubEvent::RegionCreated {
                region_id: id?.to_string()?,
            }),
            Which::RegionDeleted(id) => Ok(IOPubEvent::RegionDeleted {
                region_id: id?.to_string()?,
            }),
            Which::RegionMoved(moved) => {
                let moved = moved?;
                Ok(IOPubEvent::RegionMoved {
                    region_id: moved.get_region_id()?.to_string()?,
                    new_position: moved.get_new_position(),
                })
            }
            Which::LatentStarted(started) => {
                let started = started?;
                Ok(IOPubEvent::LatentStarted {
                    region_id: started.get_region_id()?.to_string()?,
                    job_id: started.get_job_id()?.to_string()?,
                })
            }
            Which::LatentProgress(progress) => {
                let progress = progress?;
                Ok(IOPubEvent::LatentProgress {
                    region_id: progress.get_region_id()?.to_string()?,
                    progress: progress.get_progress(),
                })
            }
            Which::LatentResolved(resolved) => {
                let resolved = resolved?;
                Ok(IOPubEvent::LatentResolved {
                    region_id: resolved.get_region_id()?.to_string()?,
                    artifact_id: resolved.get_artifact_id()?.to_string()?,
                    content_hash: resolved.get_content_hash()?.to_string()?,
                })
            }
            Which::LatentFailed(failed) => {
                let failed = failed?;
                Ok(IOPubEvent::LatentFailed {
                    region_id: failed.get_region_id()?.to_string()?,
                    error: failed.get_error()?.to_string()?,
                })
            }
            Which::LatentApproved(id) => Ok(IOPubEvent::LatentApproved {
                region_id: id?.to_string()?,
            }),
            Which::LatentRejected(rejected) => {
                let rejected = rejected?;
                let reason_str = rejected.get_reason()?.to_str()?;
                let reason = if reason_str.is_empty() { None } else { Some(reason_str.to_string()) };
                Ok(IOPubEvent::LatentRejected {
                    region_id: rejected.get_region_id()?.to_string()?,
                    reason,
                })
            }
            Which::NodeAdded(added) => {
                let added = added?;
                Ok(IOPubEvent::NodeAdded {
                    node_id: added.get_node_id()?.to_string()?,
                    name: added.get_name()?.to_string()?,
                })
            }
            Which::NodeRemoved(id) => Ok(IOPubEvent::NodeRemoved {
                node_id: id?.to_string()?,
            }),
            Which::ConnectionMade(conn) => {
                let conn = conn?;
                Ok(IOPubEvent::ConnectionMade {
                    source_id: conn.get_source_id()?.to_string()?,
                    source_port: conn.get_source_port()?.to_string()?,
                    dest_id: conn.get_dest_id()?.to_string()?,
                    dest_port: conn.get_dest_port()?.to_string()?,
                })
            }
            Which::ConnectionBroken(conn) => {
                let conn = conn?;
                Ok(IOPubEvent::ConnectionBroken {
                    source_id: conn.get_source_id()?.to_string()?,
                    source_port: conn.get_source_port()?.to_string()?,
                    dest_id: conn.get_dest_id()?.to_string()?,
                    dest_port: conn.get_dest_port()?.to_string()?,
                })
            }
            Which::AudioAttached(attached) => {
                let attached = attached?;
                Ok(IOPubEvent::AudioAttached {
                    device_name: attached.get_device_name()?.to_string()?,
                    sample_rate: attached.get_sample_rate(),
                    latency_frames: attached.get_latency_frames(),
                })
            }
            Which::AudioDetached(()) => Ok(IOPubEvent::AudioDetached),
            Which::AudioUnderrun(count) => Ok(IOPubEvent::AudioUnderrun { count }),
            Which::Error(err) => {
                let err = err?;
                let context_str = err.get_context()?.to_str()?;
                let context = if context_str.is_empty() { None } else { Some(context_str.to_string()) };
                Ok(IOPubEvent::Error {
                    error: err.get_error()?.to_string()?,
                    context,
                })
            }
            Which::Warning(msg) => Ok(IOPubEvent::Warning {
                message: msg?.to_string()?,
            }),
        }
    }
}

// ============================================================================
// Cap'n Proto Serialization (Rust types -> garden_capnp)
// Used by chaosgarden to build response messages
// ============================================================================

impl GardenSnapshot {
    /// Write to Cap'n Proto builder.
    pub fn to_capnp(&self, builder: &mut garden_capnp::garden_snapshot::Builder) {
        builder.set_version(self.version);

        // Transport state
        let mut transport_builder = builder.reborrow().init_transport();
        transport_builder.set_playing(self.transport.playing);
        transport_builder.reborrow().init_position().set_value(self.transport.position);
        transport_builder.set_tempo(self.transport.tempo);

        // Regions
        let mut regions_builder = builder.reborrow().init_regions(self.regions.len() as u32);
        for (i, region) in self.regions.iter().enumerate() {
            region.to_capnp(&mut regions_builder.reborrow().get(i as u32));
        }

        // Nodes
        let mut nodes_builder = builder.reborrow().init_nodes(self.nodes.len() as u32);
        for (i, node) in self.nodes.iter().enumerate() {
            node.to_capnp(&mut nodes_builder.reborrow().get(i as u32));
        }

        // Edges
        let mut edges_builder = builder.reborrow().init_edges(self.edges.len() as u32);
        for (i, edge) in self.edges.iter().enumerate() {
            edge.to_capnp(&mut edges_builder.reborrow().get(i as u32));
        }

        // Latent jobs
        let mut jobs_builder = builder.reborrow().init_latent_jobs(self.latent_jobs.len() as u32);
        for (i, job) in self.latent_jobs.iter().enumerate() {
            job.to_capnp(&mut jobs_builder.reborrow().get(i as u32));
        }

        // Pending approvals
        let mut approvals_builder = builder.reborrow().init_pending_approvals(self.pending_approvals.len() as u32);
        for (i, approval) in self.pending_approvals.iter().enumerate() {
            approval.to_capnp(&mut approvals_builder.reborrow().get(i as u32));
        }

        // Outputs
        let mut outputs_builder = builder.reborrow().init_outputs(self.outputs.len() as u32);
        for (i, output) in self.outputs.iter().enumerate() {
            output.to_capnp(&mut outputs_builder.reborrow().get(i as u32));
        }

        // Inputs
        let mut inputs_builder = builder.reborrow().init_inputs(self.inputs.len() as u32);
        for (i, input) in self.inputs.iter().enumerate() {
            input.to_capnp(&mut inputs_builder.reborrow().get(i as u32));
        }

        // MIDI devices
        let mut midi_builder = builder.reborrow().init_midi_devices(self.midi_devices.len() as u32);
        for (i, device) in self.midi_devices.iter().enumerate() {
            device.to_capnp(&mut midi_builder.reborrow().get(i as u32));
        }

        // Tempo map
        self.tempo_map.to_capnp(&mut builder.reborrow().init_tempo_map());
    }
}

impl RegionSnapshot {
    pub fn to_capnp(&self, builder: &mut garden_capnp::region_snapshot::Builder) {
        builder.set_id(&self.id);
        builder.set_position(self.position);
        builder.set_duration(self.duration);

        builder.set_behavior_type(match self.behavior_type {
            BehaviorType::PlayContent => garden_capnp::BehaviorType::PlayContent,
            BehaviorType::Latent => garden_capnp::BehaviorType::Latent,
            BehaviorType::ApplyProcessing => garden_capnp::BehaviorType::ApplyProcessing,
            BehaviorType::EmitTrigger => garden_capnp::BehaviorType::EmitTrigger,
            BehaviorType::Custom => garden_capnp::BehaviorType::Custom,
        });

        builder.set_name(self.name.as_deref().unwrap_or(""));

        let mut tags_builder = builder.reborrow().init_tags(self.tags.len() as u32);
        for (i, tag) in self.tags.iter().enumerate() {
            tags_builder.set(i as u32, tag);
        }

        builder.set_content_hash(self.content_hash.as_deref().unwrap_or(""));

        builder.set_content_type(match self.content_type {
            Some(MediaType::Audio) => garden_capnp::ContentTypeEnum::Audio,
            Some(MediaType::Midi) => garden_capnp::ContentTypeEnum::Midi,
            Some(MediaType::Control) => garden_capnp::ContentTypeEnum::Control,
            None => garden_capnp::ContentTypeEnum::Audio, // Default
        });

        builder.set_latent_status(match self.latent_status {
            None => garden_capnp::LatentStatusEnum::None,
            Some(LatentStatus::Pending) => garden_capnp::LatentStatusEnum::Pending,
            Some(LatentStatus::Running) => garden_capnp::LatentStatusEnum::Running,
            Some(LatentStatus::Resolved) => garden_capnp::LatentStatusEnum::Resolved,
            Some(LatentStatus::Approved) => garden_capnp::LatentStatusEnum::Approved,
            Some(LatentStatus::Rejected) => garden_capnp::LatentStatusEnum::Rejected,
            Some(LatentStatus::Failed) => garden_capnp::LatentStatusEnum::Failed,
        });

        builder.set_latent_progress(self.latent_progress);
        builder.set_job_id(self.job_id.as_deref().unwrap_or(""));
        builder.set_generation_tool(self.generation_tool.as_deref().unwrap_or(""));

        builder.set_is_resolved(self.is_resolved);
        builder.set_is_approved(self.is_approved);
        builder.set_is_playable(self.is_playable);
        builder.set_is_alive(self.is_alive);
        builder.set_is_tombstoned(self.is_tombstoned);
    }
}

impl GraphNode {
    pub fn to_capnp(&self, builder: &mut garden_capnp::graph_node::Builder) {
        builder.set_id(&self.id);
        builder.set_name(&self.name);
        builder.set_type_id(&self.type_id);

        let mut inputs_builder = builder.reborrow().init_inputs(self.inputs.len() as u32);
        for (i, port) in self.inputs.iter().enumerate() {
            port.to_capnp(&mut inputs_builder.reborrow().get(i as u32));
        }

        let mut outputs_builder = builder.reborrow().init_outputs(self.outputs.len() as u32);
        for (i, port) in self.outputs.iter().enumerate() {
            port.to_capnp(&mut outputs_builder.reborrow().get(i as u32));
        }

        builder.set_latency_samples(self.latency_samples);
        builder.set_can_realtime(self.can_realtime);
        builder.set_can_offline(self.can_offline);
    }
}

impl Port {
    pub fn to_capnp(&self, builder: &mut garden_capnp::port::Builder) {
        builder.set_name(&self.name);
        builder.set_signal_type(match self.signal_type {
            SignalType::Audio => garden_capnp::SignalTypeEnum::Audio,
            SignalType::Midi => garden_capnp::SignalTypeEnum::Midi,
            SignalType::Control => garden_capnp::SignalTypeEnum::Control,
            SignalType::Trigger => garden_capnp::SignalTypeEnum::Trigger,
        });
    }
}

impl GraphEdge {
    pub fn to_capnp(&self, builder: &mut garden_capnp::graph_edge::Builder) {
        builder.set_source_id(&self.source_id);
        builder.set_source_port(&self.source_port);
        builder.set_dest_id(&self.dest_id);
        builder.set_dest_port(&self.dest_port);
    }
}

impl LatentJob {
    pub fn to_capnp(&self, builder: &mut garden_capnp::latent_job::Builder) {
        builder.set_id(&self.id);
        builder.set_region_id(&self.region_id);
        builder.set_tool(&self.tool);
        builder.set_progress(self.progress);
    }
}

impl ApprovalInfo {
    pub fn to_capnp(&self, builder: &mut garden_capnp::approval_info::Builder) {
        builder.set_region_id(&self.region_id);
        builder.set_content_hash(&self.content_hash);
        builder.set_content_type(match self.content_type {
            MediaType::Audio => garden_capnp::ContentTypeEnum::Audio,
            MediaType::Midi => garden_capnp::ContentTypeEnum::Midi,
            MediaType::Control => garden_capnp::ContentTypeEnum::Control,
        });
    }
}

impl AudioOutput {
    pub fn to_capnp(&self, builder: &mut garden_capnp::audio_output::Builder) {
        builder.set_id(&self.id);
        builder.set_name(&self.name);
        builder.set_channels(self.channels);
        if let Some(pw_id) = self.pw_node_id {
            builder.set_pw_node_id(pw_id);
            builder.set_has_pw_node_id(true);
        } else {
            builder.set_has_pw_node_id(false);
        }
    }
}

impl AudioInput {
    pub fn to_capnp(&self, builder: &mut garden_capnp::audio_input::Builder) {
        builder.set_id(&self.id);
        builder.set_name(&self.name);
        builder.set_channels(self.channels);
        builder.set_port_pattern(self.port_pattern.as_deref().unwrap_or(""));
        if let Some(pw_id) = self.pw_node_id {
            builder.set_pw_node_id(pw_id);
            builder.set_has_pw_node_id(true);
        } else {
            builder.set_has_pw_node_id(false);
        }
    }
}

impl MidiDeviceInfo {
    pub fn to_capnp(&self, builder: &mut garden_capnp::midi_device_info::Builder) {
        builder.set_id(&self.id);
        builder.set_name(&self.name);
        builder.set_direction(match self.direction {
            MidiDirection::Input => garden_capnp::MidiDirection::Input,
            MidiDirection::Output => garden_capnp::MidiDirection::Output,
        });
        if let Some(pw_id) = self.pw_node_id {
            builder.set_pw_node_id(pw_id);
            builder.set_has_pw_node_id(true);
        } else {
            builder.set_has_pw_node_id(false);
        }
    }
}

impl TempoMapSnapshot {
    pub fn to_capnp(&self, builder: &mut garden_capnp::tempo_map_snapshot::Builder) {
        builder.set_default_tempo(self.default_tempo);
        builder.set_ticks_per_beat(self.ticks_per_beat);

        let mut changes_builder = builder.reborrow().init_changes(self.changes.len() as u32);
        for (i, change) in self.changes.iter().enumerate() {
            let mut change_builder = changes_builder.reborrow().get(i as u32);
            change_builder.set_tick(change.tick);
            change_builder.set_tempo(change.tempo);
        }
    }
}

impl IOPubMessage {
    pub fn to_capnp(&self, builder: &mut garden_capnp::i_o_pub_message::Builder) {
        builder.set_version(self.version);
        builder.set_timestamp(self.timestamp);
        self.event.to_capnp(builder.reborrow().init_event());
    }
}

impl IOPubEvent {
    /// Write to Cap'n Proto builder. Takes ownership because union init methods consume self.
    pub fn to_capnp(&self, mut builder: garden_capnp::i_o_pub_event_union::Builder) {
        match self {
            IOPubEvent::StateChanged => builder.set_state_changed(()),
            IOPubEvent::PlaybackStarted => builder.set_playback_started(()),
            IOPubEvent::PlaybackStopped => builder.set_playback_stopped(()),
            IOPubEvent::PlaybackPosition { beat, second } => {
                let mut pos = builder.init_playback_position();
                pos.set_beat(*beat);
                pos.set_second(*second);
            }
            IOPubEvent::RegionCreated { region_id } => {
                builder.set_region_created(region_id);
            }
            IOPubEvent::RegionDeleted { region_id } => {
                builder.set_region_deleted(region_id);
            }
            IOPubEvent::RegionMoved { region_id, new_position } => {
                let mut moved = builder.init_region_moved();
                moved.set_region_id(region_id);
                moved.set_new_position(*new_position);
            }
            IOPubEvent::LatentStarted { region_id, job_id } => {
                let mut started = builder.init_latent_started();
                started.set_region_id(region_id);
                started.set_job_id(job_id);
            }
            IOPubEvent::LatentProgress { region_id, progress } => {
                let mut prog = builder.init_latent_progress();
                prog.set_region_id(region_id);
                prog.set_progress(*progress);
            }
            IOPubEvent::LatentResolved { region_id, artifact_id, content_hash } => {
                let mut resolved = builder.init_latent_resolved();
                resolved.set_region_id(region_id);
                resolved.set_artifact_id(artifact_id);
                resolved.set_content_hash(content_hash);
            }
            IOPubEvent::LatentFailed { region_id, error } => {
                let mut failed = builder.init_latent_failed();
                failed.set_region_id(region_id);
                failed.set_error(error);
            }
            IOPubEvent::LatentApproved { region_id } => {
                builder.set_latent_approved(region_id);
            }
            IOPubEvent::LatentRejected { region_id, reason } => {
                let mut rejected = builder.init_latent_rejected();
                rejected.set_region_id(region_id);
                rejected.set_reason(reason.as_deref().unwrap_or(""));
            }
            IOPubEvent::NodeAdded { node_id, name } => {
                let mut added = builder.init_node_added();
                added.set_node_id(node_id);
                added.set_name(name);
            }
            IOPubEvent::NodeRemoved { node_id } => {
                builder.set_node_removed(node_id);
            }
            IOPubEvent::ConnectionMade { source_id, source_port, dest_id, dest_port } => {
                let mut conn = builder.init_connection_made();
                conn.set_source_id(source_id);
                conn.set_source_port(source_port);
                conn.set_dest_id(dest_id);
                conn.set_dest_port(dest_port);
            }
            IOPubEvent::ConnectionBroken { source_id, source_port, dest_id, dest_port } => {
                let mut conn = builder.init_connection_broken();
                conn.set_source_id(source_id);
                conn.set_source_port(source_port);
                conn.set_dest_id(dest_id);
                conn.set_dest_port(dest_port);
            }
            IOPubEvent::AudioAttached { device_name, sample_rate, latency_frames } => {
                let mut attached = builder.init_audio_attached();
                attached.set_device_name(device_name);
                attached.set_sample_rate(*sample_rate);
                attached.set_latency_frames(*latency_frames);
            }
            IOPubEvent::AudioDetached => builder.set_audio_detached(()),
            IOPubEvent::AudioUnderrun { count } => builder.set_audio_underrun(*count),
            IOPubEvent::Error { error, context } => {
                let mut err = builder.init_error();
                err.set_error(error);
                err.set_context(context.as_deref().unwrap_or(""));
            }
            IOPubEvent::Warning { message } => {
                builder.set_warning(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iopub_event_cache_invalidation() {
        assert!(IOPubEvent::StateChanged.invalidates_cache());
        assert!(IOPubEvent::RegionCreated { region_id: "test".into() }.invalidates_cache());
        assert!(!IOPubEvent::PlaybackStarted.invalidates_cache());
        assert!(!IOPubEvent::LatentProgress { region_id: "test".into(), progress: 0.5 }.invalidates_cache());
    }

    #[test]
    fn test_behavior_type_serde() {
        let json = serde_json::to_string(&BehaviorType::PlayContent).unwrap();
        assert_eq!(json, "\"play_content\"");

        let parsed: BehaviorType = serde_json::from_str("\"latent\"").unwrap();
        assert_eq!(parsed, BehaviorType::Latent);
    }

    #[test]
    fn test_region_snapshot_serde() {
        let region = RegionSnapshot {
            id: "test-uuid".to_string(),
            position: 4.0,
            duration: 8.0,
            behavior_type: BehaviorType::PlayContent,
            name: Some("intro".to_string()),
            tags: vec!["jazzy".to_string()],
            content_hash: Some("abc123".to_string()),
            content_type: Some(MediaType::Midi),
            latent_status: None,
            latent_progress: 0.0,
            job_id: None,
            generation_tool: None,
            is_resolved: true,
            is_approved: true,
            is_playable: true,
            is_alive: true,
            is_tombstoned: false,
        };

        let json = serde_json::to_string(&region).unwrap();
        let parsed: RegionSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, region.id);
        assert_eq!(parsed.name, region.name);
    }
}
