# 06: Query Layer

**File:** `src/query.rs`
**Focus:** Trustfall adapter exposing Flayer to agents
**Dependencies:** `trustfall`

---

## Task

Create `crates/flayer/src/query.rs` with Trustfall adapter. Use `trustfall_stubgen` to generate skeleton from schema, then implement the 4 adapter methods.

**Why this first?** Agents need to reason about the graph — find regions, trace signal paths, check resolution state. Trustfall makes everything queryable. This is how AI collaborates with the timeline.

**Deliverables:**
1. `flayer.graphql` schema file
2. `query.rs` with FlayerAdapter implementing Adapter trait
3. Tests executing example queries against mock data

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ Live PipeWire node queries — use hootenanny's adapter
- ❌ Artifact queries — use hootenanny's adapter
- ❌ Pattern-level queries (Track, Bus) — expose primitives only

Focus ONLY on Region, Node, Edge, and time conversion queries.

**Approach:** Run `trustfall_stubgen --schema flayer.graphql --target src/query/` to generate boilerplate.

---

## trustfall Pattern

```rust
use trustfall::{Schema, FieldValue, provider::*};

// Adapter trait requires 4 methods:
pub trait Adapter<'vertex> {
    type Vertex: Clone + Debug;

    fn resolve_starting_vertices(...) -> VertexIterator<'vertex, Self::Vertex>;
    fn resolve_property(...) -> ContextOutcomeIterator<...>;
    fn resolve_neighbors(...) -> ContextOutcomeIterator<...>;
    fn resolve_coercion(...) -> ContextOutcomeIterator<...>;
}

// Generate adapter skeleton:
// trustfall_stubgen --schema flayer.graphql --target src/query/
```

---

## Schema (flayer.graphql)

```graphql
schema {
    query: Query
}

type Query {
    Region(id: ID): [Region!]!
    RegionInRange(start: Float!, end: Float!): [Region!]!
    Node(id: ID, type_prefix: String): [Node!]!
    Edge: [Edge!]!
    Output: [Output!]!
    Input: [Input!]!
    MidiDevice(direction: String): [MidiDevice!]!
    TempoAt(beat: Float!): Float!
    BeatToSecond(beat: Float!): Float!
    SecondToBeat(second: Float!): Float!
}

type Region {
    id: ID!
    position: Float!
    duration: Float!
    end: Float!
    behavior_type: String!
    is_resolved: Boolean!
    content_hash: String
    content_type: String
    generation_tool: String
    generation_params: String
    name: String
    tags: [String!]!
}

type Node {
    id: ID!
    name: String!
    type_id: String!
    inputs: [Port!]!
    outputs: [Port!]!
    upstream: [Node!]!
    downstream: [Node!]!
    signal_path_to(target: ID!): [Node!]!
}

type Port {
    name: String!
    signal_type: String!
}

type Edge {
    source: Node!
    source_port: String!
    dest: Node!
    dest_port: String!
    gain: Float!
    active: Boolean!
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
```

---

## Adapter Types

```rust
#[derive(Debug, Clone)]
pub enum Vertex {
    Region(Arc<Region>),
    Node(NodeVertex),
    Port(PortVertex),
    Edge(EdgeVertex),
    Output(OutputVertex),
    Input(InputVertex),
    MidiDevice(MidiVertex),
}

pub struct FlayerAdapter {
    regions: Vec<Arc<Region>>,
    graph: Arc<Graph>,
    io_manager: Arc<ExternalIOManager>,
    tempo_map: Arc<TempoMap>,
    schema: Arc<Schema>,
}
```

---

## Key Queries Agents Will Use

```graphql
# Find regions in a time range
{ RegionInRange(start: 24.0, end: 40.0) { name @output position @output } }

# Find unresolved generative regions
{ Region { is_resolved @filter(op: "=", value: [false]) name @output } }

# Trace signal path
{ Node(type_prefix: "source.") { name @output downstream { name @output } } }
```

---

## Acceptance Criteria

- [ ] Schema parses without errors
- [ ] `resolve_starting_vertices` returns regions/nodes
- [ ] `resolve_property` extracts all scalar fields
- [ ] `resolve_neighbors` traverses upstream/downstream
- [ ] Example queries execute and return correct results
