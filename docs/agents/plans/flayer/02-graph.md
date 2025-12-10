# 02: Graph

**File:** `src/graph.rs`
**Focus:** DAG topology, petgraph wrapper, no rendering concerns
**Dependencies:** `petgraph`, `uuid`

---

## Task

Create `crates/flayer/src/graph.rs` wrapping petgraph's StableGraph. Implement node/edge management, topological ordering, and traversal methods.

**Why this first?** Rendering needs topological order. Resolution needs to find nodes. Patterns build graphs. The graph is the central data structure everything else manipulates.

**Deliverables:**
1. `graph.rs` with Graph struct and all methods
2. Tests for: add/connect/disconnect, toposort, cycle detection, upstream/downstream traversal

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ Buffer allocation — task 04
- ❌ Processing loop — task 04
- ❌ Trustfall queries — task 06

Focus ONLY on graph topology and traversal.

---

## petgraph Patterns

```rust
use petgraph::stable_graph::{StableGraph, NodeIndex, EdgeIndex};
use petgraph::algo::toposort;
use petgraph::Direction;

// Topological sort - returns processing order or error if cycle
let order: Result<Vec<NodeIndex>, _> = toposort(&graph, None);

// Traverse neighbors
for neighbor in graph.neighbors_directed(node, Direction::Incoming) { }
for neighbor in graph.neighbors_directed(node, Direction::Outgoing) { }

// Find edge between nodes
let edge: Option<EdgeIndex> = graph.find_edge(source, dest);
```

---

## Types

```rust
#[derive(Debug, Clone)]
pub struct Edge {
    pub source_port: String,
    pub dest_port: String,
    pub gain: f64,
    pub active: bool,
}

pub struct Graph {
    inner: StableGraph<BoxedNode, Edge>,
    index_map: HashMap<Uuid, NodeIndex>,
    topo_order: Option<Vec<NodeIndex>>,  // cached, invalidated on modification
}

#[derive(Debug, Clone)]
pub enum GraphError {
    NodeNotFound(Uuid),
    PortNotFound { node: Uuid, port: String },
    TypeMismatch { expected: SignalType, got: SignalType },
    CycleDetected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<NodeDescriptor>,
    pub edges: Vec<EdgeSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeSnapshot {
    pub source_id: Uuid,
    pub source_port: String,
    pub dest_id: Uuid,
    pub dest_port: String,
    pub gain: f64,
    pub active: bool,
}
```

---

## Graph Methods to Implement

**Construction:**
- `new() -> Self`
- `add_node(node: BoxedNode) -> NodeIndex`
- `remove_node(id: Uuid) -> Option<BoxedNode>`

**Connections:**
- `connect(source_id, source_port, dest_id, dest_port) -> Result<EdgeIndex, GraphError>`
- `disconnect(source_id, dest_id) -> bool`

**Queries:**
- `node(&self, id: Uuid) -> Option<&BoxedNode>`
- `node_mut(&mut self, id: Uuid) -> Option<&mut BoxedNode>`
- `node_ids(&self) -> Vec<Uuid>`
- `processing_order(&mut self) -> Result<&[NodeIndex], GraphError>` (cached toposort)

**Internal accessors (for rendering):**
- `node_at(&self, index: NodeIndex) -> Option<&BoxedNode>`
- `node_at_mut(&mut self, index: NodeIndex) -> Option<&mut BoxedNode>`
- `incoming_edges(&self, index) -> impl Iterator<Item = (NodeIndex, &Edge)>`
- `outgoing_edges(&self, index) -> impl Iterator<Item = (NodeIndex, &Edge)>`
- `index_of(&self, id: Uuid) -> Option<NodeIndex>`

**Traversal:**
- `sources(&self) -> Vec<NodeIndex>` (nodes with no incoming edges)
- `sinks(&self) -> Vec<NodeIndex>` (nodes with no outgoing edges)
- `upstream(&self, id: Uuid) -> Vec<Uuid>`
- `downstream(&self, id: Uuid) -> Vec<Uuid>`
- `signal_path(&self, source: Uuid, sink: Uuid) -> Option<Vec<Uuid>>`
- `find_by_type(&self, type_prefix: &str) -> Vec<Uuid>`

**Modification:**
- `insert_between(new_node, before_id, after_id) -> Result<Uuid, GraphError>`
- `replace_node(old_id, new_node) -> Result<(), GraphError>`
- `bypass_node(id) -> Result<(), GraphError>`

**Serialization:**
- `snapshot(&self) -> GraphSnapshot`

**Internal:**
- `invalidate_topo(&mut self)` — clear cached order
- `validate_connection(...)` — check port types match

---

## Port Validation Rule

When connecting `source_port` to `dest_port`, their `SignalType` must match:
- Audio → Audio ✓
- Midi → Midi ✓
- Audio → Midi ✗ (GraphError::TypeMismatch)

---

## Acceptance Criteria

- [ ] `add_node` + `connect` creates edges in petgraph
- [ ] `processing_order` returns topologically sorted nodes
- [ ] `CycleDetected` error when cycle would form
- [ ] `upstream`/`downstream` traverse correctly
- [ ] Port type mismatch returns `TypeMismatch` error
- [ ] `bypass_node` preserves signal flow around bypassed node
