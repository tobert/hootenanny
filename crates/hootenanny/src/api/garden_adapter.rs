//! GardenStateAdapter - Trustfall adapter using cached garden state
//!
//! Evaluates Trustfall queries locally using GardenSnapshot from GardenStateCache.
//! This eliminates JSON/GraphQL parsing from the chaosgarden real-time process.

use std::sync::Arc;

use hooteproto::garden_snapshot::{
    BehaviorType, GardenSnapshot, GraphEdge, GraphNode, LatentJob, LatentStatus, MediaType,
    MidiDeviceInfo, MidiDirection, Port, RegionSnapshot, SignalType,
};
use trustfall::{
    provider::{
        resolve_neighbors_with, resolve_property_with, AsVertex, ContextIterator,
        ContextOutcomeIterator, EdgeParameters, Typename, VertexIterator,
    },
    FieldValue, Schema,
};

/// Vertex types for the Trustfall adapter
#[derive(Debug, Clone)]
pub enum Vertex {
    Region(Arc<RegionSnapshot>),
    Node(Arc<GraphNode>),
    Port(Arc<Port>),
    Edge(Arc<GraphEdge>),
    Output(Arc<OutputVertex>),
    Input(Arc<InputVertex>),
    MidiDevice(Arc<MidiDeviceInfo>),
    Job(Arc<LatentJob>),
    Approval(Arc<ApprovalVertex>),
    TempoResult(f64),
    TimeResult(f64),
}

#[derive(Debug, Clone)]
pub struct OutputVertex {
    pub id: String,
    pub name: String,
    pub channels: u8,
    pub pw_node_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct InputVertex {
    pub id: String,
    pub name: String,
    pub channels: u8,
    pub port_pattern: Option<String>,
    pub pw_node_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ApprovalVertex {
    pub region_id: String,
    pub content_hash: String,
    pub content_type: MediaType,
}

impl Typename for Vertex {
    fn typename(&self) -> &'static str {
        match self {
            Self::Region(_) => "Region",
            Self::Node(_) => "Node",
            Self::Port(_) => "Port",
            Self::Edge(_) => "Edge",
            Self::Output(_) => "Output",
            Self::Input(_) => "Input",
            Self::MidiDevice(_) => "MidiDevice",
            Self::Job(_) => "Job",
            Self::Approval(_) => "Approval",
            Self::TempoResult(_) => "TempoResult",
            Self::TimeResult(_) => "TimeResult",
        }
    }
}

/// GraphQL schema for garden queries (matches chaosgarden's schema.graphql)
pub const SCHEMA: &str = r#"
schema {
    query: Query
}

type Query {
    # Region queries
    Region(id: String): [Region!]!
    RegionInRange(start: Float!, end: Float!): [Region!]!
    RegionByTag(tag: String!): [Region!]!
    LatentRegion: [Region!]!
    PlayableRegion: [Region!]!

    # Node queries
    Node(id: String, type_prefix: String): [Node!]!

    # Graph structure
    Edge: [Edge!]!

    # I/O queries
    Output: [Output!]!
    Input: [Input!]!
    MidiDevice(direction: String): [MidiDevice!]!

    # Time queries - these return scalar values wrapped in a type
    TempoAt(beat: Float!): TempoResult!
    BeatToSecond(beat: Float!): TimeResult!
    SecondToBeat(second: Float!): TimeResult!

    # Job queries
    RunningJob: [Job!]!
    PendingApproval: [Approval!]!
}

type Region {
    id: ID!
    position: Float!
    duration: Float!
    end: Float!
    behavior_type: String!
    name: String
    tags: [String!]!

    # Latent state (null if not latent)
    latent_status: String
    latent_progress: Float
    job_id: String

    # Resolved content
    is_resolved: Boolean!
    is_approved: Boolean!
    is_playable: Boolean!
    content_hash: String
    content_type: String

    # Generation info (for latent regions)
    generation_tool: String

    # Lifecycle
    is_alive: Boolean!
    is_tombstoned: Boolean!
}

type Node {
    id: ID!
    name: String!
    type_id: String!
    inputs: [Port!]!
    outputs: [Port!]!
    latency_samples: Int!

    # Capabilities
    can_realtime: Boolean!
    can_offline: Boolean!

    # Graph traversal
    upstream: [Node!]!
    downstream: [Node!]!
}

type Port {
    name: String!
    signal_type: String!
}

type Edge {
    source_id: ID!
    source_port: String!
    dest_id: ID!
    dest_port: String!
}

type Output {
    id: ID!
    name: String!
    channels: Int!
    pw_node_id: Int
}

type Input {
    id: ID!
    name: String!
    channels: Int!
    port_pattern: String
    pw_node_id: Int
}

type MidiDevice {
    id: ID!
    name: String!
    direction: String!
    pw_node_id: Int
}

type Job {
    id: ID!
    region_id: ID!
    tool: String!
    progress: Float!
}

type Approval {
    region_id: ID!
    content_hash: String!
    content_type: String!
}

type TempoResult {
    bpm: Float!
}

type TimeResult {
    value: Float!
}
"#;

/// Trustfall adapter that evaluates queries against a GardenSnapshot
pub struct GardenStateAdapter {
    snapshot: GardenSnapshot,
    schema: Arc<Schema>,
}

impl GardenStateAdapter {
    /// Create a new adapter from a snapshot
    pub fn new(snapshot: GardenSnapshot) -> anyhow::Result<Self> {
        let schema = Arc::new(Schema::parse(SCHEMA)?);
        Ok(Self { snapshot, schema })
    }

    /// Get the schema
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Get schema as Arc for query execution
    pub fn schema_arc(&self) -> Arc<Schema> {
        Arc::clone(&self.schema)
    }

    fn get_regions(&self) -> Vec<Arc<RegionSnapshot>> {
        self.snapshot.regions.iter().cloned().map(Arc::new).collect()
    }

    fn get_nodes(&self) -> Vec<Arc<GraphNode>> {
        self.snapshot.nodes.iter().cloned().map(Arc::new).collect()
    }

    fn get_edges(&self) -> Vec<Arc<GraphEdge>> {
        self.snapshot.edges.iter().cloned().map(Arc::new).collect()
    }
}

impl<'a> trustfall::provider::BasicAdapter<'a> for GardenStateAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name {
            "Region" => {
                let id_filter = parameters.get("id").and_then(|v| v.as_str());

                let regions = self.get_regions();
                let filtered: Vec<_> = if let Some(id) = id_filter {
                    regions.into_iter().filter(|r| r.id == id).collect()
                } else {
                    regions
                };

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "RegionInRange" => {
                let start = parameters
                    .get("start")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let end = parameters
                    .get("end")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(f64::MAX);

                let regions = self.get_regions();
                let filtered: Vec<_> = regions
                    .into_iter()
                    .filter(|r| {
                        let region_end = r.position + r.duration;
                        r.position < end && region_end > start
                    })
                    .collect();

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "RegionByTag" => {
                let tag = parameters.get("tag").and_then(|v| v.as_str()).unwrap_or("");

                let regions = self.get_regions();
                let filtered: Vec<_> = regions
                    .into_iter()
                    .filter(|r| r.tags.iter().any(|t| t == tag))
                    .collect();

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "LatentRegion" => {
                let regions = self.get_regions();
                let filtered: Vec<_> = regions
                    .into_iter()
                    .filter(|r| r.behavior_type == BehaviorType::Latent)
                    .collect();

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "PlayableRegion" => {
                let regions = self.get_regions();
                let filtered: Vec<_> = regions
                    .into_iter()
                    .filter(|r| r.is_playable)
                    .collect();

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "Node" => {
                let id_filter = parameters.get("id").and_then(|v| v.as_str());
                let type_prefix = parameters.get("type_prefix").and_then(|v| v.as_str());

                let nodes = self.get_nodes();
                let filtered: Vec<_> = nodes
                    .into_iter()
                    .filter(|n| {
                        if let Some(id) = id_filter {
                            if n.id != id {
                                return false;
                            }
                        }
                        if let Some(prefix) = type_prefix {
                            if !n.type_id.starts_with(prefix) {
                                return false;
                            }
                        }
                        true
                    })
                    .collect();

                Box::new(filtered.into_iter().map(Vertex::Node))
            }
            "Edge" => {
                let edges = self.get_edges();
                Box::new(edges.into_iter().map(Vertex::Edge))
            }
            "Output" => {
                let outputs: Vec<_> = self
                    .snapshot
                    .outputs
                    .iter()
                    .map(|o| {
                        Arc::new(OutputVertex {
                            id: o.id.clone(),
                            name: o.name.clone(),
                            channels: o.channels,
                            pw_node_id: o.pw_node_id,
                        })
                    })
                    .collect();

                Box::new(outputs.into_iter().map(Vertex::Output))
            }
            "Input" => {
                let inputs: Vec<_> = self
                    .snapshot
                    .inputs
                    .iter()
                    .map(|i| {
                        Arc::new(InputVertex {
                            id: i.id.clone(),
                            name: i.name.clone(),
                            channels: i.channels,
                            port_pattern: i.port_pattern.clone(),
                            pw_node_id: i.pw_node_id,
                        })
                    })
                    .collect();

                Box::new(inputs.into_iter().map(Vertex::Input))
            }
            "MidiDevice" => {
                let direction_filter = parameters.get("direction").and_then(|v| v.as_str());

                let devices: Vec<_> = self
                    .snapshot
                    .midi_devices
                    .iter()
                    .filter(|d| {
                        if let Some(dir_str) = direction_filter {
                            let expected_dir = match dir_str {
                                "input" => MidiDirection::Input,
                                "output" => MidiDirection::Output,
                                _ => return true,
                            };
                            d.direction == expected_dir
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .map(Arc::new)
                    .collect();

                Box::new(devices.into_iter().map(Vertex::MidiDevice))
            }
            "TempoAt" => {
                let _beat = parameters
                    .get("beat")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                // Use snapshot's tempo (we don't have full tempo map)
                let bpm = self.snapshot.tempo_map.default_tempo;
                Box::new(std::iter::once(Vertex::TempoResult(bpm)))
            }
            "BeatToSecond" => {
                let beat = parameters
                    .get("beat")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                // Simple conversion using default tempo
                let bpm = self.snapshot.tempo_map.default_tempo;
                let seconds = beat * 60.0 / bpm;
                Box::new(std::iter::once(Vertex::TimeResult(seconds)))
            }
            "SecondToBeat" => {
                let second = parameters
                    .get("second")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                // Simple conversion using default tempo
                let bpm = self.snapshot.tempo_map.default_tempo;
                let beats = second * bpm / 60.0;
                Box::new(std::iter::once(Vertex::TimeResult(beats)))
            }
            "RunningJob" => {
                let jobs: Vec<_> = self
                    .snapshot
                    .latent_jobs
                    .iter()
                    .cloned()
                    .map(Arc::new)
                    .collect();
                Box::new(jobs.into_iter().map(Vertex::Job))
            }
            "PendingApproval" => {
                let approvals: Vec<_> = self
                    .snapshot
                    .pending_approvals
                    .iter()
                    .map(|a| {
                        Arc::new(ApprovalVertex {
                            region_id: a.region_id.clone(),
                            content_hash: a.content_hash.clone(),
                            content_type: a.content_type.clone(),
                        })
                    })
                    .collect();
                Box::new(approvals.into_iter().map(Vertex::Approval))
            }
            _ => Box::new(std::iter::empty()),
        }
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeIterator<'a, V, FieldValue> {
        match (type_name, property_name) {
            // Region properties
            ("Region", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::String(r.id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "position") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Float64(r.position)
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "duration") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Float64(r.duration)
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "end") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Float64(r.position + r.duration)
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "behavior_type") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    let s = match r.behavior_type {
                        BehaviorType::PlayContent => "play_content",
                        BehaviorType::Latent => "latent",
                        BehaviorType::ApplyProcessing => "apply_processing",
                        BehaviorType::EmitTrigger => "emit_trigger",
                        BehaviorType::Custom => "custom",
                    };
                    FieldValue::String(s.into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    r.name.clone().map_or(FieldValue::Null, |s| FieldValue::String(s.into()))
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "tags") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::List(
                        r.tags
                            .iter()
                            .map(|t| FieldValue::String(t.clone().into()))
                            .collect::<Vec<_>>()
                            .into(),
                    )
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "latent_status") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    r.latent_status.as_ref().map_or(FieldValue::Null, |s| {
                        let status_str = match s {
                            LatentStatus::Pending => "pending",
                            LatentStatus::Running => "running",
                            LatentStatus::Resolved => "resolved",
                            LatentStatus::Approved => "approved",
                            LatentStatus::Rejected => "rejected",
                            LatentStatus::Failed => "failed",
                        };
                        FieldValue::String(status_str.into())
                    })
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "latent_progress") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Float64(r.latent_progress as f64)
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "job_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    r.job_id.clone().map_or(FieldValue::Null, |s| FieldValue::String(s.into()))
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "is_resolved") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_resolved)
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "is_approved") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_approved)
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "is_playable") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_playable)
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "content_hash") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    r.content_hash.clone().map_or(FieldValue::Null, |s| FieldValue::String(s.into()))
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "content_type") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    r.content_type.as_ref().map_or(FieldValue::Null, |ct| {
                        let s = match ct {
                            MediaType::Audio => "audio",
                            MediaType::Midi => "midi",
                            MediaType::Control => "control",
                        };
                        FieldValue::String(s.into())
                    })
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "generation_tool") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    r.generation_tool.clone().map_or(FieldValue::Null, |s| FieldValue::String(s.into()))
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "is_alive") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_alive)
                } else {
                    FieldValue::Null
                }
            }),
            ("Region", "is_tombstoned") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_tombstoned)
                } else {
                    FieldValue::Null
                }
            }),

            // Node properties
            ("Node", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::String(n.id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Node", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::String(n.name.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Node", "type_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::String(n.type_id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Node", "latency_samples") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::Int64(n.latency_samples as i64)
                } else {
                    FieldValue::Null
                }
            }),
            ("Node", "can_realtime") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::Boolean(n.can_realtime)
                } else {
                    FieldValue::Null
                }
            }),
            ("Node", "can_offline") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::Boolean(n.can_offline)
                } else {
                    FieldValue::Null
                }
            }),

            // Port properties
            ("Port", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Port(p) = v {
                    FieldValue::String(p.name.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Port", "signal_type") => resolve_property_with(contexts, |v| {
                if let Vertex::Port(p) = v {
                    let s = match p.signal_type {
                        SignalType::Audio => "audio",
                        SignalType::Midi => "midi",
                        SignalType::Control => "control",
                        SignalType::Trigger => "trigger",
                    };
                    FieldValue::String(s.into())
                } else {
                    FieldValue::Null
                }
            }),

            // Edge properties
            ("Edge", "source_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Edge(e) = v {
                    FieldValue::String(e.source_id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Edge", "source_port") => resolve_property_with(contexts, |v| {
                if let Vertex::Edge(e) = v {
                    FieldValue::String(e.source_port.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Edge", "dest_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Edge(e) = v {
                    FieldValue::String(e.dest_id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Edge", "dest_port") => resolve_property_with(contexts, |v| {
                if let Vertex::Edge(e) = v {
                    FieldValue::String(e.dest_port.clone().into())
                } else {
                    FieldValue::Null
                }
            }),

            // Output properties
            ("Output", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Output(o) = v {
                    FieldValue::String(o.id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Output", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Output(o) = v {
                    FieldValue::String(o.name.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Output", "channels") => resolve_property_with(contexts, |v| {
                if let Vertex::Output(o) = v {
                    FieldValue::Int64(o.channels as i64)
                } else {
                    FieldValue::Null
                }
            }),
            ("Output", "pw_node_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Output(o) = v {
                    o.pw_node_id
                        .map(|id| FieldValue::Int64(id as i64))
                        .unwrap_or(FieldValue::Null)
                } else {
                    FieldValue::Null
                }
            }),

            // Input properties
            ("Input", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    FieldValue::String(i.id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Input", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    FieldValue::String(i.name.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Input", "channels") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    FieldValue::Int64(i.channels as i64)
                } else {
                    FieldValue::Null
                }
            }),
            ("Input", "port_pattern") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    i.port_pattern
                        .clone()
                        .map(|s| FieldValue::String(s.into()))
                        .unwrap_or(FieldValue::Null)
                } else {
                    FieldValue::Null
                }
            }),
            ("Input", "pw_node_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    i.pw_node_id
                        .map(|id| FieldValue::Int64(id as i64))
                        .unwrap_or(FieldValue::Null)
                } else {
                    FieldValue::Null
                }
            }),

            // MidiDevice properties
            ("MidiDevice", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::MidiDevice(m) = v {
                    FieldValue::String(m.id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("MidiDevice", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::MidiDevice(m) = v {
                    FieldValue::String(m.name.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("MidiDevice", "direction") => resolve_property_with(contexts, |v| {
                if let Vertex::MidiDevice(m) = v {
                    let s = match m.direction {
                        MidiDirection::Input => "input",
                        MidiDirection::Output => "output",
                    };
                    FieldValue::String(s.into())
                } else {
                    FieldValue::Null
                }
            }),
            ("MidiDevice", "pw_node_id") => resolve_property_with(contexts, |v| {
                if let Vertex::MidiDevice(m) = v {
                    m.pw_node_id
                        .map(|id| FieldValue::Int64(id as i64))
                        .unwrap_or(FieldValue::Null)
                } else {
                    FieldValue::Null
                }
            }),

            // Job properties
            ("Job", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Job(j) = v {
                    FieldValue::String(j.id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Job", "region_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Job(j) = v {
                    FieldValue::String(j.region_id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Job", "tool") => resolve_property_with(contexts, |v| {
                if let Vertex::Job(j) = v {
                    FieldValue::String(j.tool.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Job", "progress") => resolve_property_with(contexts, |v| {
                if let Vertex::Job(j) = v {
                    FieldValue::Float64(j.progress as f64)
                } else {
                    FieldValue::Null
                }
            }),

            // Approval properties
            ("Approval", "region_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Approval(a) = v {
                    FieldValue::String(a.region_id.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Approval", "content_hash") => resolve_property_with(contexts, |v| {
                if let Vertex::Approval(a) = v {
                    FieldValue::String(a.content_hash.clone().into())
                } else {
                    FieldValue::Null
                }
            }),
            ("Approval", "content_type") => resolve_property_with(contexts, |v| {
                if let Vertex::Approval(a) = v {
                    let s = match a.content_type {
                        MediaType::Audio => "audio",
                        MediaType::Midi => "midi",
                        MediaType::Control => "control",
                    };
                    FieldValue::String(s.into())
                } else {
                    FieldValue::Null
                }
            }),

            // TempoResult properties
            ("TempoResult", "bpm") => resolve_property_with(contexts, |v| {
                if let Vertex::TempoResult(bpm) = v {
                    FieldValue::Float64(*bpm)
                } else {
                    FieldValue::Null
                }
            }),

            // TimeResult properties
            ("TimeResult", "value") => resolve_property_with(contexts, |v| {
                if let Vertex::TimeResult(value) = v {
                    FieldValue::Float64(*value)
                } else {
                    FieldValue::Null
                }
            }),

            _ => resolve_property_with(contexts, |_| FieldValue::Null),
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &str,
        edge_name: &str,
        _parameters: &EdgeParameters,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Self::Vertex>> {
        match (type_name, edge_name) {
            ("Node", "inputs") => {
                resolve_neighbors_with(contexts, |v| {
                    if let Vertex::Node(n) = v {
                        let ports: Vec<_> = n.inputs.iter().cloned().map(Arc::new).map(Vertex::Port).collect();
                        Box::new(ports.into_iter())
                    } else {
                        Box::new(std::iter::empty())
                    }
                })
            }
            ("Node", "outputs") => {
                resolve_neighbors_with(contexts, |v| {
                    if let Vertex::Node(n) = v {
                        let ports: Vec<_> = n.outputs.iter().cloned().map(Arc::new).map(Vertex::Port).collect();
                        Box::new(ports.into_iter())
                    } else {
                        Box::new(std::iter::empty())
                    }
                })
            }
            // Graph traversal (upstream/downstream) requires edge data
            ("Node", "upstream") | ("Node", "downstream") => {
                let edges = self.snapshot.edges.clone();
                let nodes = self.snapshot.nodes.clone();
                let is_upstream = edge_name == "upstream";

                resolve_neighbors_with(contexts, move |v| {
                    if let Vertex::Node(n) = v {
                        let node_id = &n.id;
                        let connected_ids: Vec<_> = edges
                            .iter()
                            .filter_map(|e| {
                                if is_upstream && e.dest_id == *node_id {
                                    Some(e.source_id.clone())
                                } else if !is_upstream && e.source_id == *node_id {
                                    Some(e.dest_id.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();

                        let connected_nodes: Vec<_> = nodes
                            .iter()
                            .filter(|node| connected_ids.contains(&node.id))
                            .cloned()
                            .map(Arc::new)
                            .map(Vertex::Node)
                            .collect();

                        Box::new(connected_nodes.into_iter())
                    } else {
                        Box::new(std::iter::empty())
                    }
                })
            }
            _ => resolve_neighbors_with(contexts, |_| Box::new(std::iter::empty())),
        }
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        _type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'a, V, bool> {
        // No coercion needed for this simple schema - all types are concrete
        let target = coerce_to_type.to_string();
        Box::new(contexts.map(move |ctx| {
            let can_coerce = ctx.active_vertex().map_or(false, |v| v.typename() == target);
            (ctx, can_coerce)
        }))
    }
}

/// Execute a Trustfall query against a garden snapshot
pub fn execute_query(
    snapshot: GardenSnapshot,
    query: &str,
    variables: std::collections::HashMap<String, serde_json::Value>,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let adapter = Arc::new(GardenStateAdapter::new(snapshot)?);
    let schema = adapter.schema_arc();

    // Convert JSON variables to Trustfall FieldValues
    let vars: std::collections::BTreeMap<std::sync::Arc<str>, _> = variables
        .into_iter()
        .map(|(k, v)| (std::sync::Arc::from(k.as_str()), json_to_field_value(&v)))
        .collect();

    let results = trustfall::execute_query(&schema, adapter, query, vars)?;

    // Convert results to JSON
    let rows: Vec<serde_json::Value> = results
        .map(|row| {
            let obj: serde_json::Map<_, _> = row
                .into_iter()
                .map(|(k, v)| (k.to_string(), field_value_to_json(&v)))
                .collect();
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(rows)
}

fn json_to_field_value(v: &serde_json::Value) -> FieldValue {
    match v {
        serde_json::Value::Null => FieldValue::Null,
        serde_json::Value::Bool(b) => FieldValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                FieldValue::Int64(i)
            } else if let Some(u) = n.as_u64() {
                FieldValue::Uint64(u)
            } else if let Some(f) = n.as_f64() {
                FieldValue::Float64(f)
            } else {
                FieldValue::Null
            }
        }
        serde_json::Value::String(s) => FieldValue::String(s.clone().into()),
        serde_json::Value::Array(arr) => {
            let items: Vec<_> = arr.iter().map(json_to_field_value).collect();
            FieldValue::List(items.into())
        }
        serde_json::Value::Object(_) => FieldValue::Null,
    }
}

fn field_value_to_json(v: &FieldValue) -> serde_json::Value {
    match v {
        FieldValue::Null => serde_json::Value::Null,
        FieldValue::Boolean(b) => serde_json::Value::Bool(*b),
        FieldValue::Int64(i) => serde_json::Value::Number((*i).into()),
        FieldValue::Uint64(u) => serde_json::Value::Number((*u).into()),
        FieldValue::Float64(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        FieldValue::String(s) => serde_json::Value::String(s.to_string()),
        FieldValue::List(items) => {
            let arr: Vec<_> = items.iter().map(field_value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        _ => serde_json::Value::Null,
    }
}
