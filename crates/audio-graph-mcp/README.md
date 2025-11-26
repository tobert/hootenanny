# 🎛️ Audio Graph MCP

**An MCP server that gives AI agents a queryable view of audio/MIDI infrastructure.**

Think of it as an "agentic DAW" where Claude acts as producer, with visibility into the full signal flow across hardware synths, Eurorack modules, software plugins, and network-connected compute.

## 🎯 Vision

In the HalfRemembered ensemble, agents need to understand the musical landscape:
- **What devices are available?** (JD-Xi, Poly 2, Eurorack modules)
- **How are they connected?** (MIDI routing, patch cables, audio sends)
- **What can they do?** (Tags: `capability:mpe`, `role:sound-source`)
- **How do I address them?** (Stable identities despite shifting USB paths)

This server provides that knowledge through a federated graph that joins:
- 🔴 **Live system state**: ALSA MIDI, PipeWire audio, USB device tree
- 💾 **Persisted annotations**: Identity bindings, tags, notes, manual connections

## 🏗️ Architecture

### Design Principles

1. **Live by default**: Device enumeration comes from live queries (ALSA/PipeWire/udev), not cached snapshots
2. **Persist only what we can't query**: Identity bindings, user annotations, patch cables
3. **Trustfall for federation**: GraphQL-style queries that join live + persisted data
4. **Organic, not transactional**: The graph is always "now"—changes flow continuously
5. **Agent-friendly**: Tools designed for LLM discovery and troubleshooting

### The Identity Problem

Hardware has **fluid identity**:
- USB paths change between reboots (`/dev/snd/midiC2D0` → `midiC3D0`)
- MIDI names vary ("JD-Xi", "Roland JD-Xi MIDI 1")
- Devices get shelved, firmware updates change fingerprints

**Solution:** Multi-hint identity matching
```
Identity: "jdxi"
  └─ Hints:
       ├─ usb_device_id: "0582:0160" (confidence: 1.0)
       ├─ midi_name: "JD-Xi" (confidence: 0.9)
       └─ alsa_card: "Roland JD-Xi" (confidence: 0.8)
```

When a device appears, we match its fingerprints against known hints. High-confidence matches auto-bind; low-confidence surface for human confirmation.

### Data Flow

```
┌─────────────────────────────────────────────────────┐
│  MCP TOOLS (graph_query, graph_find, graph_bind)   │
└─────────────────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────┐
│        TRUSTFALL QUERY ENGINE (GraphQL)             │
└─────────────────────────────────────────────────────┘
          │                │                 │
          ▼                ▼                 ▼
    ┌─────────┐      ┌─────────┐      ┌─────────┐
    │  ALSA   │      │PipeWire │      │ SQLite  │
    │ (live)  │      │ (live)  │      │(persist)│
    └─────────┘      └─────────┘      └─────────┘
```

## 🚀 Quick Start (Future State)

```bash
# Install virtual MIDI devices for testing
sudo modprobe snd-virmidi midi_devs=4

# Run the MCP server
cargo run --bin audio-graph-mcp

# In Claude Code, query devices
> Use graph_query to find all MIDI devices

# Bind a device
> Use graph_bind to bind hw:2,0 to identity "test-synth"

# Tag it
> Use graph_tag to tag test-synth with manufacturer:roland, role:sound-source

# Find it later
> Use graph_find to find devices tagged role:sound-source
```

## 📊 Example Queries (GraphQL via Trustfall)

### "What MIDI devices are connected?"

```graphql
query ConnectedMidi {
    AlsaMidiDevice {
        name @output
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
        name @filter(op: "=", value: ["jdxi"])
        pipewire_nodes {
            links {
                input_port {
                    node {
                        name @output
                        identity { name @output }
                    }
                }
            }
        }
        manual_connections_from {
            to_identity { name @output }
            to_port @output
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
            ports { direction @output }
        }
    }
}
```

## 🎼 Integration with HalfRemembered Ensemble

Audio Graph MCP is a **knowledge layer** for the ensemble:

1. **Agent discovers capabilities**: "What Eurorack modules are online?" → Tags guide routing
2. **Music generation with context**: Orpheus generates MIDI → Agent routes to appropriate device
3. **Signal flow understanding**: "Trace audio from Bitbox to mixer" → Troubleshoot feedback loops
4. **Patch documentation**: Record manual patch cables → Agents understand full signal path

## 📦 Project Structure

```
crates/audio-graph-mcp/
├── README.md                 ← You are here
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── main.rs              # MCP server entry
│   ├── schema.graphql       # Trustfall schema
│   ├── adapter/             # Trustfall adapter (joins sources)
│   ├── sources/             # ALSA, PipeWire, USB, SQLite
│   ├── tools/               # MCP tool implementations
│   ├── db/                  # SQLite schema & migrations
│   └── types.rs             # Core types
├── tests/
└── fixtures/                # Virtual device configs
```

## 🗂️ Implementation Plan

See `docs/agents/plans/07-graph-tools/tasks/` for detailed task breakdowns.

### Graph MVP (prove Trustfall on SQLite first)

| Order | Task | Description |
|-------|------|-------------|
| 1 | **[Task 01](../../docs/agents/plans/07-graph-tools/tasks/task-01-sqlite-foundation.md)** | SQLite foundation and schema |
| 2 | **[Task 04](../../docs/agents/plans/07-graph-tools/tasks/task-04-trustfall-adapter.md)** | Trustfall adapter (SQLite only) |
| 3 | **[Task 08](../../docs/agents/plans/07-graph-tools/tasks/task-08-testing-fixtures.md)** | Testing fixtures |
| 4 | **[Task 05](../../docs/agents/plans/07-graph-tools/tasks/task-05-mcp-tools.md)** | MCP tool interface |

### Live Sources (add incrementally)

| Order | Task | Description |
|-------|------|-------------|
| 5 | **[Task 02](../../docs/agents/plans/07-graph-tools/tasks/task-02-alsa-enumeration.md)** | ALSA MIDI enumeration |
| 6 | **[Task 03](../../docs/agents/plans/07-graph-tools/tasks/task-03-identity-matching.md)** | Identity matching / joins |
| 7 | **[Task 06](../../docs/agents/plans/07-graph-tools/tasks/task-06-pipewire-integration.md)** | PipeWire audio routing |
| 8 | **[Task 07](../../docs/agents/plans/07-graph-tools/tasks/task-07-manual-connections.md)** | Manual connection tracking |
| 9 | **[Task 09](../../docs/agents/plans/07-graph-tools/tasks/task-09-ensemble-integration.md)** | Hootenanny integration |

**OTEL**: Added incrementally as we build each component.

## 🔧 Hardware Context

Example devices this system will track:

- **Polyend Poly 2**: 8-voice MIDI-to-CV, gateway from MIDI to Eurorack
- **1010music Bitbox mk2**: Eurorack sampler, audio I/O bridge
- **Flame 4VOX**: Quad wavetable oscillator with MIDI/CV
- **Roland JD-Xi**: Desktop synth, USB MIDI, multi-engine
- **Arturia Keystep Pro**: MIDI controller + sequencer
- **Doepfer modules**: A-110 VCO, A-120 VCF, A-132 VCA, etc.

These represent different gateway types into the modular world, each with unique identity characteristics.

## 🧪 Testing Strategy

1. **Virtual MIDI devices** (`snd-virmidi`): Test enumeration, identity matching
2. **Mock PipeWire dumps**: Test JSON parsing, link resolution
3. **SQLite fixtures**: Test identity hint scoring, tag queries
4. **Integration tests**: Full query paths (live + persisted joins)
5. **Hardware validation**: Final testing with real Poly 2, JD-Xi, Eurorack

## 📚 References

- **Trustfall**: https://github.com/obi1kenobi/trustfall
- **"How to Query Everything"**: https://predr.ag/blog/how-to-query-almost-everything-hytradboi/
- **ALSA Sequencer**: https://www.alsa-project.org/alsa-doc/alsa-lib/seq.html
- **PipeWire**: https://docs.pipewire.org/
- **Architecture doc**: `docs/agents/plans/graph-mcp/claude-opus-audio-graph-mcp-architecture.md`

## 🎨 Philosophy

From the HalfRemembered ethos:

> "Expressiveness over Performance. We favor code that is rich in meaning. Use Rust's type system to tell a story."

This project embodies:
- **Rich types**: `Identity`, `HintKind`, `DeviceFingerprint` (not raw strings)
- **Enums as storytellers**: `BindingConfidence::High | Low | RequiresConfirmation`
- **Compiler as partner**: Trustfall's type safety ensures query correctness at compile time

---

**Status**: ✅ Graph MVP complete (23 tests). Ready for PipeWire integration.

**Next**: Pick a task from `docs/agents/plans/graph-mcp/tasks/` and start building!
