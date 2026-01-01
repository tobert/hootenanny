# 06: Query Layer

**File:** `src/query.rs`
**Focus:** Trustfall adapter exposing Chaosgarden to all participants
**Dependencies:** `trustfall`

---

## Timing Units

All Region timing uses **beats** (not seconds or samples):

| Schema Field | Description | Unit |
|--------------|-------------|------|
| `position` | Start position | beats |
| `duration` | Length | beats |
| `end` | Computed end (position + duration) | beats |

The `schedule` MCP tool uses `at` for position—same concept, different name.

---

## Task

Create `crates/chaosgarden/src/query.rs` with Trustfall adapter. Use `trustfall_stubgen` to generate skeleton from schema, then implement the adapter methods.

**Why this matters:** Trustfall isn't about optimizing known queries. It's about enabling questions we haven't thought of yet. Every participant—human, agent, model—can ask anything about the performance state.

**Philosophy:** The schema grows as we discover what participants need to perceive. Start minimal, extend based on actual usage.

**Deliverables:**
1. `chaosgarden.graphql` schema file
2. `query.rs` with ChaosgardenAdapter implementing Adapter trait
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
- ❌ Artifact content queries — use hootenanny's adapter
- ❌ Complex analytics — start with structure queries

Focus ONLY on Region, Node, Edge, time, and latent state queries.

**Approach:** Run `trustfall_stubgen --schema chaosgarden.graphql --target src/query/` to generate boilerplate.

---

## The Vision

Today a participant might ask: "What regions are in the chorus?"
Tomorrow: "Show me a spectrogram of the loudest 4 bars"
Next month: "Which artifacts have similar harmonic content to this reference?"

The schema grows as we discover what participants need:

| Category | Today | Future |
|----------|-------|--------|
| **Structure** | Regions, Nodes, Edges | Tracks, Sections, Projects |
| **State** | Latent status, job progress | Approval history, decision audit |
| **Time** | Beat/second conversion | Phrase boundaries, tempo events |
| **Perception** | - | Spectrograms, waveforms, classifiers |
| **Relationships** | Upstream/downstream | Similarity, influence, lineage |
| **Capabilities** | - | Node capabilities, available tools |

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
// trustfall_stubgen --schema chaosgarden.graphql --target src/query/
```

---

## Schema (chaosgarden.graphql)

```graphql
schema {
    query: Query
}

type Query {
    # Region queries
    Region(id: ID): [Region!]!
    RegionInRange(start: Float!, end: Float!): [Region!]!
    RegionByStatus(status: String!): [Region!]!
    LatentRegion: [Region!]!
    PlayableRegion: [Region!]!

    # Node queries
    Node(id: ID, type_prefix: String): [Node!]!
    NodeByCapability(realtime: Boolean, offline: Boolean): [Node!]!

    # Graph structure
    Edge: [Edge!]!

    # I/O queries
    Output: [Output!]!
    Input: [Input!]!
    MidiDevice(direction: String): [MidiDevice!]!

    # Time queries
    TempoAt(beat: Float!): Float!
    BeatToSecond(beat: Float!): Float!
    SecondToBeat(second: Float!): Float!
    BeatToSample(beat: Float!, sample_rate: Int!): Int!

    # Job queries
    RunningJob: [Job!]!
    PendingApproval: [Approval!]!

    # Participant/Capability queries
    Participant(id: ID, kind: String, online: Boolean): [Participant!]!
    ParticipantWithCapability(uri: String!, available: Boolean): [Participant!]!
    Capability(uriPrefix: String, available: Boolean): [Capability!]!

    # Lifecycle/Grooming queries
    CurrentGeneration: Int!
    TombstonedRegion: [Region!]!
    TombstonedParticipant: [Participant!]!
    StaleSince(generation: Int!): [Participant!]!

    # Tag/Label queries
    ParticipantByTag(tag: String!): [Participant!]!
    RegionByTag(tag: String!): [Region!]!
    ParticipantByUserLabel(label: String!): Participant
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

    # Resolved content (null if not resolved)
    is_resolved: Boolean!
    is_approved: Boolean!
    is_playable: Boolean!
    content_hash: String
    content_type: String
    artifact_id: String

    # Generation info (for latent regions)
    generation_tool: String
    generation_params: String

    # Lifecycle (for grooming)
    lifecycle: Lifecycle!
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

type Job {
    id: ID!
    region_id: ID!
    tool: String!
    progress: Float!
    started_at: String
}

type Approval {
    region_id: ID!
    artifact_id: String!
    content_type: String!
    resolved_at: String!
}

type Participant {
    id: ID!
    kind: String!
    name: String!
    online: Boolean!
    last_seen: String
    tags: [String!]!

    capabilities: [Capability!]!
    capabilitiesMatching(uriPrefix: String!): [Capability!]!

    # Identity hints for device reconciliation
    identity_hints: IdentityHints!

    # Lifecycle (for grooming)
    lifecycle: Lifecycle!
}

type IdentityHints {
    product_name: String
    manufacturer: String
    serial_number: String
    usb_vendor_id: Int
    usb_product_id: Int
    alsa_card_name: String
    mac_address: String
    ipv4_address: String
    ipv6_address: String
    user_label: String
}

type Lifecycle {
    created_at: String!
    created_generation: Int!
    last_touched_at: String!
    last_touched_generation: Int!
    tombstoned_at: String
    tombstoned_generation: Int
    permanent: Boolean!
    is_alive: Boolean!
}

type Capability {
    uri: String!
    name: String!
    description: String
    available: Boolean!
    confidence: Float

    constraints: [Constraint!]!
    provider: Participant!
}

type Constraint {
    key: String!
    kind: String!
    value: String!
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
    Job(JobVertex),
    Approval(ApprovalVertex),
    Participant(Arc<Participant>),
    Capability(CapabilityVertex),
    Constraint(ConstraintVertex),
}

pub struct ChaosgardenAdapter {
    regions: Arc<RwLock<Vec<Region>>>,
    graph: Arc<RwLock<Graph>>,
    io_manager: Option<Arc<ExternalIOManager>>,
    latent_manager: Arc<LatentManager>,
    tempo_map: Arc<TempoMap>,
    capability_registry: Arc<CapabilityRegistry>,
    schema: Arc<Schema>,
}

impl ChaosgardenAdapter {
    pub fn new(
        regions: Arc<RwLock<Vec<Region>>>,
        graph: Arc<RwLock<Graph>>,
        latent_manager: Arc<LatentManager>,
        tempo_map: Arc<TempoMap>,
        capability_registry: Arc<CapabilityRegistry>,
    ) -> Result<Self>;

    /// Load schema from embedded string or file
    pub fn schema() -> &'static Schema;
}
```

---

## Example Queries Participants Will Run

### Find regions needing attention

```graphql
# Latent regions that have resolved but need approval
{
    PendingApproval {
        region_id @output
        artifact_id @output
        content_type @output
    }
}
```

### Understand generation progress

```graphql
# All running jobs with progress
{
    RunningJob {
        id @output
        tool @output
        progress @output
        region_id @output
    }
}
```

### Navigate the graph

```graphql
# Trace signal path from source to output
{
    Node(type_prefix: "source.") {
        name @output
        downstream {
            name @output
            type_id @output
            downstream {
                name @output
            }
        }
    }
}
```

### Find playable content in a range

```graphql
# What can actually play in the chorus (beats 32-64)?
{
    RegionInRange(start: 32.0, end: 64.0) {
        name @output
        position @output
        is_playable @filter(op: "=", value: ["$true"])
        content_type @output
    }
}
```

### Find capable nodes

```graphql
# What nodes can run in realtime?
{
    NodeByCapability(realtime: true) {
        name @output
        type_id @output
        latency_samples @output
    }
}
```

### Find participants who can generate MIDI

```graphql
# Who can generate MIDI?
{
    ParticipantWithCapability(uri: "gen:midi", available: true) {
        name @output
        kind @output
        capabilities {
            uri @filter(op: "has_prefix", value: ["gen:"])
            name @output
            available @output
        }
    }
}
```

### Find all available capabilities by namespace

```graphql
# What generation capabilities are available?
{
    Capability(uriPrefix: "gen:", available: true) {
        uri @output
        name @output
        provider {
            name @output
            kind @output
        }
        constraints {
            key @output
            value @output
        }
    }
}
```

### Find human approvers

```graphql
# Who can approve content?
{
    Participant(kind: "human", online: true) {
        name @output
        capabilitiesMatching(uriPrefix: "hitl:") {
            uri @output
            available @output
        }
    }
}
```

---

## Extending the Schema

When adding new queryable types:

1. **Add to schema.graphql** — Entry points in `Query`, types as new blocks
2. **Add Vertex variant** — `enum Vertex { ..., NewType(Arc<NewType>) }`
3. **Implement resolution** — `resolve_starting_vertices`, `resolve_property`, `resolve_neighbors`
4. **Wire data sources** — Adapter pulls from regions, graph, latent_manager, etc.

### Future Extensions

```graphql
# Future: Classifier results
type ClassifierResult {
    artifact_id: ID!
    classifier: String!
    score: Float!
    label: String
}

# Future: Spectrogram data
type Spectrogram {
    artifact_id: ID!
    bins: Int!
    frames: Int!
    data_url: String!  # base64 or file reference
}
```

> **Note:** Capability discovery is now implemented in the main schema (see Participant, Capability, Constraint types).

---

## Integration with Hootenanny

Chaosgarden's adapter focuses on performance state. For artifact content, lineage, and PipeWire routing, use hootenanny's existing Trustfall adapter:

```rust
// Compose adapters for unified queries
pub struct ComposedAdapter {
    chaosgarden: ChaosgardenAdapter,
    hootenanny: AudioGraphAdapter,
}
```

This allows queries that span both domains:
- "Find regions whose artifacts are tagged 'jazzy'"
- "Which PipeWire nodes are connected to our master output?"

---

## Acceptance Criteria

- [ ] Schema parses without errors
- [ ] `resolve_starting_vertices` returns regions/nodes/jobs
- [ ] `resolve_property` extracts all scalar fields
- [ ] `resolve_neighbors` traverses upstream/downstream
- [ ] Latent state fields resolve correctly
- [ ] Job and Approval queries work
- [ ] Time conversion queries return accurate results
- [ ] Example queries execute and return correct results
