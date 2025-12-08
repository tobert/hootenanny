# Task 05: Layered Audio Graph Architecture

**Priority:** High
**Estimated Sessions:** 4-5
**Depends On:** 01-core-structs, 03-renderer

---

## Objective

Implement a two-layer audio graph architecture:

1. **Internal Layer (dasp_graph)**: Per-Timeline compute graphs for clips, effects, mixing
2. **External Layer (PipeWire)**: Cross-Timeline routing, hardware I/O, session management

Both layers are queryable through Trustfall, enabling queries that span from "which clip" to "which speaker."

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      PipeWire (System Audio Bus)                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │  Hardware   │  │  Timeline A │  │  Timeline B │  │  External   │    │
│  │  Inputs     │  │  (drums)    │  │  (synths)   │  │  Apps       │    │
│  │  ┌───┐┌───┐ │  │  ┌───────┐  │  │  ┌───────┐  │  │  ┌───────┐  │    │
│  │  │Mic││USB│ │  │  │Master │  │  │  │Master │  │  │  │Reaper │  │    │
│  │  └─┬─┘└─┬─┘ │  │  └───┬───┘  │  │  └───┬───┘  │  │  └───┬───┘  │    │
│  └────┼────┼───┘  └──────┼──────┘  └──────┼──────┘  └──────┼──────┘    │
│       │    │             │                │                │           │
│       │    └─────────────┼────────────────┼────────────────┘           │
│       │                  │                │                             │
│       │         ┌────────┴────────────────┴────────┐                   │
│       │         │         Submix Bus               │                   │
│       │         └────────────────┬─────────────────┘                   │
│       │                          │                                      │
│       │                  ┌───────┴───────┐                             │
│       └─────────────────▶│ Master Output │──────▶ Speakers             │
│                          └───────────────┘                             │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                    Timeline A - Internal dasp_graph                      │
│                                                                          │
│  ┌─────────┐   ┌─────────┐        ┌─────────┐                          │
│  │ Clip:   │──▶│         │        │         │                          │
│  │ kick.wav│   │  Track  │───────▶│         │                          │
│  └─────────┘   │  "Drums"│        │         │                          │
│  ┌─────────┐   │  vol:0.8│        │ Master  │───▶ PipeWire Node        │
│  │ Latent: │──▶│  pan:0  │        │         │     "Timeline A"         │
│  │ fills   │   └─────────┘        │         │                          │
│  └─────────┘                      │         │                          │
│  ┌─────────┐   ┌─────────┐        │         │                          │
│  │ Clip:   │──▶│  Track  │───────▶│         │                          │
│  │ hats.wav│   │  "Perc" │        └─────────┘                          │
│  └─────────┘   └─────────┘                                              │
└─────────────────────────────────────────────────────────────────────────┘
```

## Why Two Layers?

| Concern | dasp_graph (Internal) | PipeWire (External) |
|---------|----------------------|---------------------|
| **Granularity** | Sample-accurate | Buffer-level (256-2048 samples) |
| **Timing** | Beat/tick synchronized | Real-time clock |
| **Parallelism** | Per-Timeline isolation | System-wide scheduling |
| **Persistence** | Project file | Session manager |
| **Hardware** | Abstracted | Direct ALSA/JACK access |
| **Latency** | Computed, compensated | Measured, reported |

**Key insight**: dasp_graph handles what happens *inside* a composition. PipeWire handles what happens *between* compositions and the real world.

## Dependencies

```toml
[dependencies]
dasp_graph = { version = "0.11", features = ["node-boxed", "node-sum", "node-pass"] }
petgraph = "0.6"
trustfall = "0.8"
pipewire = "0.8"  # For PipeWire client integration
```

## Files to Create

### `crates/flayer/src/graph/mod.rs`

```rust
pub mod internal;
pub mod external;
pub mod unified_adapter;

pub use internal::{RenderGraph, RenderNode, RenderEdge};
pub use external::{PipeWireClient, ExternalNode};
pub use unified_adapter::{UnifiedGraphAdapter, GraphVertex};
```

### `crates/flayer/src/graph/internal.rs`

```rust
//! Internal render graph using dasp_graph
//!
//! Each Timeline has its own RenderGraph. Multiple Timelines can process
//! in parallel since their graphs are isolated.

use dasp_graph::{Buffer, Input, Node, NodeData};
use petgraph::stable_graph::{NodeIndex, StableGraph};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::{Clip, ClipSource, Latent, Timeline, Track};

/// Node types in the internal render graph
#[derive(Debug, Clone)]
pub enum RenderNode {
    /// Audio/MIDI source (clip or resolved latent)
    Source {
        id: Uuid,
        hash: String,
        gain: f64,
        /// Position on timeline (samples from start)
        offset_samples: usize,
        /// Duration in samples
        duration_samples: usize,
    },

    /// Track mixer (sums inputs, applies volume/pan)
    TrackMix {
        track_id: Uuid,
        name: String,
        volume: f64,
        pan: f64,
        muted: bool,
        solo: bool,
    },

    /// Effect processor
    Effect {
        id: Uuid,
        effect_type: EffectType,
        params: serde_json::Value,
    },

    /// Timeline master output - connects to PipeWire
    Master {
        /// PipeWire node name (if connected)
        pipewire_name: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum EffectType {
    Gain,
    Pan,
    Compressor,
    Reverb,
    Delay,
    Filter,
    Custom(String),
}

/// Edge representing audio flow
#[derive(Debug, Clone, Default)]
pub struct RenderEdge {
    /// Gain applied on this connection
    pub gain: f64,
}

impl RenderEdge {
    pub fn new() -> Self {
        Self { gain: 1.0 }
    }

    pub fn with_gain(gain: f64) -> Self {
        Self { gain }
    }
}

/// The internal render graph for a single Timeline
pub struct RenderGraph {
    /// Underlying petgraph structure
    pub graph: StableGraph<RenderNode, RenderEdge>,

    /// Map from clip/latent UUID to node index
    pub source_indices: HashMap<Uuid, NodeIndex>,

    /// Map from track UUID to mixer node index
    pub track_indices: HashMap<Uuid, NodeIndex>,

    /// The master output node
    pub master_index: NodeIndex,

    /// Timeline metadata for time conversion
    pub sample_rate: u32,
    pub bpm: f64,
    pub ppq: u16,
}

impl RenderGraph {
    /// Build a render graph from a Timeline
    pub fn from_timeline(timeline: &Timeline, sample_rate: u32) -> Self {
        let mut graph = StableGraph::new();
        let mut source_indices = HashMap::new();
        let mut track_indices = HashMap::new();

        // Create master node
        let master_index = graph.add_node(RenderNode::Master {
            pipewire_name: None,
        });

        // Create track mixer nodes and their sources
        for track in &timeline.tracks {
            let track_node = graph.add_node(RenderNode::TrackMix {
                track_id: track.id,
                name: track.name.clone(),
                volume: track.volume,
                pan: track.pan,
                muted: track.muted,
                solo: track.solo,
            });
            track_indices.insert(track.id, track_node);

            // Connect track to master
            graph.add_edge(track_node, master_index, RenderEdge::new());

            // Add clip sources
            for clip in &track.clips {
                let hash = match &clip.source {
                    ClipSource::Audio(a) => a.hash.clone(),
                    ClipSource::Midi(m) => m.hash.clone(),
                };

                let offset = Self::beats_to_samples(clip.at, timeline.bpm, sample_rate);
                let duration = Self::beats_to_samples(clip.duration, timeline.bpm, sample_rate);

                let source_node = graph.add_node(RenderNode::Source {
                    id: clip.id,
                    hash,
                    gain: clip.gain,
                    offset_samples: offset,
                    duration_samples: duration,
                });
                source_indices.insert(clip.id, source_node);

                graph.add_edge(source_node, track_node, RenderEdge::with_gain(clip.gain));
            }

            // Add resolved latent sources
            for latent in &track.latents {
                if let Some(resolved) = &latent.resolved {
                    let hash = match &resolved.source {
                        ClipSource::Audio(a) => a.hash.clone(),
                        ClipSource::Midi(m) => m.hash.clone(),
                    };

                    let offset = Self::beats_to_samples(latent.at, timeline.bpm, sample_rate);
                    let duration = Self::beats_to_samples(latent.duration, timeline.bpm, sample_rate);

                    let source_node = graph.add_node(RenderNode::Source {
                        id: latent.id,
                        hash,
                        gain: resolved.gain,
                        offset_samples: offset,
                        duration_samples: duration,
                    });
                    source_indices.insert(latent.id, source_node);

                    graph.add_edge(source_node, track_node, RenderEdge::with_gain(resolved.gain));
                }
            }
        }

        Self {
            graph,
            source_indices,
            track_indices,
            master_index,
            sample_rate,
            bpm: timeline.bpm,
            ppq: timeline.ppq,
        }
    }

    fn beats_to_samples(beats: f64, bpm: f64, sample_rate: u32) -> usize {
        let seconds = beats * 60.0 / bpm;
        (seconds * sample_rate as f64) as usize
    }

    /// Get all source nodes (leaf nodes with no inputs)
    pub fn sources(&self) -> Vec<NodeIndex> {
        self.graph.node_indices()
            .filter(|&idx| {
                self.graph.neighbors_directed(idx, petgraph::Direction::Incoming).count() == 0
            })
            .collect()
    }

    /// Get all sink nodes (should just be master)
    pub fn sinks(&self) -> Vec<NodeIndex> {
        self.graph.node_indices()
            .filter(|&idx| {
                self.graph.neighbors_directed(idx, petgraph::Direction::Outgoing).count() == 0
            })
            .collect()
    }

    /// Get the signal path from a source to master
    pub fn signal_path(&self, source: NodeIndex) -> Vec<NodeIndex> {
        use petgraph::algo::astar;

        if let Some((_, path)) = astar(
            &self.graph,
            source,
            |n| n == self.master_index,
            |_| 1,
            |_| 0,
        ) {
            path
        } else {
            vec![]
        }
    }

    /// Find all nodes that must render before the given node
    pub fn dependencies(&self, node: NodeIndex) -> Vec<NodeIndex> {
        use petgraph::visit::Dfs;

        let mut deps = Vec::new();
        let reversed = petgraph::visit::Reversed(&self.graph);
        let mut dfs = Dfs::new(&reversed, node);

        while let Some(nx) = dfs.next(&reversed) {
            if nx != node {
                deps.push(nx);
            }
        }

        deps
    }

    /// Get topological order for rendering
    pub fn render_order(&self) -> Vec<NodeIndex> {
        use petgraph::algo::toposort;

        // Reverse because we want sources first, master last
        let mut order = toposort(&self.graph, None).unwrap_or_default();
        order.reverse();
        order
    }

    /// Insert an effect between a source and its current output
    pub fn insert_effect(&mut self, after: NodeIndex, effect: RenderNode) -> NodeIndex {
        let effect_idx = self.graph.add_node(effect);

        // Find all outgoing edges from 'after'
        let outgoing: Vec<_> = self.graph
            .neighbors_directed(after, petgraph::Direction::Outgoing)
            .collect();

        // Redirect them through the effect
        for target in outgoing {
            if let Some(edge_idx) = self.graph.find_edge(after, target) {
                let edge_weight = self.graph.remove_edge(edge_idx).unwrap_or_default();
                self.graph.add_edge(effect_idx, target, edge_weight);
            }
        }

        // Connect source to effect
        self.graph.add_edge(after, effect_idx, RenderEdge::new());

        effect_idx
    }

    /// Connect this graph's master to a PipeWire node name
    pub fn connect_to_pipewire(&mut self, pipewire_name: &str) {
        if let Some(master) = self.graph.node_weight_mut(self.master_index) {
            if let RenderNode::Master { pipewire_name: ref mut pw_name } = master {
                *pw_name = Some(pipewire_name.to_string());
            }
        }
    }
}
```

### `crates/flayer/src/graph/external.rs`

```rust
//! External audio graph via PipeWire
//!
//! Handles routing between Timelines and hardware I/O.
//! Queries the existing PipeWireSnapshot from audio-graph-mcp.

use std::sync::Arc;
use crate::graph::internal::RenderGraph;

/// Reference to a PipeWire node that a Timeline's master outputs to
#[derive(Debug, Clone)]
pub struct ExternalNode {
    pub id: u32,
    pub name: String,
    pub description: Option<String>,
    pub media_class: Option<String>,
}

/// A live Timeline with both internal graph and external PipeWire connection
pub struct LiveTimeline {
    /// The Timeline being rendered
    pub timeline_id: uuid::Uuid,

    /// Internal render graph (rebuilt when timeline changes)
    pub render_graph: RenderGraph,

    /// PipeWire node ID for this timeline's output
    pub pipewire_node_id: Option<u32>,

    /// Name visible in PipeWire/pavucontrol
    pub pipewire_name: String,
}

impl LiveTimeline {
    pub fn new(timeline: &crate::Timeline, sample_rate: u32, name: &str) -> Self {
        let mut render_graph = RenderGraph::from_timeline(timeline, sample_rate);
        render_graph.connect_to_pipewire(name);

        Self {
            timeline_id: timeline.id,
            render_graph,
            pipewire_node_id: None,
            pipewire_name: name.to_string(),
        }
    }
}

/// Manager for multiple live timelines
pub struct TimelineManager {
    /// All active timelines
    pub timelines: Vec<LiveTimeline>,

    /// Sample rate for all timelines
    pub sample_rate: u32,
}

impl TimelineManager {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            timelines: Vec::new(),
            sample_rate,
        }
    }

    pub fn add_timeline(&mut self, timeline: &crate::Timeline, name: &str) -> usize {
        let live = LiveTimeline::new(timeline, self.sample_rate, name);
        self.timelines.push(live);
        self.timelines.len() - 1
    }

    /// Get all timelines that could be rendered in parallel
    /// (those with no cross-timeline dependencies)
    pub fn parallel_groups(&self) -> Vec<Vec<usize>> {
        // For now, all timelines are independent
        // Future: detect sidechain connections between timelines
        self.timelines.iter().enumerate().map(|(i, _)| vec![i]).collect()
    }
}
```

### `crates/flayer/src/graph/unified_adapter.rs`

```rust
//! Unified Trustfall adapter that spans both internal and external graphs

use std::sync::Arc;
use trustfall::{
    provider::{
        AsVertex, ContextIterator, ContextOutcomeIterator, EdgeParameters,
        Typename, VertexIterator, resolve_neighbors_with, resolve_property_with,
    },
    FieldValue, Schema,
};
use petgraph::stable_graph::NodeIndex;

use crate::graph::internal::{RenderGraph, RenderNode};
use audio_graph_mcp::sources::{PipeWireNode, PipeWirePort, PipeWireSnapshot};

/// Unified vertex type spanning internal and external graphs
#[derive(Debug, Clone)]
pub enum GraphVertex {
    // Internal (dasp_graph / RenderGraph)
    InternalSource {
        index: NodeIndex,
        id: uuid::Uuid,
        hash: String,
        gain: f64,
    },
    InternalTrack {
        index: NodeIndex,
        track_id: uuid::Uuid,
        name: String,
        volume: f64,
        pan: f64,
        muted: bool,
    },
    InternalMaster {
        index: NodeIndex,
        pipewire_name: Option<String>,
    },
    InternalEffect {
        index: NodeIndex,
        id: uuid::Uuid,
        effect_type: String,
    },

    // External (PipeWire)
    ExternalNode(Arc<PipeWireNode>),
    ExternalPort(Arc<PipeWirePort>),
}

impl Typename for GraphVertex {
    fn typename(&self) -> &'static str {
        match self {
            Self::InternalSource { .. } => "Source",
            Self::InternalTrack { .. } => "Track",
            Self::InternalMaster { .. } => "Master",
            Self::InternalEffect { .. } => "Effect",
            Self::ExternalNode(_) => "PipeWireNode",
            Self::ExternalPort(_) => "PipeWirePort",
        }
    }
}

/// Adapter that can query both the internal render graph and external PipeWire state
pub struct UnifiedGraphAdapter {
    /// Internal render graphs (one per timeline)
    render_graphs: Vec<Arc<RenderGraph>>,

    /// External PipeWire state
    pipewire_snapshot: Arc<PipeWireSnapshot>,

    /// Schema
    schema: Arc<Schema>,
}

impl UnifiedGraphAdapter {
    pub fn new(
        render_graphs: Vec<RenderGraph>,
        pipewire_snapshot: PipeWireSnapshot,
    ) -> anyhow::Result<Self> {
        let schema_text = include_str!("unified_graph.graphql");
        let schema = Arc::new(Schema::parse(schema_text)?);

        Ok(Self {
            render_graphs: render_graphs.into_iter().map(Arc::new).collect(),
            pipewire_snapshot: Arc::new(pipewire_snapshot),
            schema,
        })
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Find the PipeWire node that a master connects to (by name matching)
    fn find_pipewire_node(&self, pipewire_name: &str) -> Option<Arc<PipeWireNode>> {
        self.pipewire_snapshot.nodes.iter()
            .find(|n| n.name == pipewire_name)
            .map(|n| Arc::new(n.clone()))
    }
}

impl<'a> trustfall::provider::BasicAdapter<'a> for UnifiedGraphAdapter {
    type Vertex = GraphVertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name {
            // Internal entry points
            "Source" => {
                let sources: Vec<_> = self.render_graphs.iter()
                    .flat_map(|rg| {
                        rg.sources().into_iter().filter_map(|idx| {
                            match rg.graph.node_weight(idx)? {
                                RenderNode::Source { id, hash, gain, .. } => {
                                    Some(GraphVertex::InternalSource {
                                        index: idx,
                                        id: *id,
                                        hash: hash.clone(),
                                        gain: *gain,
                                    })
                                }
                                _ => None,
                            }
                        })
                    })
                    .collect();
                Box::new(sources.into_iter())
            }
            "Track" => {
                let tracks: Vec<_> = self.render_graphs.iter()
                    .flat_map(|rg| {
                        rg.track_indices.values().filter_map(|&idx| {
                            match rg.graph.node_weight(idx)? {
                                RenderNode::TrackMix { track_id, name, volume, pan, muted, .. } => {
                                    Some(GraphVertex::InternalTrack {
                                        index: idx,
                                        track_id: *track_id,
                                        name: name.clone(),
                                        volume: *volume,
                                        pan: *pan,
                                        muted: *muted,
                                    })
                                }
                                _ => None,
                            }
                        })
                    })
                    .collect();
                Box::new(tracks.into_iter())
            }
            "Master" => {
                let masters: Vec<_> = self.render_graphs.iter()
                    .filter_map(|rg| {
                        match rg.graph.node_weight(rg.master_index)? {
                            RenderNode::Master { pipewire_name } => {
                                Some(GraphVertex::InternalMaster {
                                    index: rg.master_index,
                                    pipewire_name: pipewire_name.clone(),
                                })
                            }
                            _ => None,
                        }
                    })
                    .collect();
                Box::new(masters.into_iter())
            }

            // External entry points
            "PipeWireNode" => {
                let media_class = parameters.get("media_class").and_then(|v| v.as_str());

                let nodes: Vec<_> = self.pipewire_snapshot.nodes.iter()
                    .filter(|n| {
                        media_class.map_or(true, |mc| n.media_class.as_deref() == Some(mc))
                    })
                    .map(|n| GraphVertex::ExternalNode(Arc::new(n.clone())))
                    .collect();
                Box::new(nodes.into_iter())
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
            // Source properties
            ("Source", "id") => resolve_property_with(contexts, |v| {
                if let GraphVertex::InternalSource { id, .. } = v {
                    FieldValue::String(id.to_string().into())
                } else { FieldValue::Null }
            }),
            ("Source", "hash") => resolve_property_with(contexts, |v| {
                if let GraphVertex::InternalSource { hash, .. } = v {
                    FieldValue::String(hash.clone().into())
                } else { FieldValue::Null }
            }),
            ("Source", "gain") => resolve_property_with(contexts, |v| {
                if let GraphVertex::InternalSource { gain, .. } = v {
                    FieldValue::Float64(*gain)
                } else { FieldValue::Null }
            }),

            // Track properties
            ("Track", "name") => resolve_property_with(contexts, |v| {
                if let GraphVertex::InternalTrack { name, .. } = v {
                    FieldValue::String(name.clone().into())
                } else { FieldValue::Null }
            }),
            ("Track", "volume") => resolve_property_with(contexts, |v| {
                if let GraphVertex::InternalTrack { volume, .. } = v {
                    FieldValue::Float64(*volume)
                } else { FieldValue::Null }
            }),
            ("Track", "muted") => resolve_property_with(contexts, |v| {
                if let GraphVertex::InternalTrack { muted, .. } = v {
                    FieldValue::Boolean(*muted)
                } else { FieldValue::Null }
            }),

            // Master properties
            ("Master", "pipewire_name") => resolve_property_with(contexts, |v| {
                if let GraphVertex::InternalMaster { pipewire_name, .. } = v {
                    pipewire_name.as_ref()
                        .map(|s| FieldValue::String(s.clone().into()))
                        .unwrap_or(FieldValue::Null)
                } else { FieldValue::Null }
            }),

            // PipeWire properties
            ("PipeWireNode", "id") => resolve_property_with(contexts, |v| {
                if let GraphVertex::ExternalNode(n) = v {
                    FieldValue::Int64(n.id as i64)
                } else { FieldValue::Null }
            }),
            ("PipeWireNode", "name") => resolve_property_with(contexts, |v| {
                if let GraphVertex::ExternalNode(n) = v {
                    FieldValue::String(n.name.clone().into())
                } else { FieldValue::Null }
            }),
            ("PipeWireNode", "media_class") => resolve_property_with(contexts, |v| {
                if let GraphVertex::ExternalNode(n) = v {
                    n.media_class.as_ref()
                        .map(|s| FieldValue::String(s.clone().into()))
                        .unwrap_or(FieldValue::Null)
                } else { FieldValue::Null }
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
        let graphs = self.render_graphs.clone();
        let snapshot = self.pipewire_snapshot.clone();

        match (type_name, edge_name) {
            // Internal navigation
            (_, "outputs") => {
                resolve_neighbors_with(contexts, move |v| {
                    let index = match v {
                        GraphVertex::InternalSource { index, .. } => *index,
                        GraphVertex::InternalTrack { index, .. } => *index,
                        GraphVertex::InternalEffect { index, .. } => *index,
                        _ => return Box::new(std::iter::empty()) as VertexIterator<'a, Self::Vertex>,
                    };

                    // Find which graph contains this node
                    for rg in graphs.iter() {
                        if rg.graph.node_weight(index).is_some() {
                            let neighbors: Vec<_> = rg.graph
                                .neighbors_directed(index, petgraph::Direction::Outgoing)
                                .filter_map(|idx| {
                                    rg.graph.node_weight(idx).map(|node| {
                                        match node {
                                            RenderNode::Source { id, hash, gain, .. } => {
                                                GraphVertex::InternalSource {
                                                    index: idx, id: *id, hash: hash.clone(), gain: *gain,
                                                }
                                            }
                                            RenderNode::TrackMix { track_id, name, volume, pan, muted, .. } => {
                                                GraphVertex::InternalTrack {
                                                    index: idx, track_id: *track_id, name: name.clone(),
                                                    volume: *volume, pan: *pan, muted: *muted,
                                                }
                                            }
                                            RenderNode::Master { pipewire_name } => {
                                                GraphVertex::InternalMaster {
                                                    index: idx, pipewire_name: pipewire_name.clone(),
                                                }
                                            }
                                            RenderNode::Effect { id, effect_type, .. } => {
                                                GraphVertex::InternalEffect {
                                                    index: idx, id: *id, effect_type: format!("{:?}", effect_type),
                                                }
                                            }
                                        }
                                    })
                                })
                                .collect();
                            return Box::new(neighbors.into_iter()) as VertexIterator<'a, Self::Vertex>;
                        }
                    }
                    Box::new(std::iter::empty())
                })
            }

            // Cross-layer: Master → PipeWire
            ("Master", "pipewire_node") => {
                resolve_neighbors_with(contexts, move |v| {
                    if let GraphVertex::InternalMaster { pipewire_name: Some(name), .. } = v {
                        let node = snapshot.nodes.iter()
                            .find(|n| &n.name == name)
                            .map(|n| GraphVertex::ExternalNode(Arc::new(n.clone())));
                        Box::new(node.into_iter()) as VertexIterator<'a, Self::Vertex>
                    } else {
                        Box::new(std::iter::empty())
                    }
                })
            }

            // PipeWire navigation
            ("PipeWireNode", "ports") => {
                resolve_neighbors_with(contexts, move |v| {
                    if let GraphVertex::ExternalNode(n) = v {
                        let ports: Vec<_> = snapshot.ports.iter()
                            .filter(|p| p.node_id == n.id)
                            .map(|p| GraphVertex::ExternalPort(Arc::new(p.clone())))
                            .collect();
                        Box::new(ports.into_iter()) as VertexIterator<'a, Self::Vertex>
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
        _coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'a, V, bool> {
        Box::new(contexts.map(|ctx| (ctx, true)))
    }
}
```

### `crates/flayer/src/graph/unified_graph.graphql`

```graphql
schema {
    query: Query
}

type Query {
    # Internal entry points
    Source: [Source!]!
    Track: [Track!]!
    Master: [Master!]!

    # External entry points
    PipeWireNode(media_class: String): [PipeWireNode!]!
}

# Internal graph types

type Source {
    id: String!
    hash: String!
    gain: Float!

    outputs: [InternalNode!]!
}

type Track {
    name: String!
    volume: Float!
    pan: Float!
    muted: Boolean!

    inputs: [Source!]!
    outputs: [InternalNode!]!
}

type Master {
    pipewire_name: String

    inputs: [Track!]!
    pipewire_node: PipeWireNode
}

type Effect {
    id: String!
    effect_type: String!

    inputs: [InternalNode!]!
    outputs: [InternalNode!]!
}

union InternalNode = Source | Track | Master | Effect

# External graph types (PipeWire)

type PipeWireNode {
    id: Int!
    name: String!
    description: String
    media_class: String

    ports: [PipeWirePort!]!
    connections: [PipeWireNode!]!
}

type PipeWirePort {
    id: Int!
    name: String!
    direction: String!
    media_type: String
}
```

## Example Queries

### Trace signal from clip to speaker

```graphql
query SignalPath {
    Source {
        id @filter(op: "=", value: ["$clip_id"])
        hash @output
        outputs {
            ... on Track {
                name @output(name: "track_name")
                outputs {
                    ... on Master {
                        pipewire_node {
                            name @output(name: "pipewire_node")
                            connections {
                                name @output(name: "destination")
                                media_class @filter(op: "=", value: ["Audio/Sink"])
                            }
                        }
                    }
                }
            }
        }
    }
}
```

### Find all muted tracks and what they're connected to

```graphql
query MutedTracks {
    Track {
        muted @filter(op: "=", value: [true])
        name @output
        inputs {
            hash @output(name: "source_hash")
        }
    }
}
```

### List all PipeWire audio sinks available for routing

```graphql
query AudioSinks {
    PipeWireNode(media_class: "Audio/Sink") {
        name @output
        description @output
        ports {
            name @output(name: "port_name")
            direction @filter(op: "=", value: ["in"])
        }
    }
}
```

## Parallel Rendering

Multiple `RenderGraph` instances can render in parallel since they're isolated:

```rust
use rayon::prelude::*;

impl TimelineManager {
    pub fn render_all(&self, duration_samples: usize) -> Vec<AudioBuffer> {
        self.timelines.par_iter()
            .map(|live| {
                // Each timeline has its own RenderGraph
                // No shared state = safe parallelism
                render_timeline(&live.render_graph, duration_samples)
            })
            .collect()
    }
}
```

## Acceptance Criteria

- [ ] `RenderGraph::from_timeline()` builds correct internal topology
- [ ] `LiveTimeline` connects internal master to PipeWire node name
- [ ] `UnifiedGraphAdapter` queries span both internal and external
- [ ] Parallel rendering of multiple timelines works
- [ ] Cross-layer queries (Master → PipeWireNode) resolve correctly

## Future Extensions

1. **Sidechain routing** - Cross-timeline audio connections (needs PipeWire links)
2. **Send/return** - Aux buses routed through PipeWire
3. **Live graph updates** - Modify topology during playback
4. **Latency compensation** - Query PipeWire for latency, compensate in internal graph
5. **MIDI routing** - PipeWire MIDI nodes as sources/sinks
