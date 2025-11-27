# Task 09: Hootenanny Ensemble Integration

**Status**: ✅ Complete
**Estimated effort**: 3-4 hours
**Prerequisites**: All previous tasks
**Depends on**: MCP tools (Task 05), audio graph query engine
**Enables**: Context-aware music generation, intelligent routing

## 🎯 Goal

Integrate audio-graph-mcp with the Hootenanny ensemble system. Enable agents to:
1. **Discover available devices** before generating music
2. **Route MIDI intelligently** based on device capabilities
3. **Understand signal flow** for troubleshooting and optimization
4. **Document patches** as part of creative sessions

## 📋 Context

### The Hootenanny Ensemble Vision

HalfRemembered is a **musical ensemble** where:
- **Agents** collaborate to create music (conversation tree)
- **Orpheus** generates MIDI (AI music model)
- **Audio Graph** provides awareness of the performance space

Before audio-graph-mcp, agents were **blind** to the hardware:
- "Generate a melody" → Where does it play? Unknown.
- "Route to the synth" → Which synth? Can't tell.

After integration:
- "Generate a melody for the JD-Xi" → Agent queries graph, confirms JD-Xi is online
- "Use Eurorack VCO" → Agent sees Doepfer A-110 is available, routes CV via Poly 2

### Integration Points

1. **Discovery Phase** (session start)
   - Agent queries: `graph_find(tags: ["role:sound-source"])` → sees available instruments
   - Agent notes online devices in conversation context

2. **Routing Phase** (music generation)
   - Orpheus generates MIDI → Agent decides: JD-Xi or Flame 4VOX?
   - Query capabilities: `graph_query { Identity(id: "jdxi") { tags { value } } }`

3. **Signal Flow Phase** (troubleshooting)
   - "No audio from Bitbox" → Agent traces: MIDI → Poly 2 → (manual connection?) → A-110 → Bitbox
   - Finds missing patch cable

4. **Documentation Phase** (archival)
   - Session ends → Agent records patch state to changelog
   - Future sessions can restore or reference the setup

## 🏗️ Integration Architecture

```
┌─────────────────────────────────────────────────┐
│          Hootenanny MCP Server                  │
├─────────────────────────────────────────────────┤
│  Ensemble Tools:                                │
│    - play (musical note)                        │
│    - orpheus_generate (MIDI generation)         │
│    - midi_to_wav (rendering)                    │
│                                                  │
│  Audio Graph Tools (NEW):                       │
│    - graph_query                                │
│    - graph_find                                 │
│    - graph_bind                                 │
│    - graph_connect                              │
└─────────────────────────────────────────────────┘
            │
            ▼
┌─────────────────────────────────────────────────┐
│      Audio Graph MCP (separate crate)           │
│  - ALSA enumeration                             │
│  - Identity matching                            │
│  - Trustfall query engine                       │
│  - SQLite persistence                           │
└─────────────────────────────────────────────────┘
```

## 🔨 Implementation (crates/hootenanny/src/mcp_tools/audio_graph.rs)

```rust
//! Audio graph tools integrated into Hootenanny

use audio_graph_mcp::{
    adapter::AudioGraphAdapter,
    mcp_tools,
};
use std::sync::Arc;

/// Re-export audio graph tools with Hootenanny context
pub async fn graph_query_with_context(
    adapter: Arc<AudioGraphAdapter>,
    query: String,
    variables: Option<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    // Delegate to audio-graph-mcp
    mcp_tools::query::graph_query(adapter, query, variables).await
}

/// Convenience: Find devices suitable for music generation
pub async fn find_instruments(
    adapter: Arc<AudioGraphAdapter>,
) -> Result<Vec<serde_json::Value>, String> {
    let query = r#"
        query {
            Identity {
                tags @filter(op: "contains", value: [
                    {namespace: "role", value: "sound-source"}
                ])
                name @output
                tags {
                    namespace @output
                    value @output
                }
                alsa_devices {
                    name @output
                    ports {
                        direction @output
                    }
                }
            }
        }
    "#.to_string();

    graph_query_with_context(adapter, query, None).await
}

/// Convenience: Find MIDI controllers
pub async fn find_controllers(
    adapter: Arc<AudioGraphAdapter>,
) -> Result<Vec<serde_json::Value>, String> {
    let query = r#"
        query {
            Identity {
                tags @filter(op: "contains", value: [
                    {namespace: "role", value: "controller"}
                ])
                name @output
                alsa_devices { name @output }
            }
        }
    "#.to_string();

    graph_query_with_context(adapter, query, None).await
}
```

## 🔨 MCP Server Registration (crates/hootenanny/src/server.rs)

```rust
use audio_graph_mcp::{adapter::AudioGraphAdapter, db::Database as AudioGraphDB};
use std::sync::Arc;

pub struct HootenanyServer {
    // ... existing fields ...
    audio_graph_adapter: Arc<AudioGraphAdapter>,
}

impl HootenanyServer {
    pub async fn new() -> Result<Self> {
        // ... existing initialization ...

        // Initialize audio graph
        let audio_graph_db = Arc::new(AudioGraphDB::open("audio_graph.db")?);
        let audio_graph_adapter = Arc::new(AudioGraphAdapter::new(audio_graph_db)?);

        Ok(Self {
            // ... existing fields ...
            audio_graph_adapter,
        })
    }

    pub async fn register_tools(&self, tool_registry: &mut ToolRegistry) {
        // ... existing tool registration ...

        // Register audio graph tools
        tool_registry.register("graph_query", |args| {
            self.audio_graph_adapter.clone().graph_query(args).boxed()
        });

        tool_registry.register("graph_find", |args| {
            self.audio_graph_adapter.clone().graph_find(args).boxed()
        });

        tool_registry.register("graph_bind", |args| {
            self.audio_graph_adapter.clone().graph_bind(args).boxed()
        });

        // Convenience wrappers
        tool_registry.register("find_instruments", |_| {
            crate::mcp_tools::audio_graph::find_instruments(
                self.audio_graph_adapter.clone()
            ).boxed()
        });

        tool_registry.register("find_controllers", |_| {
            crate::mcp_tools::audio_graph::find_controllers(
                self.audio_graph_adapter.clone()
            ).boxed()
        });
    }
}
```

## 🎼 Agent Workflow Examples

### Scenario 1: Session Start - Device Discovery

```
Agent: Starting new ensemble session. Let me see what instruments are available.

[Uses find_instruments tool]

Result: [
  { name: "Roland JD-Xi", tags: ["manufacturer:roland", "capability:midi-in"] },
  { name: "Flame 4VOX", tags: ["manufacturer:flame", "capability:midi-in"] },
]

Agent: I see we have a JD-Xi and Flame 4VOX online. Both are MIDI-capable.
       Let's start with the JD-Xi for the bass line.

[Uses orpheus_generate to create MIDI]
[Routes to JD-Xi via ALSA port]
```

### Scenario 2: Eurorack Patch Setup

```
User: I've patched Poly 2 CV out 1 to the Doepfer VCO.

Agent: Let me record that connection.

[Uses graph_connect]
  from_identity: "poly2"
  from_port: "cv_out_1"
  to_identity: "doepfer_a110"
  to_port: "voct_in"
  transport: "patch_cable_cv"

Agent: Recorded. Now when I generate MIDI for Poly 2, I know it'll drive the VCO.

[Uses orpheus_generate with MIDI for channel 1]
[Sends to Poly 2 ALSA port]
[Agent knows: MIDI → Poly 2 → CV → Doepfer A-110 → audio]
```

### Scenario 3: Troubleshooting

```
User: I'm not hearing audio from the Bitbox.

Agent: Let me trace the signal path to Bitbox.

[Uses graph_query to find connections to Bitbox]

Result: No connections found to "bitbox" identity.

Agent: I don't see any recorded connections to Bitbox. Did you patch something to it?

User: Yes, from the Doepfer VCO audio out.

Agent: Let me record that.

[Uses graph_connect]
  from_identity: "doepfer_a110"
  from_port: "audio_out"
  to_identity: "bitbox"
  to_port: "input_1"

Agent: Recorded. Now the full path is:
       MIDI → Poly 2 → CV → Doepfer A-110 → Audio → Bitbox
       Check your Bitbox input levels and monitoring settings.
```

## 🧪 Integration Tests (tests/ensemble_integration.rs)

```rust
#[tokio::test]
async fn test_agent_discovers_instruments() {
    let server = HootenanyServer::new().await.unwrap();

    // Simulate agent calling find_instruments
    let instruments = find_instruments(server.audio_graph_adapter.clone())
        .await
        .unwrap();

    assert!(!instruments.is_empty(), "Should find at least one instrument");

    println!("Discovered instruments:");
    for inst in &instruments {
        println!("  - {}", inst["name"]);
    }
}

#[tokio::test]
async fn test_orpheus_with_device_context() {
    let server = HootenanyServer::new().await.unwrap();

    // Find available sound sources
    let instruments = find_instruments(server.audio_graph_adapter.clone())
        .await
        .unwrap();

    let target_device = &instruments[0]["name"];
    println!("Generating MIDI for: {}", target_device);

    // Generate MIDI (existing Orpheus functionality)
    let job_id = server.orpheus_generate(Default::default()).await.unwrap();

    // Wait for generation
    let result = server.wait_for_job(job_id).await.unwrap();

    println!("✓ Generated MIDI with device awareness: {:?}", result);
}
```

## ✅ Acceptance Criteria

1. ✅ Hootenanny server initializes audio-graph-mcp adapter
2. ✅ Audio graph tools registered and callable via MCP
3. ✅ `find_instruments` returns available sound sources
4. ✅ Agents can query device capabilities before generating music
5. ✅ Manual connections recorded during sessions
6. ✅ Integration tests demonstrate workflow

## 📚 Documentation Updates

Update `crates/hootenanny/README.md`:

```markdown
## Audio Graph Integration

Hootenanny now includes device awareness through the audio-graph-mcp system:

### Available Tools

- `find_instruments` - Discover available sound sources
- `find_controllers` - Discover MIDI controllers
- `graph_query` - Execute arbitrary GraphQL queries against the audio graph
- `graph_bind` - Bind live devices to identities
- `graph_connect` - Record manual patch cable connections

### Example: Context-Aware Generation

```bash
# Agent workflow
1. find_instruments → discovers JD-Xi, Flame 4VOX
2. orpheus_generate → creates MIDI
3. Route to specific device based on capabilities
```

See [audio-graph-mcp README](../audio-graph-mcp/README.md) for details.
```

## 🎨 Future Enhancements

- **Automatic MIDI routing**: Agent automatically selects best device for generated content
- **Patch templates**: Save/load common Eurorack patches
- **Visual patch diagrams**: Generate mermaid/graphviz from graph state
- **Telemetry integration**: Emit device events to OTLP for observability

## 🎬 Result

After this task, HalfRemembered agents have **full awareness** of the musical environment:
- They know what instruments are connected
- They can route music intelligently
- They understand signal flow end-to-end
- They document patches for future sessions

**The ensemble becomes truly collaborative** 🎼🤖
