# Audio Graph MCP Architecture

## Overview

An MCP server that provides agents with a queryable view of audio/MIDI infrastructure. Think of it as an "agentic DAW" where Claude acts as producer, with visibility into the full signal flow across hardware synths, Eurorack modules, software plugins, and network-connected compute.

**Key insight**: The graph is a *federated view* materialized at query time, not a cache we constantly reconcile. Live system state (ALSA, PipeWire, USB) is queried on-demand and joined with persisted annotations (identity bindings, tags, manual connections).

## Design Principles

1. **Live by default**: Device enumeration, software connections, and system state come from live queries against ALSA/PipeWire/udev—not cached snapshots.

2. **Persist only what we can't query**: Identity bindings ("USB ID X = my JDXi"), user annotations (tags, notes), and manual connections (physical patch cables) go in SQLite.

3. **Trustfall for federation**: Use Trustfall's adapter architecture to join live sources with persisted data in a single GraphQL query.

4. **Organic, not transactional**: The graph is always "now." Changes flow in continuously. We keep a changelog for archaeology but it's append-only telemetry, not rollback state.

5. **Agent-friendly**: Tools designed for LLM agents to discover, connect, trace, and troubleshoot audio systems.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                      MCP TOOLS                               │
│  graph_query, graph_find, graph_connect, graph_bind, etc.   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   TRUSTFALL QUERY ENGINE                     │
│              (GraphQL parsing, IR, interpreter)              │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│  AlsaAdapter  │    │PipeWireAdapter│    │ SqliteAdapter │
│  (live enum)  │    │  (live enum)  │    │ (persisted)   │
└───────────────┘    └───────────────┘    └───────────────┘
        │                     │                     │
        ▼                     ▼                     ▼
   /proc/asound         PipeWire API           SQLite DB
   ALSA sequencer       (pw-dump, etc)         (identities,
   USB enumeration                              tags, notes,
                                                manual conns)
```

## Data Model

### What's Live vs Persisted

| Data | Source | Why |
|------|--------|-----|
| MIDI devices & ports | Live (ALSA) | Always current |
| Audio nodes & links | Live (PipeWire) | Always current |
| USB device tree | Live (udev/sysfs) | Always current |
| Software connections | Live (PipeWire/JACK) | System manages these |
| **Identity bindings** | Persisted | "USB ID X = my JDXi" survives reboots |
| **Tags** | Persisted | User/agent annotations |
| **Notes** | Persisted | History, context |
| **Manual connections** | Persisted | Physical patch cables we can't detect |
| **Changelog** | Persisted | Archaeology, debugging |

### Identity System

Devices have fluid identity—MIDI names change, USB paths shift, devices get shelved. We solve this with **identity hints**: multiple fingerprints that map to a stable logical identity.

```
Identity: "jdxi"
  └─ Hints:
       ├─ usb_device_id: "0582:0160" (confidence: 1.0)
       ├─ midi_name: "JD-Xi" (confidence: 0.9)
       └─ alsa_card: "Roland JD-Xi" (confidence: 0.8)
```

When a live device is discovered, we match its fingerprints against known hints. High-confidence matches auto-bind; low-confidence matches surface for human confirmation.

### Structured Tags

Tags use `namespace:value` format for precise querying:

```yaml
# Agent-managed namespaces
manufacturer: doepfer, roland, arturia, moog, polyend, 1010music, flame
capability: midi-in, midi-out, cv-in, cv-out, gate-in, gate-out, mpe, osc
role: controller, sound-source, processor, gateway, utility, recorder
form-factor: eurorack, desktop, keyboard, rack-mount, pedal

# User namespaces
user: favorite, needs-repair, borrowed
project: ambient-session, live-set-2025
```

## SQLite Schema (Persisted Layer)

Only store what we can't query live:

```sql
-- ============================================
-- IDENTITY BINDINGS
-- "This hardware fingerprint = this logical node"
-- ============================================

CREATE TABLE identities (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    data JSON NOT NULL DEFAULT '{}'  -- {manufacturer, model, kind, ...}
);

-- Multiple hints can map to one identity
CREATE TABLE identity_hints (
    identity_id TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    hint_kind TEXT NOT NULL,  -- 'usb_device_id', 'midi_name', 'alsa_card', ...
    hint_value TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (hint_kind, hint_value)
);

CREATE INDEX idx_hints_identity ON identity_hints(identity_id);

-- ============================================
-- ANNOTATIONS
-- ============================================

CREATE TABLE tags (
    identity_id TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (identity_id, namespace, value)
);

CREATE INDEX idx_tags_ns_val ON tags(namespace, value);

CREATE TABLE notes (
    id TEXT PRIMARY KEY,
    target_kind TEXT NOT NULL,  -- 'identity', 'port', 'connection'
    target_id TEXT NOT NULL,    -- For live entities, use canonical ID
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    source TEXT NOT NULL,       -- 'user', 'agent', 'discovery'
    message TEXT NOT NULL
);

CREATE INDEX idx_notes_target ON notes(target_kind, target_id);

-- ============================================
-- MANUAL CONNECTIONS
-- Physical patches we can't auto-detect
-- ============================================

CREATE TABLE manual_connections (
    id TEXT PRIMARY KEY,
    from_identity TEXT NOT NULL,
    from_port TEXT NOT NULL,      -- Port name pattern
    to_identity TEXT NOT NULL,
    to_port TEXT NOT NULL,
    transport_kind TEXT,          -- 'patch_cable_cv', 'din_midi', 'trs_midi_a', etc.
    signal_direction TEXT,        -- 'forward', 'reverse', 'unknown'
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL,
    UNIQUE (from_identity, from_port, to_identity, to_port)
);

-- ============================================
-- CHANGELOG (append-only telemetry)
-- ============================================

CREATE TABLE changelog (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    source TEXT NOT NULL,         -- 'discovery:alsa', 'agent', 'user'
    operation TEXT NOT NULL,      -- 'identity_create', 'tag_add', 'connection_add'
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    details JSON NOT NULL
);

CREATE INDEX idx_changelog_target ON changelog(target_kind, target_id);
CREATE INDEX idx_changelog_time ON changelog(timestamp DESC);
```

## GraphQL Schema

### Live Data Sources

```graphql
# ============================================
# ALSA MIDI (live from system)
# ============================================

type AlsaMidiDevice {
    card_id: Int!
    device_id: Int!
    name: String!
    subdevice_name: String
    hardware_id: String          # "hw:2,0"
    
    ports: [AlsaMidiPort!]!
    
    # Join to persisted identity
    identity: Identity @join(on: "hint_match")
}

type AlsaMidiPort {
    id: String!                  # "hw:2,0,0"
    name: String!
    direction: PortDirection!
    device: AlsaMidiDevice!
    
    # Notes attached to this port
    notes: [Note!]! @join(on: "target_id")
}

# ============================================
# PIPEWIRE (live from system)
# ============================================

type PipeWireNode {
    id: Int!
    name: String!
    nick: String
    media_class: String!         # "Audio/Sink", "Midi/Bridge", etc.
    
    ports: [PipeWirePort!]!
    links: [PipeWireLink!]!      # Live connections FROM this node
    
    identity: Identity @join(on: "hint_match")
}

type PipeWirePort {
    id: Int!
    name: String!
    direction: PortDirection!
    media_type: String!          # "audio", "midi"
    node: PipeWireNode!
}

type PipeWireLink {
    id: Int!
    output_port: PipeWirePort!
    input_port: PipeWirePort!
    state: String!               # "active", "paused"
}

# ============================================
# USB (live from system)
# ============================================

type UsbDevice {
    bus: Int!
    device: Int!
    vendor_id: String!           # "0582"
    product_id: String!          # "0160"
    manufacturer: String
    product: String
    serial: String
    
    identity: Identity @join(on: "hint_match")
}

# ============================================
# PERSISTED DATA
# ============================================

type Identity {
    id: ID!
    name: String!
    created_at: DateTime!
    data: JSON!
    
    hints: [IdentityHint!]!
    tags: [Tag!]!
    notes: [Note!]!
    
    manual_connections_from: [ManualConnection!]!
    manual_connections_to: [ManualConnection!]!
    
    # Reverse joins to live data
    alsa_devices: [AlsaMidiDevice!]! @join(on: "hint_match")
    pipewire_nodes: [PipeWireNode!]! @join(on: "hint_match")
    usb_devices: [UsbDevice!]! @join(on: "hint_match")
}

type IdentityHint {
    identity: Identity!
    kind: String!
    value: String!
    confidence: Float!
}

type Tag {
    identity: Identity!
    namespace: String!
    value: String!
}

type Note {
    id: ID!
    target_kind: String!
    target_id: String!
    created_at: DateTime!
    source: String!
    message: String!
}

type ManualConnection {
    id: ID!
    from_identity: Identity!
    from_port: String!
    to_identity: Identity!
    to_port: String!
    transport_kind: String
    signal_direction: String
    created_at: DateTime!
    created_by: String!
}

type ChangelogEntry {
    id: Int!
    timestamp: DateTime!
    source: String!
    operation: String!
    target_kind: String!
    target_id: String!
    details: JSON!
}

# ============================================
# ENUMS
# ============================================

enum PortDirection {
    IN
    OUT
    BIDIRECTIONAL
}

# ============================================
# QUERY ENTRYPOINTS
# ============================================

type Query {
    # Live sources
    AlsaMidiDevice: [AlsaMidiDevice!]!
    PipeWireNode(media_class: String): [PipeWireNode!]!
    UsbDevice: [UsbDevice!]!
    
    # Persisted
    Identity(id: ID, name: String): [Identity!]!
    Changelog(since: DateTime, target: String): [ChangelogEntry!]!
    
    # Unbound: live devices with no identity match
    UnboundDevice: [UnboundDevice!]!
}

type UnboundDevice {
    source: String!              # "alsa", "pipewire", "usb"
    raw_name: String!
    hints: [IdentityHintCandidate!]!
    best_match: Identity
    best_match_confidence: Float
}

type IdentityHintCandidate {
    kind: String!
    value: String!
}
```

## MCP Tools

### Core Query Tool

```rust
/// GraphQL query - the primary interface
/// 
/// Execute arbitrary Trustfall queries against the federated graph.
/// Joins live system state with persisted annotations automatically.
#[tool]
async fn graph_query(
    /// GraphQL query string
    query: String,
    /// Query variables as JSON
    variables: Option<JsonValue>,
) -> QueryResult;
```

### Convenience Tools

```rust
/// Find nodes by criteria (wraps common query patterns)
#[tool]
async fn graph_find(
    name: Option<String>,           // Fuzzy match
    tags: Option<Vec<String>>,      // "namespace:value" or just "value"
    protocols: Option<Vec<String>>, // "midi", "audio", "cv"
    kind: Option<String>,           // "device", "software"
) -> Vec<NodeSummary>;

/// Trace signal paths through the graph
#[tool]
async fn graph_trace(
    from: String,                   // Node or port reference
    to: Option<String>,             // None = all reachable
    protocol: Option<String>,       // Filter by signal type
    max_hops: Option<u32>,
) -> Vec<SignalPath>;
```

### Identity Management

```rust
/// Bind a live device to an identity (creates identity if needed)
#[tool]
async fn graph_bind(
    /// Live device ref: "alsa:hw:2,0" or "pipewire:42" or "usb:0582:0160"
    device: String,
    /// Identity ID to bind to, or "new"
    identity: String,
    /// Name if creating new identity
    name: Option<String>,
) -> Identity;

/// Remove an identity and its bindings
#[tool]
async fn graph_unbind(
    identity: String,
) -> UnbindResult;

/// Show unbound devices (discovered but not matched)
#[tool]
async fn graph_unbound() -> Vec<UnboundDevice>;
```

### Annotation Tools

```rust
/// Add/remove tags on an identity
#[tool]
async fn graph_tag(
    identity: String,
    add: Option<Vec<String>>,       // "namespace:value"
    remove: Option<Vec<String>>,
) -> Identity;

/// Add a note to any entity
#[tool]
async fn graph_note(
    /// Target ref: "identity:X" or "port:alsa:hw:2,0:0"
    target: String,
    message: String,
) -> Note;
```

### Manual Connection Tools

```rust
/// Record a manual connection (physical patch cable)
#[tool]
async fn graph_connect(
    from_identity: String,
    from_port: String,
    to_identity: String,
    to_port: String,
    transport: Option<String>,      // "patch_cable_cv", "din_midi", etc.
) -> ManualConnection;

/// Remove a manual connection
#[tool]
async fn graph_disconnect(
    connection_id: Option<String>,
    // Or by endpoints
    from_identity: Option<String>,
    from_port: Option<String>,
    to_identity: Option<String>,
    to_port: Option<String>,
) -> DisconnectResult;
```

### History Tool

```rust
/// Query the changelog
#[tool]
async fn graph_history(
    target: Option<String>,         // Filter by entity
    source: Option<String>,         // Filter by source
    since: Option<String>,          // ISO timestamp
    limit: Option<u32>,
) -> Vec<ChangelogEntry>;
```

## Trustfall Implementation Guide

### Key Concepts

From the Trustfall documentation and examples:

> "Trustfall is a query engine for querying any kind of data source, from APIs and databases to any kind of files on disk."

> "The adapter is a massively smaller and simpler piece of code than Trustfall itself — usually, only a few hundred lines of code. It provides a schema — the dataset's vertex types, properties, and edges."

> "The adapter implements four functions that cover operations over that schema:
> 1. Resolve the 'entrypoint' of a query, getting an initial set of vertices
> 2. Resolve the value of a specific property on a vertex
> 3. Resolve an edge, starting from a vertex and getting its neighbors
> 4. Resolve a vertex subtyping relationship (isinstance check)"

### Adapter Architecture

```rust
use trustfall::{FieldValue, Schema, TrustfallEnumVertex};
use trustfall::provider::{
    BasicAdapter, ContextIterator, ContextOutcomeIterator,
    EdgeParameters, VertexIterator,
};

/// Unified vertex type across all sources
#[derive(Debug, Clone, TrustfallEnumVertex)]
pub enum Vertex {
    // Live sources
    AlsaMidiDevice(AlsaMidiDeviceData),
    AlsaMidiPort(AlsaMidiPortData),
    PipeWireNode(PipeWireNodeData),
    PipeWirePort(PipeWirePortData),
    PipeWireLink(PipeWireLinkData),
    UsbDevice(UsbDeviceData),
    
    // Persisted
    Identity(IdentityData),
    IdentityHint(IdentityHintData),
    Tag(TagData),
    Note(NoteData),
    ManualConnection(ManualConnectionData),
    ChangelogEntry(ChangelogEntryData),
    
    // Synthetic
    UnboundDevice(UnboundDeviceData),
}

/// Federated adapter that joins live + persisted sources
pub struct AudioGraphAdapter {
    alsa: AlsaSource,
    pipewire: PipeWireSource,
    usb: UsbSource,
    sqlite: SqliteSource,
    cache: Cache,
}

impl BasicAdapter<'static> for AudioGraphAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexIterator<'static, Self::Vertex> {
        match edge_name {
            // Live sources
            "AlsaMidiDevice" => self.alsa.enumerate_devices(),
            "PipeWireNode" => self.pipewire.enumerate_nodes(parameters),
            "UsbDevice" => self.usb.enumerate_devices(),
            
            // Persisted
            "Identity" => self.sqlite.query_identities(parameters),
            "Changelog" => self.sqlite.query_changelog(parameters),
            
            // Synthetic
            "UnboundDevice" => self.compute_unbound_devices(),
            
            _ => unreachable!("Unknown starting edge: {edge_name}"),
        }
    }

    fn resolve_property(
        &self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        match (type_name, property_name) {
            ("AlsaMidiDevice", "name") => /* ... */,
            ("AlsaMidiDevice", "card_id") => /* ... */,
            ("Identity", "name") => /* ... */,
            // etc.
        }
    }

    fn resolve_neighbors(
        &self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        match (type_name, edge_name) {
            // Live → Persisted joins (the magic!)
            ("AlsaMidiDevice", "identity") => {
                self.resolve_identity_join(contexts)
            }
            
            // Persisted → Live joins
            ("Identity", "alsa_devices") => {
                self.resolve_alsa_devices_for_identity(contexts)
            }
            ("Identity", "pipewire_nodes") => {
                self.resolve_pipewire_nodes_for_identity(contexts)
            }
            
            // Live → Live
            ("PipeWireNode", "links") => {
                self.pipewire.resolve_links(contexts)
            }
            ("PipeWireNode", "ports") => {
                self.pipewire.resolve_ports(contexts)
            }
            
            // Persisted → Persisted
            ("Identity", "tags") => {
                self.sqlite.resolve_tags(contexts)
            }
            ("Identity", "hints") => {
                self.sqlite.resolve_hints(contexts)
            }
            
            _ => unreachable!("Unknown edge: {type_name}.{edge_name}"),
        }
    }

    fn resolve_coercion(
        &self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        // Handle type coercions if needed
        // e.g., Node → AlsaMidiDevice
    }
}
```

### Identity Join Implementation

The key join logic—matching live devices to persisted identities:

```rust
impl AudioGraphAdapter {
    /// Match a live device to its persisted identity via hints
    fn resolve_identity_join(
        &self,
        contexts: ContextIterator<'static, Vertex>,
    ) -> ContextOutcomeIterator<'static, Vertex, VertexIterator<'static, Vertex>> {
        Box::new(contexts.map(|ctx| {
            let vertex = ctx.active_vertex();
            let hints = self.extract_hints_from_live_device(vertex);
            
            // Query SQLite for matching identity
            let identity = self.sqlite.find_identity_by_hints(&hints);
            
            match identity {
                Some(id) => (ctx, Box::new(std::iter::once(Vertex::Identity(id)))),
                None => (ctx, Box::new(std::iter::empty())),
            }
        }))
    }
    
    /// Extract fingerprints from a live device
    fn extract_hints_from_live_device(&self, vertex: &Vertex) -> Vec<(String, String)> {
        match vertex {
            Vertex::AlsaMidiDevice(d) => vec![
                ("alsa_card".into(), d.name.clone()),
                ("alsa_hw".into(), d.hardware_id.clone()),
            ],
            Vertex::UsbDevice(d) => vec![
                ("usb_device_id".into(), format!("{}:{}", d.vendor_id, d.product_id)),
                ("usb_serial".into(), d.serial.clone().unwrap_or_default()),
            ],
            Vertex::PipeWireNode(n) => vec![
                ("pipewire_name".into(), n.name.clone()),
            ],
            _ => vec![],
        }
    }
}
```

### Live Source Implementations

#### ALSA Source

```rust
pub struct AlsaSource;

impl AlsaSource {
    /// Enumerate MIDI devices via ALSA sequencer
    pub fn enumerate_devices(&self) -> VertexIterator<'static, Vertex> {
        // Option 1: Parse /proc/asound/
        // Option 2: Use alsa-rs crate
        // Option 3: Shell out to `arecord -l` / `aplaymidi -l`
        
        // Example using alsa-rs:
        let seq = alsa::Seq::open(None, None, false).unwrap();
        let clients = seq.client_info_iter();
        
        let devices: Vec<Vertex> = clients
            .filter_map(|client| {
                // Filter to hardware clients, extract device info
                // ...
            })
            .collect();
        
        Box::new(devices.into_iter())
    }
}
```

#### PipeWire Source

```rust
pub struct PipeWireSource;

impl PipeWireSource {
    /// Enumerate nodes via pw-dump or pipewire-rs
    pub fn enumerate_nodes(&self, params: &EdgeParameters) -> VertexIterator<'static, Vertex> {
        // Option 1: Parse `pw-dump` JSON output
        // Option 2: Use pipewire-rs crate (more complex)
        
        let output = std::process::Command::new("pw-dump")
            .output()
            .expect("pw-dump failed");
        
        let dump: Vec<PipeWireObject> = serde_json::from_slice(&output.stdout).unwrap();
        
        let nodes: Vec<Vertex> = dump
            .into_iter()
            .filter(|obj| obj.type_ == "PipeWire:Interface:Node")
            .filter(|obj| {
                // Apply media_class filter if provided
                if let Some(class) = params.get("media_class") {
                    obj.info.props.get("media.class") == Some(class)
                } else {
                    true
                }
            })
            .map(|obj| Vertex::PipeWireNode(obj.into()))
            .collect();
        
        Box::new(nodes.into_iter())
    }
}
```

### Caching Strategy

```rust
pub struct Cache {
    /// Live data: short TTL
    alsa_devices: TimedCache<Vec<AlsaMidiDeviceData>>,   // 5 sec TTL
    pipewire_nodes: TimedCache<Vec<PipeWireNodeData>>,   // 5 sec TTL
    
    /// Identity matches: invalidate on bind/unbind
    identity_matches: LruCache<HintSet, Option<String>>,
}

impl Cache {
    pub fn invalidate_live(&mut self) {
        self.alsa_devices.clear();
        self.pipewire_nodes.clear();
    }
    
    pub fn invalidate_identities(&mut self) {
        self.identity_matches.clear();
    }
}
```

## Example Queries

### "What MIDI devices are connected?"

```graphql
query ConnectedMidi {
    AlsaMidiDevice {
        name @output
        card_id @output
        
        ports {
            name @output
            direction @output
        }
        
        identity {
            name @output
            tags {
                namespace @output
                value @output
            }
        }
    }
}
```

### "What's connected to my JDXi?"

```graphql
query JdxiConnections {
    Identity {
        name @filter(op: "=", value: ["JDXi"])
        
        # Live PipeWire connections
        pipewire_nodes {
            links {
                input_port {
                    name @output
                    node {
                        name @output
                        identity { name @output }
                    }
                }
            }
        }
        
        # Manual connections (patch cables)
        manual_connections_from {
            to_identity { name @output }
            to_port @output
            transport_kind @output
        }
    }
}
```

### "Show online Doepfer modules"

```graphql
query OnlineDoepfer {
    Identity {
        tags @filter(op: "contains", value: [{namespace: "manufacturer", value: "doepfer"}])
        name @output
        
        alsa_devices {
            name @output
            ports {
                name @output
                direction @output
            }
        }
    }
}
```

### "Trace feedback loop" (troubleshooting)

```graphql
query TraceFromBitbox {
    PipeWireNode {
        name @filter(op: "regex", value: ["(?i)bitbox"])
        
        ports {
            direction @filter(op: "=", value: ["OUT"])
            name @output
            
            # Follow links forward
            outgoing_links @recurse(depth: 10) {
                input_port {
                    node {
                        name @output
                        # Detect if we loop back
                        identity { name @output }
                    }
                }
            }
        }
    }
}
```

### "What changed recently?"

```graphql
query RecentChanges {
    Changelog {
        timestamp @filter(op: ">", value: ["$since"]) @output
        source @output
        operation @output
        target_kind @output
        target_id @output
        details @output
    }
}
```

## Agent Workflow Examples

### Scenario: "Let's build a new synth patch"

```
User: "The eurorack is clear and all devices are powered on. 
       Let's build something using the Doepfer modules."

Agent: [graph_query: AlsaMidiDevice with identity joins]
       → Sees: Poly 2, Bitbox, 4VOX online
       → Also sees unbound device hints for Doepfer modules

Agent: [graph_find(tags: ["manufacturer:doepfer"])]
       → Returns: A-110 VCO, A-120 VCF, A-132 VCA, A-140 ADSR

Agent: "I see your Doepfer modules. Here's a classic subtractive patch:
        Poly 2 → A-110 → A-120 → A-132 → Bitbox
        
        Want me to record these connections?"

User: "yes"

Agent: [graph_connect for each patch cable]
       → Manual connections persisted
       
Agent: "Recorded. Ready to test?"
```

### Scenario: "Troubleshooting feedback"

```
User: "There's feedback on Bitbox input 7"

Agent: [graph_query: trace backwards from bitbox input 7]
       → Finds: VCF → VCA → mixer → FX send → ... → back to VCF

Agent: "Found a loop through your mixer's FX send. 
        The signal path is: ...
        Check the mixer aux routing."
```

### Scenario: "Device firmware changed"

```
User: "Replaced the FooBar firmware, it shows up differently now"

Agent: [graph_find(name: "foobar")]
       → Gets current identity with old hints

Agent: [graph_unbound()]
       → Sees new device with different USB ID / MIDI name

Agent: "I see the old FooBar identity and a new unbound device.
        Should I update the binding?"

User: "yes"

Agent: [graph_unbind(identity: "foobar_v1")]
Agent: [graph_bind(device: "usb:1234:9999", identity: "new", name: "FooBar")]
       → New identity with updated hints
```

## Module Structure

```
audio-graph-mcp/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── main.rs                    # MCP server entry point
│   │
│   ├── schema.graphql             # Trustfall schema
│   │
│   ├── adapter/
│   │   ├── mod.rs                 # FederatedAdapter
│   │   ├── vertex.rs              # Vertex enum
│   │   ├── properties.rs          # Property resolution
│   │   ├── edges.rs               # Edge resolution
│   │   └── joins.rs               # Cross-source join logic
│   │
│   ├── sources/
│   │   ├── mod.rs
│   │   ├── alsa.rs                # ALSA MIDI enumeration
│   │   ├── pipewire.rs            # PipeWire enumeration
│   │   ├── usb.rs                 # USB device enumeration
│   │   └── sqlite.rs              # Persisted data access
│   │
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── query.rs               # graph_query
│   │   ├── find.rs                # graph_find, graph_trace
│   │   ├── identity.rs            # graph_bind, graph_unbind
│   │   ├── annotate.rs            # graph_tag, graph_note
│   │   ├── connect.rs             # graph_connect, graph_disconnect
│   │   └── history.rs             # graph_history
│   │
│   ├── cache.rs                   # Caching layer
│   └── changelog.rs               # Changelog recording
│
├── migrations/
│   └── 001_initial.sql
│
└── tests/
    ├── adapter_tests.rs
    └── integration_tests.rs
```

## Dependencies

```toml
[dependencies]
# MCP
mcp-server = "..."  # Your MCP framework

# Trustfall
trustfall = "0.8"
trustfall_derive = "0.2"

# Database
rusqlite = { version = "0.31", features = ["bundled"] }

# Audio/MIDI
alsa = "0.8"  # ALSA bindings

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Async
tokio = { version = "1", features = ["full"] }

# Caching
lru = "0.12"

# Utils
thiserror = "1"
tracing = "0.1"
```

## Implementation Order

**Graph MVP** (prove Trustfall on SQLite first):
1. SQLite schema + CRUD (Task 01)
2. Trustfall adapter - SQLite only (Task 04, simplified)
3. Test fixtures for graph queries (Task 08)
4. MCP tools - basic query interface (Task 05)

**Live Sources** (add incrementally):
5. ALSA enumeration (Task 02)
6. Identity matching / joins (Task 03)
7. PipeWire source (Task 06)
8. Manual connections (Task 07)

**Capstone**:
9. Hootenanny integration (Task 09)

**OTEL**: Add instrumentation incrementally as we build each component.

## References

- Trustfall GitHub: https://github.com/obi1kenobi/trustfall
- Trustfall "How to Query Everything" talk: https://predr.ag/blog/how-to-query-almost-everything-hytradboi/
- cargo-semver-checks (real-world Trustfall usage): https://github.com/obi1kenobi/cargo-semver-checks
- ALSA sequencer docs: https://www.alsa-project.org/alsa-doc/alsa-lib/seq.html
- PipeWire docs: https://docs.pipewire.org/

## Hardware Context

Example devices this system will track:

- **Polyend Poly 2**: 8-voice MIDI-to-CV converter, USB + DIN MIDI input, 20 CV/Gate outputs. Gateway from MIDI world to Eurorack.

- **1010music Bitbox mk2**: Eurorack sampler with MIDI + CV control. Can record and playback, serves as audio I/O bridge.

- **Flame 4VOX**: Quad wavetable oscillator with MIDI input. Can be played via CV or directly via MIDI. 16 voices across 4 VCO channels.

- **Roland JD-Xi**: Desktop synth with USB MIDI, contains multiple synth engines that present as separate MIDI channels.

- **Arturia Keystep Pro**: MIDI controller + sequencer, can drive both MIDI and CV outputs.

These represent different gateway types into the modular world, each with unique identity characteristics and port configurations.
