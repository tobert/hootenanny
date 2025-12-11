//! Audio processing graph
//!
//! DAG topology using petgraph's StableGraph. Handles node/edge management,
//! topological ordering for processing, and traversal methods.

use std::collections::HashMap;

use petgraph::algo::toposort;
use petgraph::stable_graph::{EdgeIndex, NodeIndex, StableGraph};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::primitives::{BoxedNode, NodeDescriptor, Port, SignalType};

/// An edge in the audio graph, connecting ports between nodes
#[derive(Debug, Clone)]
pub struct Edge {
    pub source_port: String,
    pub dest_port: String,
    pub gain: f64,
    pub active: bool,
}

impl Edge {
    pub fn new(source_port: impl Into<String>, dest_port: impl Into<String>) -> Self {
        Self {
            source_port: source_port.into(),
            dest_port: dest_port.into(),
            gain: 1.0,
            active: true,
        }
    }

    pub fn with_gain(mut self, gain: f64) -> Self {
        self.gain = gain;
        self
    }
}

/// Errors that can occur during graph operations
#[derive(Debug, Clone)]
pub enum GraphError {
    NodeNotFound(Uuid),
    PortNotFound {
        node: Uuid,
        port: String,
    },
    TypeMismatch {
        expected: SignalType,
        got: SignalType,
    },
    CycleDetected,
    EdgeNotFound {
        source: Uuid,
        dest: Uuid,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::NodeNotFound(id) => write!(f, "node not found: {}", id),
            GraphError::PortNotFound { node, port } => {
                write!(f, "port not found: {}.{}", node, port)
            }
            GraphError::TypeMismatch { expected, got } => {
                write!(
                    f,
                    "signal type mismatch: expected {:?}, got {:?}",
                    expected, got
                )
            }
            GraphError::CycleDetected => write!(f, "cycle detected in graph"),
            GraphError::EdgeNotFound { source, dest } => {
                write!(f, "edge not found between {} and {}", source, dest)
            }
        }
    }
}

impl std::error::Error for GraphError {}

/// Serializable snapshot of a graph edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeSnapshot {
    pub source_id: Uuid,
    pub source_port: String,
    pub dest_id: Uuid,
    pub dest_port: String,
    pub gain: f64,
    pub active: bool,
}

/// Serializable snapshot of the entire graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<NodeDescriptor>,
    pub edges: Vec<EdgeSnapshot>,
}

/// The audio processing graph
///
/// Wraps petgraph's StableGraph with Uuid-based node lookup and
/// cached topological ordering.
pub struct Graph {
    inner: StableGraph<BoxedNode, Edge>,
    index_map: HashMap<Uuid, NodeIndex>,
    topo_order: Option<Vec<NodeIndex>>,
}

impl Graph {
    /// Create a new empty graph
    pub fn new() -> Self {
        Self {
            inner: StableGraph::new(),
            index_map: HashMap::new(),
            topo_order: None,
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: BoxedNode) -> NodeIndex {
        let id = node.descriptor().id;
        let index = self.inner.add_node(node);
        self.index_map.insert(id, index);
        self.invalidate_topo();
        index
    }

    /// Remove a node from the graph
    pub fn remove_node(&mut self, id: Uuid) -> Option<BoxedNode> {
        let index = self.index_map.remove(&id)?;
        let node = self.inner.remove_node(index)?;
        self.invalidate_topo();
        Some(node)
    }

    /// Connect two nodes by their port names
    pub fn connect(
        &mut self,
        source_id: Uuid,
        source_port: &str,
        dest_id: Uuid,
        dest_port: &str,
    ) -> Result<EdgeIndex, GraphError> {
        let source_idx = self
            .index_map
            .get(&source_id)
            .copied()
            .ok_or(GraphError::NodeNotFound(source_id))?;

        let dest_idx = self
            .index_map
            .get(&dest_id)
            .copied()
            .ok_or(GraphError::NodeNotFound(dest_id))?;

        self.validate_connection(source_id, source_port, dest_id, dest_port)?;

        let edge = Edge::new(source_port, dest_port);
        let edge_idx = self.inner.add_edge(source_idx, dest_idx, edge);

        if self.has_cycle() {
            self.inner.remove_edge(edge_idx);
            return Err(GraphError::CycleDetected);
        }

        self.invalidate_topo();
        Ok(edge_idx)
    }

    /// Disconnect two nodes
    pub fn disconnect(&mut self, source_id: Uuid, dest_id: Uuid) -> bool {
        let source_idx = match self.index_map.get(&source_id) {
            Some(idx) => *idx,
            None => return false,
        };

        let dest_idx = match self.index_map.get(&dest_id) {
            Some(idx) => *idx,
            None => return false,
        };

        if let Some(edge_idx) = self.inner.find_edge(source_idx, dest_idx) {
            self.inner.remove_edge(edge_idx);
            self.invalidate_topo();
            true
        } else {
            false
        }
    }

    /// Get a reference to a node by UUID
    pub fn node(&self, id: Uuid) -> Option<&BoxedNode> {
        let index = self.index_map.get(&id)?;
        self.inner.node_weight(*index)
    }

    /// Get a mutable reference to a node by UUID
    pub fn node_mut(&mut self, id: Uuid) -> Option<&mut BoxedNode> {
        let index = self.index_map.get(&id)?;
        self.inner.node_weight_mut(*index)
    }

    /// Get all node UUIDs in the graph
    pub fn node_ids(&self) -> Vec<Uuid> {
        self.index_map.keys().copied().collect()
    }

    /// Get the number of nodes in the graph
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Get the number of edges in the graph
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Get the cached topological processing order, computing if necessary
    pub fn processing_order(&mut self) -> Result<&[NodeIndex], GraphError> {
        if self.topo_order.is_none() {
            let order = toposort(&self.inner, None).map_err(|_| GraphError::CycleDetected)?;
            self.topo_order = Some(order);
        }
        Ok(self.topo_order.as_ref().unwrap())
    }

    /// Get a node by its internal index
    pub fn node_at(&self, index: NodeIndex) -> Option<&BoxedNode> {
        self.inner.node_weight(index)
    }

    /// Get a mutable reference to a node by its internal index
    pub fn node_at_mut(&mut self, index: NodeIndex) -> Option<&mut BoxedNode> {
        self.inner.node_weight_mut(index)
    }

    /// Get the internal index for a UUID
    pub fn index_of(&self, id: Uuid) -> Option<NodeIndex> {
        self.index_map.get(&id).copied()
    }

    /// Get incoming edges to a node
    pub fn incoming_edges(&self, index: NodeIndex) -> impl Iterator<Item = (NodeIndex, &Edge)> {
        self.inner
            .edges_directed(index, Direction::Incoming)
            .map(|e| (e.source(), e.weight()))
    }

    /// Get outgoing edges from a node
    pub fn outgoing_edges(&self, index: NodeIndex) -> impl Iterator<Item = (NodeIndex, &Edge)> {
        self.inner
            .edges_directed(index, Direction::Outgoing)
            .map(|e| (e.target(), e.weight()))
    }

    /// Get source nodes (no incoming edges)
    pub fn sources(&self) -> Vec<NodeIndex> {
        self.inner
            .node_indices()
            .filter(|&idx| {
                self.inner
                    .edges_directed(idx, Direction::Incoming)
                    .next()
                    .is_none()
            })
            .collect()
    }

    /// Get sink nodes (no outgoing edges)
    pub fn sinks(&self) -> Vec<NodeIndex> {
        self.inner
            .node_indices()
            .filter(|&idx| {
                self.inner
                    .edges_directed(idx, Direction::Outgoing)
                    .next()
                    .is_none()
            })
            .collect()
    }

    /// Get all nodes upstream of the given node (feeding into it)
    pub fn upstream(&self, id: Uuid) -> Vec<Uuid> {
        let Some(start_idx) = self.index_map.get(&id) else {
            return vec![];
        };

        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![*start_idx];

        while let Some(idx) = stack.pop() {
            for neighbor in self.inner.neighbors_directed(idx, Direction::Incoming) {
                if visited.insert(neighbor) {
                    if let Some(node) = self.inner.node_weight(neighbor) {
                        result.push(node.descriptor().id);
                    }
                    stack.push(neighbor);
                }
            }
        }

        result
    }

    /// Get all nodes downstream of the given node (fed by it)
    pub fn downstream(&self, id: Uuid) -> Vec<Uuid> {
        let Some(start_idx) = self.index_map.get(&id) else {
            return vec![];
        };

        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![*start_idx];

        while let Some(idx) = stack.pop() {
            for neighbor in self.inner.neighbors_directed(idx, Direction::Outgoing) {
                if visited.insert(neighbor) {
                    if let Some(node) = self.inner.node_weight(neighbor) {
                        result.push(node.descriptor().id);
                    }
                    stack.push(neighbor);
                }
            }
        }

        result
    }

    /// Find a signal path from source to sink
    pub fn signal_path(&self, source: Uuid, sink: Uuid) -> Option<Vec<Uuid>> {
        let source_idx = self.index_map.get(&source)?;
        let sink_idx = self.index_map.get(&sink)?;

        let mut path = Vec::new();
        let mut visited = std::collections::HashSet::new();

        if self.dfs_path(*source_idx, *sink_idx, &mut path, &mut visited) {
            Some(path)
        } else {
            None
        }
    }

    fn dfs_path(
        &self,
        current: NodeIndex,
        target: NodeIndex,
        path: &mut Vec<Uuid>,
        visited: &mut std::collections::HashSet<NodeIndex>,
    ) -> bool {
        if !visited.insert(current) {
            return false;
        }

        if let Some(node) = self.inner.node_weight(current) {
            path.push(node.descriptor().id);
        }

        if current == target {
            return true;
        }

        for neighbor in self.inner.neighbors_directed(current, Direction::Outgoing) {
            if self.dfs_path(neighbor, target, path, visited) {
                return true;
            }
        }

        path.pop();
        false
    }

    /// Find nodes by type prefix (e.g., "source.", "effect.")
    pub fn find_by_type(&self, type_prefix: &str) -> Vec<Uuid> {
        self.inner
            .node_weights()
            .filter(|node| node.descriptor().type_id.starts_with(type_prefix))
            .map(|node| node.descriptor().id)
            .collect()
    }

    /// Insert a new node between two existing nodes
    pub fn insert_between(
        &mut self,
        new_node: BoxedNode,
        before_id: Uuid,
        after_id: Uuid,
    ) -> Result<Uuid, GraphError> {
        let before_idx = self
            .index_map
            .get(&before_id)
            .copied()
            .ok_or(GraphError::NodeNotFound(before_id))?;

        let after_idx = self
            .index_map
            .get(&after_id)
            .copied()
            .ok_or(GraphError::NodeNotFound(after_id))?;

        let edge_idx =
            self.inner
                .find_edge(before_idx, after_idx)
                .ok_or(GraphError::EdgeNotFound {
                    source: before_id,
                    dest: after_id,
                })?;

        let old_edge = self.inner.remove_edge(edge_idx).unwrap();

        let new_id = new_node.descriptor().id;
        let new_idx = self.add_node(new_node);

        self.inner.add_edge(
            before_idx,
            new_idx,
            Edge::new(&old_edge.source_port, "input"),
        );
        self.inner
            .add_edge(new_idx, after_idx, Edge::new("output", &old_edge.dest_port));

        self.invalidate_topo();
        Ok(new_id)
    }

    /// Replace a node while preserving connections
    pub fn replace_node(&mut self, old_id: Uuid, new_node: BoxedNode) -> Result<(), GraphError> {
        let old_idx = self
            .index_map
            .get(&old_id)
            .copied()
            .ok_or(GraphError::NodeNotFound(old_id))?;

        let incoming: Vec<_> = self
            .inner
            .edges_directed(old_idx, Direction::Incoming)
            .map(|e| (e.source(), e.weight().clone()))
            .collect();

        let outgoing: Vec<_> = self
            .inner
            .edges_directed(old_idx, Direction::Outgoing)
            .map(|e| (e.target(), e.weight().clone()))
            .collect();

        self.index_map.remove(&old_id);
        self.inner.remove_node(old_idx);

        let new_id = new_node.descriptor().id;
        let new_idx = self.inner.add_node(new_node);
        self.index_map.insert(new_id, new_idx);

        for (source_idx, edge) in incoming {
            self.inner.add_edge(source_idx, new_idx, edge);
        }
        for (target_idx, edge) in outgoing {
            self.inner.add_edge(new_idx, target_idx, edge);
        }

        self.invalidate_topo();
        Ok(())
    }

    /// Bypass a node, connecting its inputs directly to its outputs
    pub fn bypass_node(&mut self, id: Uuid) -> Result<(), GraphError> {
        let idx = self
            .index_map
            .get(&id)
            .copied()
            .ok_or(GraphError::NodeNotFound(id))?;

        let incoming: Vec<_> = self
            .inner
            .edges_directed(idx, Direction::Incoming)
            .map(|e| (e.source(), e.weight().clone()))
            .collect();

        let outgoing: Vec<_> = self
            .inner
            .edges_directed(idx, Direction::Outgoing)
            .map(|e| (e.target(), e.weight().clone()))
            .collect();

        for (source_idx, in_edge) in &incoming {
            for (target_idx, out_edge) in &outgoing {
                let bypass_edge = Edge {
                    source_port: in_edge.source_port.clone(),
                    dest_port: out_edge.dest_port.clone(),
                    gain: in_edge.gain * out_edge.gain,
                    active: in_edge.active && out_edge.active,
                };
                self.inner.add_edge(*source_idx, *target_idx, bypass_edge);
            }
        }

        self.index_map.remove(&id);
        self.inner.remove_node(idx);
        self.invalidate_topo();

        Ok(())
    }

    /// Create a serializable snapshot of the graph
    pub fn snapshot(&self) -> GraphSnapshot {
        let nodes: Vec<_> = self
            .inner
            .node_weights()
            .map(|node| node.descriptor().clone())
            .collect();

        let edges: Vec<_> = self
            .inner
            .edge_references()
            .filter_map(|e| {
                let source = self.inner.node_weight(e.source())?;
                let target = self.inner.node_weight(e.target())?;
                let edge = e.weight();

                Some(EdgeSnapshot {
                    source_id: source.descriptor().id,
                    source_port: edge.source_port.clone(),
                    dest_id: target.descriptor().id,
                    dest_port: edge.dest_port.clone(),
                    gain: edge.gain,
                    active: edge.active,
                })
            })
            .collect();

        GraphSnapshot { nodes, edges }
    }

    fn invalidate_topo(&mut self) {
        self.topo_order = None;
    }

    fn has_cycle(&self) -> bool {
        toposort(&self.inner, None).is_err()
    }

    fn validate_connection(
        &self,
        source_id: Uuid,
        source_port: &str,
        dest_id: Uuid,
        dest_port: &str,
    ) -> Result<(), GraphError> {
        let source_node = self
            .node(source_id)
            .ok_or(GraphError::NodeNotFound(source_id))?;
        let dest_node = self
            .node(dest_id)
            .ok_or(GraphError::NodeNotFound(dest_id))?;

        let source_port_def = find_port(&source_node.descriptor().outputs, source_port).ok_or(
            GraphError::PortNotFound {
                node: source_id,
                port: source_port.to_string(),
            },
        )?;

        let dest_port_def = find_port(&dest_node.descriptor().inputs, dest_port).ok_or(
            GraphError::PortNotFound {
                node: dest_id,
                port: dest_port.to_string(),
            },
        )?;

        if source_port_def.signal_type != dest_port_def.signal_type {
            return Err(GraphError::TypeMismatch {
                expected: dest_port_def.signal_type,
                got: source_port_def.signal_type,
            });
        }

        Ok(())
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

fn find_port<'a>(ports: &'a [Port], name: &str) -> Option<&'a Port> {
    ports.iter().find(|p| p.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{NodeCapabilities, ProcessContext, ProcessError, SignalBuffer};

    struct TestNode {
        descriptor: NodeDescriptor,
    }

    impl TestNode {
        fn new(name: &str, type_id: &str) -> Self {
            Self {
                descriptor: NodeDescriptor {
                    id: Uuid::new_v4(),
                    name: name.to_string(),
                    type_id: type_id.to_string(),
                    inputs: vec![Port {
                        name: "input".to_string(),
                        signal_type: SignalType::Audio,
                    }],
                    outputs: vec![Port {
                        name: "output".to_string(),
                        signal_type: SignalType::Audio,
                    }],
                    latency_samples: 0,
                    capabilities: NodeCapabilities::default(),
                },
            }
        }

        fn source(name: &str) -> Self {
            Self {
                descriptor: NodeDescriptor {
                    id: Uuid::new_v4(),
                    name: name.to_string(),
                    type_id: "source.audio".to_string(),
                    inputs: vec![],
                    outputs: vec![Port {
                        name: "output".to_string(),
                        signal_type: SignalType::Audio,
                    }],
                    latency_samples: 0,
                    capabilities: NodeCapabilities::default(),
                },
            }
        }

        fn sink(name: &str) -> Self {
            Self {
                descriptor: NodeDescriptor {
                    id: Uuid::new_v4(),
                    name: name.to_string(),
                    type_id: "sink.audio".to_string(),
                    inputs: vec![Port {
                        name: "input".to_string(),
                        signal_type: SignalType::Audio,
                    }],
                    outputs: vec![],
                    latency_samples: 0,
                    capabilities: NodeCapabilities::default(),
                },
            }
        }

        fn midi_source(name: &str) -> Self {
            Self {
                descriptor: NodeDescriptor {
                    id: Uuid::new_v4(),
                    name: name.to_string(),
                    type_id: "source.midi".to_string(),
                    inputs: vec![],
                    outputs: vec![Port {
                        name: "output".to_string(),
                        signal_type: SignalType::Midi,
                    }],
                    latency_samples: 0,
                    capabilities: NodeCapabilities::default(),
                },
            }
        }
    }

    impl crate::primitives::Node for TestNode {
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

    #[test]
    fn test_add_and_connect() {
        let mut graph = Graph::new();

        let source = TestNode::source("src");
        let source_id = source.descriptor.id;

        let sink = TestNode::sink("sink");
        let sink_id = sink.descriptor.id;

        graph.add_node(Box::new(source));
        graph.add_node(Box::new(sink));

        assert_eq!(graph.node_count(), 2);

        let result = graph.connect(source_id, "output", sink_id, "input");
        assert!(result.is_ok());
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_processing_order() {
        let mut graph = Graph::new();

        let source = TestNode::source("src");
        let source_id = source.descriptor.id;

        let effect = TestNode::new("fx", "effect.gain");
        let effect_id = effect.descriptor.id;

        let sink = TestNode::sink("sink");
        let sink_id = sink.descriptor.id;

        graph.add_node(Box::new(source));
        graph.add_node(Box::new(effect));
        graph.add_node(Box::new(sink));

        graph
            .connect(source_id, "output", effect_id, "input")
            .unwrap();
        graph
            .connect(effect_id, "output", sink_id, "input")
            .unwrap();

        let order = graph.processing_order().unwrap().to_vec();
        assert_eq!(order.len(), 3);

        let source_idx = graph.index_of(source_id).unwrap();
        let effect_idx = graph.index_of(effect_id).unwrap();
        let sink_idx = graph.index_of(sink_id).unwrap();

        let source_pos = order.iter().position(|&idx| idx == source_idx).unwrap();
        let effect_pos = order.iter().position(|&idx| idx == effect_idx).unwrap();
        let sink_pos = order.iter().position(|&idx| idx == sink_idx).unwrap();

        assert!(source_pos < effect_pos);
        assert!(effect_pos < sink_pos);
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = Graph::new();

        let node1 = TestNode::new("n1", "effect.a");
        let node1_id = node1.descriptor.id;

        let node2 = TestNode::new("n2", "effect.b");
        let node2_id = node2.descriptor.id;

        graph.add_node(Box::new(node1));
        graph.add_node(Box::new(node2));

        graph
            .connect(node1_id, "output", node2_id, "input")
            .unwrap();

        let result = graph.connect(node2_id, "output", node1_id, "input");
        assert!(matches!(result, Err(GraphError::CycleDetected)));
    }

    #[test]
    fn test_type_mismatch() {
        let mut graph = Graph::new();

        let midi_source = TestNode::midi_source("midi");
        let midi_id = midi_source.descriptor.id;

        let audio_sink = TestNode::sink("audio");
        let audio_id = audio_sink.descriptor.id;

        graph.add_node(Box::new(midi_source));
        graph.add_node(Box::new(audio_sink));

        let result = graph.connect(midi_id, "output", audio_id, "input");
        assert!(matches!(result, Err(GraphError::TypeMismatch { .. })));
    }

    #[test]
    fn test_upstream_downstream() {
        let mut graph = Graph::new();

        let src1 = TestNode::source("src1");
        let src1_id = src1.descriptor.id;

        let src2 = TestNode::source("src2");
        let src2_id = src2.descriptor.id;

        let mixer = TestNode::new("mixer", "effect.mixer");
        let mixer_id = mixer.descriptor.id;

        let sink = TestNode::sink("sink");
        let sink_id = sink.descriptor.id;

        graph.add_node(Box::new(src1));
        graph.add_node(Box::new(src2));
        graph.add_node(Box::new(mixer));
        graph.add_node(Box::new(sink));

        graph.connect(src1_id, "output", mixer_id, "input").unwrap();
        graph.connect(src2_id, "output", mixer_id, "input").unwrap();
        graph.connect(mixer_id, "output", sink_id, "input").unwrap();

        let upstream = graph.upstream(mixer_id);
        assert_eq!(upstream.len(), 2);
        assert!(upstream.contains(&src1_id));
        assert!(upstream.contains(&src2_id));

        let downstream = graph.downstream(mixer_id);
        assert_eq!(downstream.len(), 1);
        assert!(downstream.contains(&sink_id));
    }

    #[test]
    fn test_sources_and_sinks() {
        let mut graph = Graph::new();

        let src = TestNode::source("src");
        let src_id = src.descriptor.id;

        let fx = TestNode::new("fx", "effect.gain");

        let sink = TestNode::sink("sink");

        let src_idx = graph.add_node(Box::new(src));
        graph.add_node(Box::new(fx));
        let sink_idx = graph.add_node(Box::new(sink));

        let sources = graph.sources();
        assert_eq!(sources.len(), 3);

        let sinks = graph.sinks();
        assert_eq!(sinks.len(), 3);

        graph
            .connect(
                src_id,
                "output",
                graph.node_at(sources[1]).unwrap().descriptor().id,
                "input",
            )
            .unwrap();

        let sources_after = graph.sources();
        assert_eq!(sources_after.len(), 2);
        assert!(sources_after.contains(&src_idx));
        assert!(sources_after.contains(&sink_idx));
    }

    #[test]
    fn test_signal_path() {
        let mut graph = Graph::new();

        let src = TestNode::source("src");
        let src_id = src.descriptor.id;

        let fx1 = TestNode::new("fx1", "effect.a");
        let fx1_id = fx1.descriptor.id;

        let fx2 = TestNode::new("fx2", "effect.b");
        let fx2_id = fx2.descriptor.id;

        let sink = TestNode::sink("sink");
        let sink_id = sink.descriptor.id;

        graph.add_node(Box::new(src));
        graph.add_node(Box::new(fx1));
        graph.add_node(Box::new(fx2));
        graph.add_node(Box::new(sink));

        graph.connect(src_id, "output", fx1_id, "input").unwrap();
        graph.connect(fx1_id, "output", fx2_id, "input").unwrap();
        graph.connect(fx2_id, "output", sink_id, "input").unwrap();

        let path = graph.signal_path(src_id, sink_id).unwrap();
        assert_eq!(path.len(), 4);
        assert_eq!(path[0], src_id);
        assert_eq!(path[1], fx1_id);
        assert_eq!(path[2], fx2_id);
        assert_eq!(path[3], sink_id);

        let no_path = graph.signal_path(sink_id, src_id);
        assert!(no_path.is_none());
    }

    #[test]
    fn test_find_by_type() {
        let mut graph = Graph::new();

        graph.add_node(Box::new(TestNode::source("src1")));
        graph.add_node(Box::new(TestNode::source("src2")));
        graph.add_node(Box::new(TestNode::new("fx", "effect.gain")));
        graph.add_node(Box::new(TestNode::sink("sink")));

        let sources = graph.find_by_type("source.");
        assert_eq!(sources.len(), 2);

        let effects = graph.find_by_type("effect.");
        assert_eq!(effects.len(), 1);

        let sinks = graph.find_by_type("sink.");
        assert_eq!(sinks.len(), 1);
    }

    #[test]
    fn test_bypass_node() {
        let mut graph = Graph::new();

        let src = TestNode::source("src");
        let src_id = src.descriptor.id;

        let fx = TestNode::new("fx", "effect.gain");
        let fx_id = fx.descriptor.id;

        let sink = TestNode::sink("sink");
        let sink_id = sink.descriptor.id;

        graph.add_node(Box::new(src));
        graph.add_node(Box::new(fx));
        graph.add_node(Box::new(sink));

        graph.connect(src_id, "output", fx_id, "input").unwrap();
        graph.connect(fx_id, "output", sink_id, "input").unwrap();

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);

        graph.bypass_node(fx_id).unwrap();

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);

        assert!(graph.node(fx_id).is_none());

        let downstream = graph.downstream(src_id);
        assert!(downstream.contains(&sink_id));
    }

    #[test]
    fn test_snapshot() {
        let mut graph = Graph::new();

        let src = TestNode::source("src");
        let src_id = src.descriptor.id;

        let sink = TestNode::sink("sink");
        let sink_id = sink.descriptor.id;

        graph.add_node(Box::new(src));
        graph.add_node(Box::new(sink));
        graph.connect(src_id, "output", sink_id, "input").unwrap();

        let snapshot = graph.snapshot();

        assert_eq!(snapshot.nodes.len(), 2);
        assert_eq!(snapshot.edges.len(), 1);
        assert_eq!(snapshot.edges[0].source_id, src_id);
        assert_eq!(snapshot.edges[0].dest_id, sink_id);
    }

    #[test]
    fn test_disconnect() {
        let mut graph = Graph::new();

        let src = TestNode::source("src");
        let src_id = src.descriptor.id;

        let sink = TestNode::sink("sink");
        let sink_id = sink.descriptor.id;

        graph.add_node(Box::new(src));
        graph.add_node(Box::new(sink));
        graph.connect(src_id, "output", sink_id, "input").unwrap();

        assert_eq!(graph.edge_count(), 1);

        let removed = graph.disconnect(src_id, sink_id);
        assert!(removed);
        assert_eq!(graph.edge_count(), 0);

        let removed_again = graph.disconnect(src_id, sink_id);
        assert!(!removed_again);
    }

    #[test]
    fn test_remove_node() {
        let mut graph = Graph::new();

        let src = TestNode::source("src");
        let src_id = src.descriptor.id;

        graph.add_node(Box::new(src));
        assert_eq!(graph.node_count(), 1);

        let removed = graph.remove_node(src_id);
        assert!(removed.is_some());
        assert_eq!(graph.node_count(), 0);
        assert!(graph.node(src_id).is_none());
    }
}
