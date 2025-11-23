# Task 06: PipeWire Integration

**Status**: 🟡 Not started
**Estimated effort**: 4-5 hours
**Prerequisites**: Task 04 (Trustfall adapter)
**Depends on**: Adapter infrastructure
**Enables**: Audio routing visibility, connection tracing

## 🎯 Goal

Add PipeWire as a live data source to see audio/MIDI routing through the software layer. Extend the GraphQL schema with `PipeWireNode`, `PipeWirePort`, `PipeWireLink`.

**Why PipeWire?** Modern Linux audio stack. Handles:
- Software audio routing (DAW → speakers)
- MIDI routing (ALSA → software synths)
- Network audio (Jack Audio Connection Kit compatibility)

## 📋 Context

### PipeWire Architecture

```
Hardware (ALSA) → PipeWire Node → PipeWire Ports → PipeWire Links → Other Nodes
```

Example: JD-Xi USB MIDI:
```
ALSA hw:2,0
  → PipeWire Node "JD-Xi MIDI"
    → Port "JD-Xi MIDI 1 Out"
      → Link → Port "Bitwig Studio MIDI In"
        → PipeWire Node "Bitwig Studio"
```

### Data Source: pw-dump

Simplest approach: parse `pw-dump` JSON output.

```bash
$ pw-dump
[
  {
    "id": 42,
    "type": "PipeWire:Interface:Node",
    "info": {
      "props": {
        "node.name": "JD-Xi",
        "media.class": "Midi/Bridge",
        ...
      }
    }
  },
  {
    "id": 128,
    "type": "PipeWire:Interface:Port",
    "info": {
      "props": {
        "port.name": "output",
        "port.direction": "out",
        "format.dsp": "8 bit raw midi"
      }
    }
  },
  {
    "id": 256,
    "type": "PipeWire:Interface:Link",
    "info": {
      "output-port": 128,
      "input-port": 256,
      "state": "active"
    }
  }
]
```

## 🎨 Extend GraphQL Schema

Add to `src/schema.graphql`. Note the link to `AlsaMidiDevice` - this allows us to traverse from the software node back to the hardware kernel device.

```graphql
type Query {
    # ... existing ...
    PipeWireNode(media_class: String): [PipeWireNode!]!
}

type PipeWireNode {
    id: Int!
    host: String!
    name: String!
    media_class: String!

    ports: [PipeWirePort!]!
    links_out: [PipeWireLink!]!  # Links FROM this node

    # Join to identity (logical device)
    identity: Identity

    # Join to underlying hardware (kernel device)
    alsa_device: AlsaMidiDevice
}

type PipeWirePort {
    id: Int!
    name: String!
    direction: PortDirection!
    media_type: String!  # "audio", "midi"
    node: PipeWireNode!
}

type PipeWireLink {
    id: Int!
    output_port: PipeWirePort!
    input_port: PipeWirePort!
    state: String!  # "active", "paused"
}
```

## 🔨 Implementation (src/sources/pipewire.rs)

```rust
use serde::{Deserialize, Serialize};
use std::process::Command;
use anyhow::{Context, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeWireDump(Vec<PipeWireObject>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeWireObject {
    pub id: u32,
    #[serde(rename = "type")]
    pub type_: String,
    pub info: serde_json::Value,
}

pub struct PipeWireSource;

impl PipeWireSource {
    pub fn new() -> Self {
        Self
    }

    /// Enumerate PipeWire nodes via pw-dump
    pub fn enumerate_nodes(&self) -> Result<Vec<PipeWireNodeData>> {
        let output = Command::new("pw-dump")
            .output()
            .context("Failed to run pw-dump (is PipeWire installed?)")?;

        if !output.status.success() {
            anyhow::bail!("pw-dump failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let dump: Vec<PipeWireObject> = serde_json::from_slice(&output.stdout)
            .context("Failed to parse pw-dump output")?;

        let nodes = dump
            .into_iter()
            .filter(|obj| obj.type_ == "PipeWire:Interface:Node")
            .filter_map(|obj| self.parse_node(obj).ok())
            .collect();

        Ok(nodes)
    }

    fn parse_node(&self, obj: PipeWireObject) -> Result<PipeWireNodeData> {
        let props = &obj.info["props"];

        Ok(PipeWireNodeData {
            id: obj.id as i32,
            name: props["node.name"].as_str().unwrap_or("Unknown").to_string(),
            media_class: props["media.class"].as_str().unwrap_or("").to_string(),
            // Capture ALSA properties if available for linking
            alsa_card: props.get("alsa.card").and_then(|v| v.as_i64()).map(|v| v as i32),
            alsa_device: props.get("alsa.device").and_then(|v| v.as_i64()).map(|v| v as i32),
            alsa_subdevice: props.get("alsa.subdevice").and_then(|v| v.as_i64()).map(|v| v as i32),
        })
    }

    pub fn extract_fingerprints(&self, node: &PipeWireNodeData) -> Vec<DeviceFingerprint> {
        vec![
            DeviceFingerprint {
                kind: HintKind::PipewireName,
                value: node.name.clone(),
            },
        ]
    }
}

#[derive(Debug, Clone)]
pub struct PipeWireNodeData {
    pub id: i32,
    pub name: String,
    pub media_class: String,
    // Foreign keys to ALSA
    pub alsa_card: Option<i32>,
    pub alsa_device: Option<i32>,
    pub alsa_subdevice: Option<i32>,
}
```

## 🔨 Extend Vertex Enum (src/adapter/vertex.rs)

```rust
#[derive(Debug, Clone, TrustfallEnumVertex)]
pub enum Vertex {
    // ... existing ...
    PipeWireNode(Box<PipeWireNodeData>),
    PipeWirePort(Box<PipeWirePortData>),
    PipeWireLink(Box<PipeWireLinkData>),
}
```

## 🔨 Extend Adapter (src/adapter/mod.rs)

Add to `AudioGraphAdapter`:

```rust
pub struct AudioGraphAdapter {
    // ... existing ...
    pipewire: PipeWireSource,
}

impl BasicAdapter {
    fn resolve_starting_vertices(...) {
        match edge_name {
            // ... existing ...
            "PipeWireNode" => {
                let media_class = parameters.get("media_class").and_then(|v| v.as_str());
                let nodes = self.pipewire.enumerate_nodes().unwrap();

                let filtered = if let Some(class) = media_class {
                    nodes.into_iter().filter(|n| n.media_class == class).collect()
                } else {
                    nodes
                };

                Box::new(filtered.into_iter().map(|n| Vertex::PipeWireNode(Box::new(n))))
            }
            // ...
        }
    }

    fn resolve_neighbors(...) {
        match (type_name, edge_name) {
            // ... existing ...
            ("PipeWireNode", "identity") => {
                // Same pattern as ALSA: extract fingerprints, match
                Box::new(contexts.map(|ctx| {
                    let node = match ctx.active_vertex() {
                        Vertex::PipeWireNode(n) => n,
                        _ => unreachable!(),
                    };

                    let fingerprints = self.pipewire.extract_fingerprints(node);
                    let matcher = IdentityMatcher::new(&self.db);
                    let best = matcher.best_match(&fingerprints).unwrap();

                    let neighbors = if let Some(m) = best {
                        Box::new(std::iter::once(Vertex::Identity(Box::new(m.identity))))
                    } else {
                        Box::new(std::iter::empty())
                    };

                    (ctx, neighbors)
                }))
            }
            // LINK TO ALSA HARDWARE
            ("PipeWireNode", "alsa_device") => {
                Box::new(contexts.map(|ctx| {
                    let node = match ctx.active_vertex() {
                        Vertex::PipeWireNode(n) => n,
                        _ => unreachable!(),
                    };

                    // If this PW node maps to specific ALSA hardware...
                    let neighbors = if let (Some(card), Some(dev)) = (node.alsa_card, node.alsa_device) {
                        // Find the ALSA device with matching card/device ID
                        let devices = self.alsa.enumerate_devices().unwrap_or_default();
                        let found = devices.into_iter().find(|d| d.card_id == card && d.device_id == dev);
                        
                        if let Some(alsa_dev) = found {
                            Box::new(std::iter::once(Vertex::AlsaMidiDevice(Box::new(alsa_dev))))
                        } else {
                            Box::new(std::iter::empty())
                        }
                    } else {
                        Box::new(std::iter::empty())
                    };

                    (ctx, neighbors)
                }))
            }
            // ...
        }
    }
}
```

## 🧪 Testing

```rust
#[test]
fn test_pipewire_enumeration() {
    let pw = PipeWireSource::new();
    let nodes = pw.enumerate_nodes().expect("Failed to enumerate PipeWire nodes");

    println!("Found {} PipeWire nodes", nodes.len());
    for node in &nodes {
        println!("  {} ({})", node.name, node.media_class);
    }

    // Should find at least some nodes on a PipeWire system
    assert!(!nodes.is_empty());
}

#[tokio::test]
async fn test_query_pipewire_nodes() {
    let db = setup_test_db();
    let adapter = Arc::new(AudioGraphAdapter::new(db).unwrap());

    let query = r#"
        {
            PipeWireNode {
                name @output
                media_class @output
            }
        }
    "#;

    let results = execute_query(adapter.schema(), adapter.clone(), query, HashMap::new())
        .unwrap()
        .collect::<Vec<_>>();

    assert!(!results.is_empty());
}
```

## ✅ Acceptance Criteria

1. ✅ `PipeWireSource::enumerate_nodes()` parses pw-dump
2. ✅ Query `PipeWireNode { name }` returns nodes
3. ✅ Filter by `media_class`: `PipeWireNode(media_class: "Midi/Bridge")`
4. ✅ Join to identity: `PipeWireNode { identity { name } }`
5. ✅ Handles missing pw-dump gracefully (clear error)

## 🚧 Out of Scope

- ❌ PipeWire ports/links (add later if needed)
- ❌ Native `pipewire-rs` bindings (pw-dump is simpler for MVP)
- ❌ Real-time link monitoring

## 🎬 Next Task

**[Task 07: Manual Connection Tracking](task-07-manual-connections.md)** - Record patch cables
