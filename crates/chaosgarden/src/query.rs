//! Trustfall adapter for querying chaosgarden state
//!
//! Exposes regions, nodes, edges, I/O devices, and time conversions to any
//! participant via GraphQL-like queries.

use std::sync::{Arc, RwLock};

use trustfall::{
    provider::{
        resolve_neighbors_with, resolve_property_with, AsVertex, ContextIterator,
        ContextOutcomeIterator, EdgeParameters, Typename, VertexIterator,
    },
    FieldValue, Schema,
};
use uuid::Uuid;

use crate::external_io::{ExternalIOManager, MidiDirection};
use crate::graph::Graph;
use crate::primitives::{
    Beat, Behavior, ContentType, LatentStatus, NodeDescriptor, Region, Second, SignalType, TempoMap,
};

/// Vertex types for the Trustfall adapter
#[derive(Debug, Clone)]
pub enum Vertex {
    Region(Arc<Region>),
    Node(Arc<NodeVertex>),
    Port(Arc<PortVertex>),
    Edge(Arc<EdgeVertex>),
    Output(Arc<OutputVertex>),
    Input(Arc<InputVertex>),
    MidiDevice(Arc<MidiDeviceVertex>),
    Job(Arc<JobVertex>),
    Approval(Arc<ApprovalVertex>),
    TempoResult(f64),
    TimeResult(f64),
}

#[derive(Debug, Clone)]
pub struct NodeVertex {
    pub descriptor: NodeDescriptor,
}

#[derive(Debug, Clone)]
pub struct PortVertex {
    pub name: String,
    pub signal_type: SignalType,
}

#[derive(Debug, Clone)]
pub struct EdgeVertex {
    pub source_id: Uuid,
    pub source_port: String,
    pub dest_id: Uuid,
    pub dest_port: String,
}

#[derive(Debug, Clone)]
pub struct OutputVertex {
    pub id: Uuid,
    pub name: String,
    pub channels: u8,
    pub pw_node_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct InputVertex {
    pub id: Uuid,
    pub name: String,
    pub channels: u8,
    pub port_pattern: Option<String>,
    pub pw_node_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct MidiDeviceVertex {
    pub id: Uuid,
    pub name: String,
    pub direction: MidiDirection,
    pub pw_node_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct JobVertex {
    pub id: String,
    pub region_id: Uuid,
    pub tool: String,
    pub progress: f32,
}

#[derive(Debug, Clone)]
pub struct ApprovalVertex {
    pub region_id: Uuid,
    pub content_hash: String,
    pub content_type: ContentType,
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

/// Trustfall adapter for chaosgarden queries
pub struct ChaosgardenAdapter {
    regions: Arc<RwLock<Vec<Region>>>,
    graph: Arc<RwLock<Graph>>,
    io_manager: Option<Arc<RwLock<ExternalIOManager>>>,
    tempo_map: Arc<TempoMap>,
    schema: Arc<Schema>,
}

impl ChaosgardenAdapter {
    /// Create a new adapter
    pub fn new(
        regions: Arc<RwLock<Vec<Region>>>,
        graph: Arc<RwLock<Graph>>,
        tempo_map: Arc<TempoMap>,
    ) -> anyhow::Result<Self> {
        let schema_text = include_str!("schema.graphql");
        let schema = Arc::new(Schema::parse(schema_text)?);
        Ok(Self {
            regions,
            graph,
            io_manager: None,
            tempo_map,
            schema,
        })
    }

    /// Create adapter with I/O manager for device queries
    pub fn with_io_manager(
        regions: Arc<RwLock<Vec<Region>>>,
        graph: Arc<RwLock<Graph>>,
        tempo_map: Arc<TempoMap>,
        io_manager: Arc<RwLock<ExternalIOManager>>,
    ) -> anyhow::Result<Self> {
        let schema_text = include_str!("schema.graphql");
        let schema = Arc::new(Schema::parse(schema_text)?);
        Ok(Self {
            regions,
            graph,
            io_manager: Some(io_manager),
            tempo_map,
            schema,
        })
    }

    /// Get the schema
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Get schema as Arc for query execution
    pub fn schema_arc(&self) -> Arc<Schema> {
        self.schema.clone()
    }

    fn get_regions(&self) -> Vec<Arc<Region>> {
        self.regions
            .read()
            .map(|r| r.iter().cloned().map(Arc::new).collect())
            .unwrap_or_default()
    }

    fn get_nodes(&self) -> Vec<Arc<NodeVertex>> {
        self.graph
            .read()
            .map(|g| {
                let snapshot = g.snapshot();
                snapshot
                    .nodes
                    .into_iter()
                    .map(|desc| Arc::new(NodeVertex { descriptor: desc }))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_edges(&self) -> Vec<Arc<EdgeVertex>> {
        self.graph
            .read()
            .map(|g| {
                let snapshot = g.snapshot();
                snapshot
                    .edges
                    .into_iter()
                    .map(|e| {
                        Arc::new(EdgeVertex {
                            source_id: e.source_id,
                            source_port: e.source_port,
                            dest_id: e.dest_id,
                            dest_port: e.dest_port,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl<'a> trustfall::provider::BasicAdapter<'a> for ChaosgardenAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name {
            "Region" => {
                let id_filter = parameters
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok());

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
                    .filter(|r| r.position.0 < end && r.end().0 > start)
                    .collect();

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "RegionByTag" => {
                let tag = parameters.get("tag").and_then(|v| v.as_str()).unwrap_or("");

                let regions = self.get_regions();
                let filtered: Vec<_> = regions
                    .into_iter()
                    .filter(|r| r.metadata.tags.iter().any(|t| t == tag))
                    .collect();

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "LatentRegion" => {
                let regions = self.get_regions();
                let filtered: Vec<_> = regions.into_iter().filter(|r| r.is_latent()).collect();

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "PlayableRegion" => {
                let regions = self.get_regions();
                let filtered: Vec<_> = regions.into_iter().filter(|r| r.is_playable()).collect();

                Box::new(filtered.into_iter().map(Vertex::Region))
            }
            "Node" => {
                let id_filter = parameters
                    .get("id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok());
                let type_prefix = parameters.get("type_prefix").and_then(|v| v.as_str());

                let nodes = self.get_nodes();
                let filtered: Vec<_> = nodes
                    .into_iter()
                    .filter(|n| {
                        if let Some(id) = id_filter {
                            if n.descriptor.id != id {
                                return false;
                            }
                        }
                        if let Some(prefix) = type_prefix {
                            if !n.descriptor.type_id.starts_with(prefix) {
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
                    .io_manager
                    .as_ref()
                    .and_then(|io| io.read().ok())
                    .map(|io| {
                        io.outputs()
                            .map(|o| {
                                Arc::new(OutputVertex {
                                    id: o.id,
                                    name: o.name.clone(),
                                    channels: o.channels,
                                    pw_node_id: o.pw_node_id,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                Box::new(outputs.into_iter().map(Vertex::Output))
            }
            "Input" => {
                let inputs: Vec<_> = self
                    .io_manager
                    .as_ref()
                    .and_then(|io| io.read().ok())
                    .map(|io| {
                        io.inputs()
                            .map(|i| {
                                Arc::new(InputVertex {
                                    id: i.id,
                                    name: i.name.clone(),
                                    channels: i.channels,
                                    port_pattern: i.port_pattern.clone(),
                                    pw_node_id: i.pw_node_id,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                Box::new(inputs.into_iter().map(Vertex::Input))
            }
            "MidiDevice" => {
                let direction_filter = parameters.get("direction").and_then(|v| v.as_str());

                let devices: Vec<_> = self
                    .io_manager
                    .as_ref()
                    .and_then(|io| io.read().ok())
                    .map(|io| {
                        io.midi_devices()
                            .filter(|d| {
                                if let Some(dir) = direction_filter {
                                    let d_dir = match d.direction {
                                        MidiDirection::Input => "input",
                                        MidiDirection::Output => "output",
                                    };
                                    d_dir == dir
                                } else {
                                    true
                                }
                            })
                            .map(|d| {
                                Arc::new(MidiDeviceVertex {
                                    id: d.id,
                                    name: d.name.clone(),
                                    direction: d.direction,
                                    pw_node_id: d.pw_node_id,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                Box::new(devices.into_iter().map(Vertex::MidiDevice))
            }
            "TempoAt" => {
                let beat = parameters
                    .get("beat")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let tick = self.tempo_map.beat_to_tick(Beat(beat));
                let bpm = self.tempo_map.tempo_at(tick);

                Box::new(std::iter::once(Vertex::TempoResult(bpm)))
            }
            "BeatToSecond" => {
                let beat = parameters
                    .get("beat")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let tick = self.tempo_map.beat_to_tick(Beat(beat));
                let second = self.tempo_map.tick_to_second(tick);

                Box::new(std::iter::once(Vertex::TimeResult(second.0)))
            }
            "SecondToBeat" => {
                let second = parameters
                    .get("second")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let tick = self.tempo_map.second_to_tick(Second(second));
                let beat = self.tempo_map.tick_to_beat(tick);

                Box::new(std::iter::once(Vertex::TimeResult(beat.0)))
            }
            "RunningJob" => {
                let regions = self.get_regions();
                let jobs: Vec<_> = regions
                    .into_iter()
                    .filter_map(|r| {
                        if let Behavior::Latent { tool, state, .. } = &r.behavior {
                            if state.status == LatentStatus::Running {
                                return Some(Arc::new(JobVertex {
                                    id: state.job_id.clone().unwrap_or_default(),
                                    region_id: r.id,
                                    tool: tool.clone(),
                                    progress: state.progress,
                                }));
                            }
                        }
                        None
                    })
                    .collect();

                Box::new(jobs.into_iter().map(Vertex::Job))
            }
            "PendingApproval" => {
                let regions = self.get_regions();
                let approvals: Vec<_> = regions
                    .into_iter()
                    .filter_map(|r| {
                        if let Behavior::Latent { state, .. } = &r.behavior {
                            if state.status == LatentStatus::Resolved {
                                if let Some(resolved) = &state.resolved {
                                    return Some(Arc::new(ApprovalVertex {
                                        region_id: r.id,
                                        content_hash: resolved.content_hash.clone(),
                                        content_type: resolved.content_type,
                                    }));
                                }
                            }
                        }
                        None
                    })
                    .collect();

                Box::new(approvals.into_iter().map(Vertex::Approval))
            }
            _ => unreachable!("Unknown starting edge: {edge_name}"),
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
                    FieldValue::String(r.id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("Region", "position") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Float64(r.position.0)
                } else {
                    unreachable!()
                }
            }),
            ("Region", "duration") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Float64(r.duration.0)
                } else {
                    unreachable!()
                }
            }),
            ("Region", "end") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Float64(r.end().0)
                } else {
                    unreachable!()
                }
            }),
            ("Region", "behavior_type") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    let btype = match &r.behavior {
                        Behavior::PlayContent { .. } => "play_content",
                        Behavior::Latent { .. } => "latent",
                        Behavior::ApplyProcessing { .. } => "apply_processing",
                        Behavior::EmitTrigger { .. } => "emit_trigger",
                        Behavior::Custom { .. } => "custom",
                    };
                    FieldValue::String(btype.into())
                } else {
                    unreachable!()
                }
            }),
            ("Region", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    r.metadata
                        .name
                        .as_ref()
                        .map(|n| FieldValue::String(n.clone().into()))
                        .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("Region", "tags") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    let tags: Vec<FieldValue> = r
                        .metadata
                        .tags
                        .iter()
                        .map(|t| FieldValue::String(t.clone().into()))
                        .collect();
                    FieldValue::List(tags.into())
                } else {
                    unreachable!()
                }
            }),
            ("Region", "latent_status") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    if let Behavior::Latent { state, .. } = &r.behavior {
                        let status = match state.status {
                            LatentStatus::Pending => "pending",
                            LatentStatus::Running => "running",
                            LatentStatus::Resolved => "resolved",
                            LatentStatus::Approved => "approved",
                            LatentStatus::Rejected => "rejected",
                            LatentStatus::Failed => "failed",
                        };
                        FieldValue::String(status.into())
                    } else {
                        FieldValue::Null
                    }
                } else {
                    unreachable!()
                }
            }),
            ("Region", "latent_progress") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    if let Behavior::Latent { state, .. } = &r.behavior {
                        FieldValue::Float64(state.progress as f64)
                    } else {
                        FieldValue::Null
                    }
                } else {
                    unreachable!()
                }
            }),
            ("Region", "job_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    if let Behavior::Latent { state, .. } = &r.behavior {
                        state
                            .job_id
                            .as_ref()
                            .map(|id| FieldValue::String(id.clone().into()))
                            .unwrap_or(FieldValue::Null)
                    } else {
                        FieldValue::Null
                    }
                } else {
                    unreachable!()
                }
            }),
            ("Region", "is_resolved") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_resolved())
                } else {
                    unreachable!()
                }
            }),
            ("Region", "is_approved") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_approved())
                } else {
                    unreachable!()
                }
            }),
            ("Region", "is_playable") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_playable())
                } else {
                    unreachable!()
                }
            }),
            ("Region", "content_hash") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    match &r.behavior {
                        Behavior::PlayContent { content_hash, .. } => {
                            FieldValue::String(content_hash.clone().into())
                        }
                        Behavior::Latent { state, .. } => state
                            .resolved
                            .as_ref()
                            .map(|res| FieldValue::String(res.content_hash.clone().into()))
                            .unwrap_or(FieldValue::Null),
                        _ => FieldValue::Null,
                    }
                } else {
                    unreachable!()
                }
            }),
            ("Region", "content_type") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    let ct = match &r.behavior {
                        Behavior::PlayContent { content_type, .. } => Some(content_type),
                        Behavior::Latent { state, .. } => {
                            state.resolved.as_ref().map(|res| &res.content_type)
                        }
                        _ => None,
                    };
                    ct.map(|t| {
                        let s = match t {
                            ContentType::Audio => "audio",
                            ContentType::Midi => "midi",
                        };
                        FieldValue::String(s.into())
                    })
                    .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("Region", "generation_tool") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    if let Behavior::Latent { tool, .. } = &r.behavior {
                        FieldValue::String(tool.clone().into())
                    } else {
                        FieldValue::Null
                    }
                } else {
                    unreachable!()
                }
            }),
            ("Region", "is_alive") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_alive())
                } else {
                    unreachable!()
                }
            }),
            ("Region", "is_tombstoned") => resolve_property_with(contexts, |v| {
                if let Vertex::Region(r) = v {
                    FieldValue::Boolean(r.is_tombstoned())
                } else {
                    unreachable!()
                }
            }),

            // Node properties
            ("Node", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::String(n.descriptor.id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("Node", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::String(n.descriptor.name.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Node", "type_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::String(n.descriptor.type_id.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Node", "latency_samples") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::Int64(n.descriptor.latency_samples as i64)
                } else {
                    unreachable!()
                }
            }),
            ("Node", "can_realtime") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::Boolean(n.descriptor.capabilities.realtime)
                } else {
                    unreachable!()
                }
            }),
            ("Node", "can_offline") => resolve_property_with(contexts, |v| {
                if let Vertex::Node(n) = v {
                    FieldValue::Boolean(n.descriptor.capabilities.offline)
                } else {
                    unreachable!()
                }
            }),

            // Port properties
            ("Port", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Port(p) = v {
                    FieldValue::String(p.name.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Port", "signal_type") => resolve_property_with(contexts, |v| {
                if let Vertex::Port(p) = v {
                    let st = match p.signal_type {
                        SignalType::Audio => "audio",
                        SignalType::Midi => "midi",
                        SignalType::Control => "control",
                        SignalType::Trigger => "trigger",
                    };
                    FieldValue::String(st.into())
                } else {
                    unreachable!()
                }
            }),

            // Edge properties
            ("Edge", "source_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Edge(e) = v {
                    FieldValue::String(e.source_id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("Edge", "source_port") => resolve_property_with(contexts, |v| {
                if let Vertex::Edge(e) = v {
                    FieldValue::String(e.source_port.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Edge", "dest_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Edge(e) = v {
                    FieldValue::String(e.dest_id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("Edge", "dest_port") => resolve_property_with(contexts, |v| {
                if let Vertex::Edge(e) = v {
                    FieldValue::String(e.dest_port.clone().into())
                } else {
                    unreachable!()
                }
            }),

            // Output properties
            ("Output", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Output(o) = v {
                    FieldValue::String(o.id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("Output", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Output(o) = v {
                    FieldValue::String(o.name.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Output", "channels") => resolve_property_with(contexts, |v| {
                if let Vertex::Output(o) = v {
                    FieldValue::Int64(o.channels as i64)
                } else {
                    unreachable!()
                }
            }),
            ("Output", "pw_node_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Output(o) = v {
                    o.pw_node_id
                        .map(|id| FieldValue::Int64(id as i64))
                        .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),

            // Input properties
            ("Input", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    FieldValue::String(i.id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("Input", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    FieldValue::String(i.name.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Input", "channels") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    FieldValue::Int64(i.channels as i64)
                } else {
                    unreachable!()
                }
            }),
            ("Input", "port_pattern") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    i.port_pattern
                        .as_ref()
                        .map(|p| FieldValue::String(p.clone().into()))
                        .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("Input", "pw_node_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Input(i) = v {
                    i.pw_node_id
                        .map(|id| FieldValue::Int64(id as i64))
                        .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),

            // MidiDevice properties
            ("MidiDevice", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::MidiDevice(d) = v {
                    FieldValue::String(d.id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("MidiDevice", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::MidiDevice(d) = v {
                    FieldValue::String(d.name.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("MidiDevice", "direction") => resolve_property_with(contexts, |v| {
                if let Vertex::MidiDevice(d) = v {
                    let dir = match d.direction {
                        MidiDirection::Input => "input",
                        MidiDirection::Output => "output",
                    };
                    FieldValue::String(dir.into())
                } else {
                    unreachable!()
                }
            }),
            ("MidiDevice", "pw_node_id") => resolve_property_with(contexts, |v| {
                if let Vertex::MidiDevice(d) = v {
                    d.pw_node_id
                        .map(|id| FieldValue::Int64(id as i64))
                        .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),

            // Job properties
            ("Job", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Job(j) = v {
                    FieldValue::String(j.id.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Job", "region_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Job(j) = v {
                    FieldValue::String(j.region_id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("Job", "tool") => resolve_property_with(contexts, |v| {
                if let Vertex::Job(j) = v {
                    FieldValue::String(j.tool.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Job", "progress") => resolve_property_with(contexts, |v| {
                if let Vertex::Job(j) = v {
                    FieldValue::Float64(j.progress as f64)
                } else {
                    unreachable!()
                }
            }),

            // Approval properties
            ("Approval", "region_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Approval(a) = v {
                    FieldValue::String(a.region_id.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            ("Approval", "content_hash") => resolve_property_with(contexts, |v| {
                if let Vertex::Approval(a) = v {
                    FieldValue::String(a.content_hash.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Approval", "content_type") => resolve_property_with(contexts, |v| {
                if let Vertex::Approval(a) = v {
                    let ct = match a.content_type {
                        ContentType::Audio => "audio",
                        ContentType::Midi => "midi",
                    };
                    FieldValue::String(ct.into())
                } else {
                    unreachable!()
                }
            }),

            // TempoResult properties
            ("TempoResult", "bpm") => resolve_property_with(contexts, |v| {
                if let Vertex::TempoResult(bpm) = v {
                    FieldValue::Float64(*bpm)
                } else {
                    unreachable!()
                }
            }),

            // TimeResult properties
            ("TimeResult", "value") => resolve_property_with(contexts, |v| {
                if let Vertex::TimeResult(val) = v {
                    FieldValue::Float64(*val)
                } else {
                    unreachable!()
                }
            }),

            _ => unreachable!("Unknown property: {type_name}.{property_name}"),
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &str,
        edge_name: &str,
        _parameters: &EdgeParameters,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Self::Vertex>> {
        let graph = self.graph.clone();

        match (type_name, edge_name) {
            ("Node", "inputs") => resolve_neighbors_with(contexts, move |v| {
                if let Vertex::Node(n) = v {
                    let ports: Vec<_> = n
                        .descriptor
                        .inputs
                        .iter()
                        .map(|p| {
                            Vertex::Port(Arc::new(PortVertex {
                                name: p.name.clone(),
                                signal_type: p.signal_type,
                            }))
                        })
                        .collect();
                    Box::new(ports.into_iter()) as VertexIterator<'a, Self::Vertex>
                } else {
                    unreachable!()
                }
            }),
            ("Node", "outputs") => resolve_neighbors_with(contexts, move |v| {
                if let Vertex::Node(n) = v {
                    let ports: Vec<_> = n
                        .descriptor
                        .outputs
                        .iter()
                        .map(|p| {
                            Vertex::Port(Arc::new(PortVertex {
                                name: p.name.clone(),
                                signal_type: p.signal_type,
                            }))
                        })
                        .collect();
                    Box::new(ports.into_iter()) as VertexIterator<'a, Self::Vertex>
                } else {
                    unreachable!()
                }
            }),
            ("Node", "upstream") => resolve_neighbors_with(contexts, move |v| {
                if let Vertex::Node(n) = v {
                    let node_id = n.descriptor.id;
                    let upstream_nodes: Vec<_> = graph
                        .read()
                        .map(|g| {
                            let upstream_ids = g.upstream(node_id);
                            upstream_ids
                                .iter()
                                .filter_map(|&id| g.node(id))
                                .map(|node| {
                                    Vertex::Node(Arc::new(NodeVertex {
                                        descriptor: node.descriptor().clone(),
                                    }))
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    Box::new(upstream_nodes.into_iter()) as VertexIterator<'a, Self::Vertex>
                } else {
                    unreachable!()
                }
            }),
            ("Node", "downstream") => resolve_neighbors_with(contexts, move |v| {
                if let Vertex::Node(n) = v {
                    let node_id = n.descriptor.id;
                    let downstream_nodes: Vec<_> = graph
                        .read()
                        .map(|g| {
                            let downstream_ids = g.downstream(node_id);
                            downstream_ids
                                .iter()
                                .filter_map(|&id| g.node(id))
                                .map(|node| {
                                    Vertex::Node(Arc::new(NodeVertex {
                                        descriptor: node.descriptor().clone(),
                                    }))
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    Box::new(downstream_nodes.into_iter()) as VertexIterator<'a, Self::Vertex>
                } else {
                    unreachable!()
                }
            }),
            _ => unreachable!("Unknown edge: {type_name}.{edge_name}"),
        }
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        _type_name: &str,
        _coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'a, V, bool> {
        Box::new(contexts.map(|ctx| (ctx, true)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::primitives::{
        Beat, Node, NodeCapabilities, NodeDescriptor, Port, ProcessContext, ProcessError,
        SignalBuffer, SignalType, TempoMap,
    };
    use serde_json::json;
    use std::sync::{Arc, RwLock};
    use trustfall::execute_query;

    type Variables = std::collections::BTreeMap<Arc<str>, FieldValue>;

    struct TestNode {
        descriptor: NodeDescriptor,
    }

    impl Node for TestNode {
        fn descriptor(&self) -> &NodeDescriptor {
            &self.descriptor
        }

        fn process(
            &mut self,
            _ctx: &ProcessContext,
            _inputs: &[SignalBuffer],
            _outputs: &mut [SignalBuffer],
        ) -> Result<(), ProcessError> {
            Ok(())
        }
    }

    fn make_test_node(name: &str, type_id: &str) -> Box<dyn Node> {
        Box::new(TestNode {
            descriptor: NodeDescriptor {
                id: Uuid::new_v4(),
                name: name.to_string(),
                type_id: type_id.to_string(),
                inputs: vec![Port {
                    name: "in".to_string(),
                    signal_type: SignalType::Audio,
                }],
                outputs: vec![Port {
                    name: "out".to_string(),
                    signal_type: SignalType::Audio,
                }],
                latency_samples: 0,
                capabilities: NodeCapabilities {
                    realtime: true,
                    offline: true,
                },
            },
        })
    }

    fn setup_test_adapter() -> Arc<ChaosgardenAdapter> {
        let mut regions = vec![
            Region::play_audio(Beat(0.0), Beat(4.0), "hash_intro".to_string()).with_name("intro"),
            Region::play_audio(Beat(4.0), Beat(8.0), "hash_verse".to_string())
                .with_name("verse")
                .with_tag("jazzy"),
            Region::latent(Beat(12.0), Beat(4.0), "orpheus_generate", json!({}))
                .with_name("pending_solo"),
        ];

        regions[2].start_job("job_123".to_string());
        regions[2].update_progress(0.5);

        let mut graph = Graph::new();
        let source = make_test_node("source", "source.audio");
        let effect = make_test_node("reverb", "effect.reverb");
        let output = make_test_node("master", "output.stereo");

        let source_id = source.descriptor().id;
        let effect_id = effect.descriptor().id;
        let output_id = output.descriptor().id;

        graph.add_node(source);
        graph.add_node(effect);
        graph.add_node(output);

        graph.connect(source_id, "out", effect_id, "in").unwrap();
        graph.connect(effect_id, "out", output_id, "in").unwrap();

        let adapter = ChaosgardenAdapter::new(
            Arc::new(RwLock::new(regions)),
            Arc::new(RwLock::new(graph)),
            Arc::new(TempoMap::new(120.0, Default::default())),
        )
        .unwrap();

        Arc::new(adapter)
    }

    #[test]
    fn test_query_all_regions() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                Region {
                    name @output
                    position @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_query_region_by_id() {
        let adapter = setup_test_adapter();
        let regions = adapter.get_regions();
        let first_id = regions[0].id.to_string();

        let query = format!(
            r#"
            query {{
                Region(id: "{}") {{
                    name @output
                }}
            }}
        "#,
            first_id
        );

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), &query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let name_key: Arc<str> = "name".into();
        assert_eq!(
            results[0].get(&name_key),
            Some(&FieldValue::String("intro".into()))
        );
    }

    #[test]
    fn test_query_regions_in_range() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                RegionInRange(start: 3.0, end: 10.0) {
                    name @output
                    position @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_regions_by_tag() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                RegionByTag(tag: "jazzy") {
                    name @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let name_key: Arc<str> = "name".into();
        assert_eq!(
            results[0].get(&name_key),
            Some(&FieldValue::String("verse".into()))
        );
    }

    #[test]
    fn test_query_latent_regions() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                LatentRegion {
                    name @output
                    latent_status @output
                    latent_progress @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let status_key: Arc<str> = "latent_status".into();
        assert_eq!(
            results[0].get(&status_key),
            Some(&FieldValue::String("running".into()))
        );
    }

    #[test]
    fn test_query_playable_regions() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                PlayableRegion {
                    name @output
                    is_playable @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_nodes() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                Node {
                    name @output
                    type_id @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_query_nodes_by_type_prefix() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                Node(type_prefix: "effect.") {
                    name @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let name_key: Arc<str> = "name".into();
        assert_eq!(
            results[0].get(&name_key),
            Some(&FieldValue::String("reverb".into()))
        );
    }

    #[test]
    fn test_query_node_with_ports() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                Node(type_prefix: "effect.") {
                    name @output
                    inputs {
                        name @output(name: "input_name")
                        signal_type @output
                    }
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let input_name_key: Arc<str> = "input_name".into();
        assert_eq!(
            results[0].get(&input_name_key),
            Some(&FieldValue::String("in".into()))
        );
    }

    #[test]
    fn test_query_edges() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                Edge {
                    source_id @output
                    dest_id @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_running_jobs() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                RunningJob {
                    id @output
                    tool @output
                    progress @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let tool_key: Arc<str> = "tool".into();
        assert_eq!(
            results[0].get(&tool_key),
            Some(&FieldValue::String("orpheus_generate".into()))
        );
    }

    #[test]
    fn test_query_tempo_at() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                TempoAt(beat: 0.0) {
                    bpm @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let bpm_key: Arc<str> = "bpm".into();
        assert_eq!(results[0].get(&bpm_key), Some(&FieldValue::Float64(120.0)));
    }

    #[test]
    fn test_query_beat_to_second() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                BeatToSecond(beat: 2.0) {
                    value @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let value_key: Arc<str> = "value".into();
        let value = results[0].get(&value_key).unwrap();
        if let FieldValue::Float64(v) = value {
            assert!((v - 1.0).abs() < 0.001);
        } else {
            panic!("Expected Float64");
        }
    }

    #[test]
    fn test_query_node_downstream_traversal() {
        let adapter = setup_test_adapter();

        let query = r#"
            query {
                Node(type_prefix: "source.") {
                    name @output
                    downstream {
                        name @output(name: "downstream_name")
                    }
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        // source has 2 downstream nodes: reverb and master (the whole chain)
        assert_eq!(results.len(), 2);

        let downstream_key: Arc<str> = "downstream_name".into();
        let downstream_names: Vec<_> = results
            .iter()
            .filter_map(|r| r.get(&downstream_key))
            .collect();
        assert!(downstream_names.contains(&&FieldValue::String("reverb".into())));
        assert!(downstream_names.contains(&&FieldValue::String("master".into())));
    }
}
