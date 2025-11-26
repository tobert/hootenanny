# Task 04: Trustfall GraphQL Adapter

**Status**: ✅ Complete (SQLite-only, 4 tests passing)
**Estimated effort**: 6-8 hours (learning curve + implementation)
**Prerequisites**: Task 01 (SQLite), Task 02 (ALSA), Task 03 (Identity matching)
**Depends on**: All data sources, matcher
**Enables**: Task 05 (MCP tools with rich queries)

## 🎯 Goal

Implement a **Trustfall adapter** that federates live sources (ALSA, future: PipeWire, USB) with persisted data (SQLite) into a unified GraphQL-queryable graph.

**Why Trustfall?** It's the 限界突破 choice:
- GraphQL-style queries without running a GraphQL server
- Composes heterogeneous data sources
- Compile-time schema validation
- Powerful for agent exploration ("find all devices tagged X with ports of type Y")

## 📋 Context

### What is Trustfall?

From the Trustfall docs:

> "Trustfall is a query engine for querying any kind of data source, from APIs and databases to any kind of files on disk."

It works by:
1. You define a **GraphQL schema** describing your domain (devices, ports, identities, etc.)
2. You implement a **BasicAdapter** with 4 methods:
   - `resolve_starting_vertices`: Entry points ("give me all AlsaMidiDevices")
   - `resolve_property`: Get a property value ("device.name")
   - `resolve_neighbors`: Traverse edges ("device → identity", "identity → tags")
   - `resolve_coercion`: Type checking/casting
3. Trustfall compiles GraphQL queries → Rust code → executes against your adapter

### The Magic: Federated Joins

The key insight: **live data joins with persisted data at query time**.

```graphql
query {
    AlsaMidiDevice {           # Live source (ALSA)
        name @output
        identity {             # JOIN to SQLite
            name @output
            tags {             # Persisted tags
                value @output
            }
        }
    }
}
```

Trustfall handles the join logic. We just implement edge resolution.

## 🗃️ GraphQL Schema (src/schema.graphql)

Start with a **minimal schema** (we'll expand in Task 06 for PipeWire):

```graphql
schema {
    query: Query
}

type Query {
    AlsaMidiDevice: [AlsaMidiDevice!]!
    Identity(id: ID, name: String): [Identity!]!
    UnboundDevice: [UnboundDevice!]!
}

# ============================================
# ALSA (Live)
# ============================================

type AlsaMidiDevice {
    card_id: Int!
    device_id: Int!
    name: String!
    hardware_id: String!

    ports: [AlsaMidiPort!]!

    # JOIN to persisted identity
    identity: Identity
}

type AlsaMidiPort {
    id: String!
    name: String!
    direction: PortDirection!
}

enum PortDirection {
    IN
    OUT
    BIDIRECTIONAL
}

# ============================================
# Identity (Persisted)
# ============================================

type Identity {
    id: ID!
    name: String!
    created_at: String!

    hints: [IdentityHint!]!
    tags: [Tag!]!
    notes: [Note!]!

    # Reverse join: which live devices match this identity?
    alsa_devices: [AlsaMidiDevice!]!
}

type IdentityHint {
    kind: String!
    value: String!
    confidence: Float!
}

type Tag {
    namespace: String!
    value: String!
}

type Note {
    id: ID!
    created_at: String!
    source: String!
    message: String!
}

# ============================================
# Unbound Devices (Synthetic)
# ============================================

type UnboundDevice {
    source: String!           # "alsa"
    raw_name: String!
    fingerprints: [DeviceFingerprint!]!
    best_match: Identity
    best_match_score: Float
}

type DeviceFingerprint {
    kind: String!
    value: String!
}
```

## 🏗️ Module Structure

```
src/
├── schema.graphql
├── adapter/
│   ├── mod.rs               # AudioGraphAdapter
│   ├── vertex.rs            # Vertex enum
│   ├── properties.rs        # Property resolution
│   ├── edges.rs             # Edge/neighbor resolution
│   └── coercion.rs          # Type coercion (optional)
```

## 📦 Dependencies (add to Cargo.toml)

```toml
[dependencies]
trustfall = "0.8"
trustfall_core = "0.8"

# For schema embedding
include_dir = "0.7"
```

## 🎨 Vertex Type (src/adapter/vertex.rs)

```rust
use trustfall::provider::TrustfallEnumVertex;
use crate::types::*;

/// Unified vertex type across all sources
#[derive(Debug, Clone, TrustfallEnumVertex)]
pub enum Vertex {
    // Live sources
    AlsaMidiDevice(Box<AlsaMidiDevice>),
    AlsaMidiPort(Box<AlsaMidiPort>),

    // Persisted
    Identity(Box<Identity>),
    IdentityHint(Box<IdentityHint>),
    Tag(Box<Tag>),
    Note(Box<Note>),

    // Synthetic
    UnboundDevice(Box<UnboundDeviceData>),
    DeviceFingerprint(Box<DeviceFingerprint>),
}

#[derive(Debug, Clone)]
pub struct UnboundDeviceData {
    pub source: String,
    pub raw_name: String,
    pub fingerprints: Vec<DeviceFingerprint>,
    pub best_match: Option<Identity>,
    pub best_match_score: Option<f64>,
}
```

## 🔨 Adapter Implementation (src/adapter/mod.rs)

```rust
use trustfall::provider::{
    BasicAdapter, ContextIterator, ContextOutcomeIterator,
    EdgeParameters, VertexIterator, ResolveEdgeInfo, ResolveInfo,
};
use trustfall::{FieldValue, Schema};
use std::sync::Arc;

use crate::db::Database;
use crate::sources::alsa::AlsaSource;
use crate::matcher::IdentityMatcher;
use crate::adapter::vertex::Vertex;

/// Per-query snapshot of live device state.
/// Frozen at query start to ensure consistency during resolution.
#[derive(Debug, Clone)]
pub struct LiveSnapshot {
    pub alsa_devices: Vec<AlsaMidiDevice>,
    pub pipewire_nodes: Vec<PipeWireNodeData>,  // Added in Task 06
    pub captured_at: std::time::Instant,
}

pub struct AudioGraphAdapter {
    schema: Schema,
    db: Arc<Database>,
    alsa: AlsaSource,
    /// Snapshot of live state - populated at query start, cleared after.
    /// This prevents race conditions if devices disconnect mid-query.
    snapshot: RefCell<Option<LiveSnapshot>>,
}

impl AudioGraphAdapter {
    pub fn new(db: Arc<Database>) -> anyhow::Result<Self> {
        let schema_text = include_str!("../schema.graphql");
        let schema = Schema::parse(schema_text)?;

        Ok(Self {
            schema,
            db,
            alsa: AlsaSource::new(),
            snapshot: RefCell::new(None),
        })
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Capture live state at query start. Call before executing query.
    #[tracing::instrument(name = "adapter.capture_snapshot", skip(self))]
    pub fn capture_snapshot(&self) -> anyhow::Result<()> {
        let alsa_devices = self.alsa.enumerate_devices()?;
        // PipeWire will be added in Task 06
        let pipewire_nodes = vec![];

        *self.snapshot.borrow_mut() = Some(LiveSnapshot {
            alsa_devices,
            pipewire_nodes,
            captured_at: std::time::Instant::now(),
        });
        Ok(())
    }

    /// Clear snapshot after query completes.
    pub fn clear_snapshot(&self) {
        *self.snapshot.borrow_mut() = None;
    }

    /// Get ALSA devices from snapshot (not live!)
    fn get_alsa_devices(&self) -> Vec<AlsaMidiDevice> {
        self.snapshot.borrow()
            .as_ref()
            .map(|s| s.alsa_devices.clone())
            .unwrap_or_default()
    }
}

impl BasicAdapter<'static> for AudioGraphAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
        _resolve_info: &ResolveInfo,
    ) -> VertexIterator<'static, Self::Vertex> {
        match edge_name {
            "AlsaMidiDevice" => {
                // Use snapshot, not live enumeration!
                let devices = self.get_alsa_devices();

                Box::new(devices.into_iter().map(|d| Vertex::AlsaMidiDevice(Box::new(d))))
            }
            "Identity" => {
                // Handle optional filters: id, name
                let id = parameters.get("id").and_then(|v| v.as_str());
                let name = parameters.get("name").and_then(|v| v.as_str());

                let identities = if let Some(id) = id {
                    self.db.get_identity(id).unwrap().into_iter().collect()
                } else if let Some(name) = name {
                    self.db.find_identities_by_name(name).unwrap()
                } else {
                    self.db.list_identities().unwrap()
                };

                Box::new(identities.into_iter().map(|i| Vertex::Identity(Box::new(i))))
            }
            "UnboundDevice" => {
                // Compute unbound devices (live devices with no identity match)
                Box::new(self.compute_unbound_devices())
            }
            _ => unreachable!("Unknown starting edge: {edge_name}"),
        }
    }

    fn resolve_property(
        &self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        property_name: &str,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, FieldValue> {
        match (type_name, property_name) {
            ("AlsaMidiDevice", "name") => {
                Box::new(contexts.map(|ctx| {
                    let value = match ctx.active_vertex() {
                        Vertex::AlsaMidiDevice(d) => FieldValue::String(d.name.clone().into()),
                        _ => unreachable!(),
                    };
                    (ctx, value)
                }))
            }
            ("AlsaMidiDevice", "card_id") => {
                Box::new(contexts.map(|ctx| {
                    let value = match ctx.active_vertex() {
                        Vertex::AlsaMidiDevice(d) => FieldValue::Int64(d.card_id as i64),
                        _ => unreachable!(),
                    };
                    (ctx, value)
                }))
            }
            ("Identity", "name") => {
                Box::new(contexts.map(|ctx| {
                    let value = match ctx.active_vertex() {
                        Vertex::Identity(i) => FieldValue::String(i.name.clone().into()),
                        _ => unreachable!(),
                    };
                    (ctx, value)
                }))
            }
            // Add more property resolvers...
            _ => unreachable!("Unknown property: {type_name}.{property_name}"),
        }
    }

    fn resolve_neighbors(
        &self,
        contexts: ContextIterator<'static, Self::Vertex>,
        type_name: &str,
        edge_name: &str,
        _parameters: &EdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, VertexIterator<'static, Self::Vertex>> {
        match (type_name, edge_name) {
            // Live → Persisted join (THE MAGIC!)
            ("AlsaMidiDevice", "identity") => {
                Box::new(contexts.map(|ctx| {
                    let device = match ctx.active_vertex() {
                        Vertex::AlsaMidiDevice(d) => d,
                        _ => unreachable!(),
                    };

                    // Extract fingerprints and match
                    let fingerprints = self.alsa.extract_fingerprints(device);
                    let matcher = IdentityMatcher::new(&self.db);
                    let best_match = matcher.best_match(&fingerprints).unwrap();

                    let neighbors: VertexIterator<'static, Vertex> = if let Some(m) = best_match {
                        Box::new(std::iter::once(Vertex::Identity(Box::new(m.identity))))
                    } else {
                        Box::new(std::iter::empty())
                    };

                    (ctx, neighbors)
                }))
            }
            // Persisted → Live join (reverse)
            ("Identity", "alsa_devices") => {
                Box::new(contexts.map(|ctx| {
                    let identity = match ctx.active_vertex() {
                        Vertex::Identity(i) => i,
                        _ => unreachable!(),
                    };

                    // Find all ALSA devices that match this identity
                    let all_devices = self.alsa.enumerate_devices().unwrap();
                    let matcher = IdentityMatcher::new(&self.db);

                    let matched: Vec<Vertex> = all_devices
                        .into_iter()
                        .filter_map(|device| {
                            let fingerprints = self.alsa.extract_fingerprints(&device);
                            if let Some(m) = matcher.best_match(&fingerprints).unwrap() {
                                if m.identity.id == identity.id {
                                    return Some(Vertex::AlsaMidiDevice(Box::new(device)));
                                }
                            }
                            None
                        })
                        .collect();

                    (ctx, Box::new(matched.into_iter()) as VertexIterator<_>)
                }))
            }
            // Identity → Tags
            ("Identity", "tags") => {
                Box::new(contexts.map(|ctx| {
                    let identity = match ctx.active_vertex() {
                        Vertex::Identity(i) => i,
                        _ => unreachable!(),
                    };

                    let tags = self.db.get_tags(&identity.id).unwrap();
                    let vertices: Vec<Vertex> = tags.into_iter()
                        .map(|t| Vertex::Tag(Box::new(t)))
                        .collect();

                    (ctx, Box::new(vertices.into_iter()) as VertexIterator<_>)
                }))
            }
            // Add more edge resolvers...
            _ => unreachable!("Unknown edge: {type_name}.{edge_name}"),
        }
    }

    fn resolve_coercion(
        &self,
        _contexts: ContextIterator<'static, Self::Vertex>,
        _type_name: &str,
        _coerce_to_type: &str,
        _resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'static, Self::Vertex, bool> {
        // No coercion needed for now
        unreachable!("No coercion defined")
    }
}

impl AudioGraphAdapter {
    /// Compute unbound devices (live devices with no high-confidence match)
    fn compute_unbound_devices(&self) -> impl Iterator<Item = Vertex> {
        let devices = self.alsa.enumerate_devices().unwrap_or_default();
        let matcher = IdentityMatcher::new(&self.db);

        devices.into_iter().filter_map(move |device| {
            let fingerprints = self.alsa.extract_fingerprints(&device);
            let best_match = matcher.best_match(&fingerprints).ok()?;

            // If no match OR low confidence, it's unbound
            if let Some(m) = &best_match {
                if m.confidence == MatchConfidence::High {
                    return None;  // Bound, skip
                }
            }

            Some(Vertex::UnboundDevice(Box::new(UnboundDeviceData {
                source: "alsa".into(),
                raw_name: device.name.clone(),
                fingerprints,
                best_match: best_match.as_ref().map(|m| m.identity.clone()),
                best_match_score: best_match.map(|m| m.score),
            })))
        })
    }
}
```

## 🧪 Testing (tests/adapter_tests.rs)

```rust
use audio_graph_mcp::adapter::AudioGraphAdapter;
use trustfall::execute_query;

#[test]
fn test_query_alsa_devices() {
    let db = setup_test_db();
    let adapter = Arc::new(AudioGraphAdapter::new(db).unwrap());

    let query = r#"
        query {
            AlsaMidiDevice {
                name @output
                card_id @output
            }
        }
    "#;

    let results = execute_query(adapter.schema(), adapter.clone(), query, HashMap::new())
        .unwrap()
        .collect::<Vec<_>>();

    assert!(!results.is_empty());
}

#[test]
fn test_query_with_identity_join() {
    let db = setup_test_db();

    // Add identity
    db.create_identity("test", "Test Device", json!({})).unwrap();
    db.add_hint("test", HintKind::AlsaCard, "Virtual Raw MIDI", 1.0).unwrap();

    let adapter = Arc::new(AudioGraphAdapter::new(db).unwrap());

    let query = r#"
        query {
            AlsaMidiDevice {
                name @output
                identity {
                    name @output
                }
            }
        }
    "#;

    let results = execute_query(adapter.schema(), adapter.clone(), query, HashMap::new())
        .unwrap()
        .collect::<Vec<_>>();

    // Should see identity for matched device
    assert!(results.iter().any(|r| r.get("identity").is_some()));
}
```

## ✅ Acceptance Criteria

When this task is complete:

1. ✅ Schema compiles and validates
2. ✅ Query `AlsaMidiDevice { name }` returns live ALSA devices
3. ✅ Query `Identity { name }` returns persisted identities
4. ✅ **JOIN works**: `AlsaMidiDevice { identity { name } }` resolves correctly
5. ✅ **Reverse join works**: `Identity { alsa_devices { name } }` finds matching devices
6. ✅ `UnboundDevice` query returns devices without high-confidence matches
7. ✅ All tests pass

## 💡 Implementation Tips

1. **Start minimal**: Get basic queries working before complex joins
2. **Test incrementally**: One query pattern at a time
3. **Use `@output` directives**: Trustfall requires explicit output marking
4. **Study examples**: cargo-semver-checks is excellent Trustfall reference code
5. **Debug with println!**: Trustfall execution can be opaque, log liberally

## 🚧 Out of Scope (for this task)

- ❌ PipeWire integration (Task 06)
- ❌ USB device enumeration
- ❌ Manual connection tracking
- ❌ Advanced filters (@filter directives)

Focus ONLY on ALSA + Identity federation. Expand to other sources in later tasks.

## 📚 References

- Trustfall docs: https://github.com/obi1kenobi/trustfall
- cargo-semver-checks (reference impl): https://github.com/obi1kenobi/cargo-semver-checks
- "How to Query Everything" talk: https://predr.ag/blog/how-to-query-almost-everything-hytradboi/

## 🎬 Next Task

After Trustfall works: **[Task 05: MCP Tool Interface](task-05-mcp-tools.md)**

We'll expose the query engine through MCP tools for agents to use.
