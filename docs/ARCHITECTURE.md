# Hootenanny Architecture

## Overview

Hootenanny is a Rust workspace composed of two main crates:
1.  **`hootenanny`**: The main application server. It runs the MCP transport (e.g., SSE, WebSockets), manages client connections, and orchestrates the overall state using an event-sourcing persistence layer.
2.  **`resonode`**: The core music generation engine. It implements the "Alchemical Codex" to translate emotional states (`EmotionalVector`) into musical expression. It is a pure, stateless library.

This project enables multi-agent collaboration for a human-AI music ensemble.

## System Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                      Multi-Agent Clients                          │
│                                                                    │
│  ┌──────────────┐  ┌──────────────┐  ┌────────┐  ┌───────────┐  │
│  │ 🤖 Claude    │  │ 💎 Gemini    │  │ 🦙 GUI │  │ 🎹 VST    │  │
│  │    Code      │  │              │  │ Client │  │  Plugin   │  │
│  └──────────────┘  └──────────────┘  └────────┘  └───────────┘  │
│         │                 │               │            │          │
└─────────┼─────────────────┼───────────────┼────────────┼──────────┘
          │                 │               │            │
          └─────────────────┴───────────────┴────────────┘
                                  │
                            SSE/WebSocket :8080
                        (Multi-client transport)
                                  │
          ┌───────────────────────▼───────────────────────┐
          │               Hootenanny Server               │
          │                                               │
          │   ┌──────────────────┐  ┌───────────────────┐ │
          │   │  hootenanny crate  │  │  resonode crate   │ │
          │   │ - Manages state    │  │ - Implements the  │ │
          │   │ - Handles network  │  │   Alchemical      │ │
          │   │ - Persistence      │  │   Codex           │ │
          │   └──────────────────┘  └───────────────────┘ │
          └────────────┬─────────────┬──────────┬─────────┘
                       │             │          │
          ┌────────────▼────┐   ┌────▼─────┐  ┌▼──────────┐
          │   Persistence   │   │   Lua    │  │   Music   │
          │  (Journaling &   │   │  Tools   │  │   Tools   │
          │   Snapshots)    │   │(Phase 2) │  │ (Phase 3) │
          └────────┬────────┘   └────┬─────┘  └─┬─────────┘
                   │                 │           │
          ┌────────▼────────┐   ┌────▼─────┐  ┌─▼─────────┐
          │   Filesystem    │   │   sled   │  │  Music    │
          │ (e.g. /tank/hr) │   │  State   │  │  Models   │
          └─────────────────┘   └──────────┘  └───────────┘
```

## Crate Structure

The project is a Rust workspace with two primary crates located in the `crates/` directory.

### `hootenanny`
- **Role**: The main application binary and server.
- **Responsibilities**:
    - Manages network transports (SSE, WebSockets).
    - Handles client connections and communication.
    - Owns and manages the overall state of the musical session.
    - Implements the event-sourcing persistence layer (journaling and snapshots).
    - Orchestrates calls to `resonode` to generate musical content.

### `resonode`
- **Role**: The core music generation engine.
- **Responsibilities**:
    - Provides the data structures for musical concepts, starting with the `EmotionalVector`.
    - Implements the transformation logic described in the **[Alchemical Codex](design/01-musical-alchemy.md)**.
    - Remains a pure, stateless library, taking in emotional state and returning musical data.

## State Management & Persistence

The system uses an **Event Sourcing** strategy powered by **AOL** and **Cap'n Proto** to ensure that the state of the musical jam session is durable, fast, and compact. This is managed by the `hootenanny` crate.

### Technology Stack

- **[AOL (Append-Only Log)](https://docs.rs/aol/)**: Simple, efficient event journal (no need to build our own)
- **[Cap'n Proto](https://capnproto.org/)**: Zero-copy serialization for speed and compactness
- **Focus**: Build music systems, not databases

See **[Persistence Architecture](design/persistence.md)** for detailed rationale.

### How It Works

1.  **The Journal** (AOL): Every change to the state is captured as an `Event` and appended to AOL's immutable log. This provides a complete, durable history of the session.

2.  **Serialization** (Cap'n Proto): Events are serialized using Cap'n Proto's zero-copy format, enabling:
    - **Fast writes**: Minimal overhead when models generate copious musical data
    - **Instant reads**: Zero-copy deserialization for playback
    - **Compact storage**: Efficient encoding of musical events and conversation trees

3.  **Snapshots**: Periodic snapshots of the current state for fast startup.

4.  **Startup Process**:
    - On launch, `hootenanny` loads the most recent snapshot
    - It then replays any events from AOL that occurred after the snapshot
    - This brings the system to its exact last-known state, ready to continue

### Why This Matters

Models will generate **copious musical data** during jam sessions. Cap'n Proto's zero-copy semantics and AOL's efficient append-only log mean we can handle thousands of events per session without performance degradation.

## Key Design Decisions

### 1. Workspace with `hootenanny` and `resonode`

**Decision**: Separate the server logic (`hootenanny`) from the music generation logic (`resonode`).

**Rationale**:
- **Logical Separation**: Enforces a clean boundary. `hootenanny` worries about state, time, and networks; `resonode` only worries about turning emotion into music.
- **Faster Compilation**: Changes to the server logic won't require recompiling the music engine, and vice-versa.
- **Clear API**: Forces a well-defined interface between the two crates, aligning with the "Compiler as Creative Partner" philosophy.
- **Reusability**: `resonode` could be used by other applications in the future.

### 2. AOL + Cap'n Proto for Persistence

**Decision**: Use AOL (Append-Only Log) for event sourcing and Cap'n Proto for serialization.

**Rationale**:
- **Don't Build Our Own**: AOL is simple, efficient infrastructure - focus on music, not databases
- **Performance**: Cap'n Proto's zero-copy semantics handle copious model-generated data
- **Durability**: The state of the jam is never lost on restart
- **Efficiency**: Compact encoding and instant deserialization for replay
- **Simplicity**: Lightweight append-only log without unnecessary complexity
- **Focus on Goals**: Build musical collaboration systems, not persistence infrastructure

See **[docs/design/persistence.md](design/persistence.md)** for detailed analysis.

### 3. SSE Transport (vs WebSocket)

**Decision**: Start with Server-Sent Events (SSE) for initial transport.

**Rationale**:
- **Simplicity**: SSE is a simple, one-way communication channel from server to client, which is sufficient for the initial phases.
- **HTTP-based**: Easier to debug and work with than the WebSocket protocol.
- **Sufficient for Now**: Can support multiple clients listening to the same stream of events.

**Future**: Can be upgraded to a bidirectional WebSocket transport when agents need to send more complex data back to the server.

## Deployment

### Build
```bash
# Build the entire workspace
cargo build --release
```

### Run
```bash
# Run the hootenanny server (requires a state directory)
./target/release/hootenanny --state-dir /path/to/your/state

# Connect clients
# MCP Inspector: npx @modelcontextprotocol/inspector http://localhost:8080/sse
```

## References

- **Alchemical Codex**: `docs/design/01-musical-alchemy.md`
- **Persistence Architecture**: `docs/design/persistence.md` ⭐ NEW
- **MCP Specification**: https://modelcontextprotocol.io
- **rmcp SDK**: https://github.com/modelcontextprotocol/rust-sdk
- **AOL Documentation**: https://docs.rs/aol/
- **Cap'n Proto**: https://capnproto.org/
- **Development Guidelines**: `docs/BOTS.md`
- **Project Context**: `docs/agents/CONTEXT.md`
- **Implementation Plans**: `docs/agents/plans/`

---

**Last Updated**: 2025-11-16
**Contributors**: 💎 Gemini (workspace refactor), 🤖 Claude (persistence docs)
**Architecture Status**: Workspace established. AOL + Cap'n Proto for persistence. Event Duality MCP server working.
