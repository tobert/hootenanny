# Hootenanny Architecture

## Crate Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              MCP Clients                                     │
│                    (Claude, other AI agents, CLI tools)                      │
└──────────────────────────────────┬──────────────────────────────────────────┘
                                   │ HTTP/SSE (MCP Protocol)
                                   ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              HOLLER                                          │
│                         (MCP Gateway + CLI)                                  │
│  • Thin MCP-to-ZMQ bridge                                                   │
│  • Routes tool calls to backends via ZMQ                                    │
│  • CLI subcommands for manual operations                                    │
└──────────────────────────────────┬──────────────────────────────────────────┘
                                   │ ZMQ (hooteproto messages)
                    ┌──────────────┼──────────────┐
                    │              │              │
                    ▼              ▼              ▼
┌─────────────────────────┐ ┌─────────────┐ ┌─────────────────────────────────┐
│      HOOTENANNY         │ │  LUANETTE   │ │        CHAOSGARDEN              │
│    (Control Plane)      │ │  (Lua VM)   │ │     (Realtime Audio)            │
│                         │ │             │ │                                 │
│ • Job orchestration     │ │ • Scripts   │ │ • PipeWire integration          │
│ • CAS management        │ │ • Workflows │ │ • Timeline playback             │
│ • GPU service calls     │ │ • MCP proxy │ │ • Audio routing                 │
│ • Artifact tracking     │ │             │ │                                 │
└────────────┬────────────┘ └──────┬──────┘ └─────────────────────────────────┘
             │                     │
             │                     │ (calls via ZMQ)
             └──────────┬──────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                          GPU SERVICES                                        │
│  (External HTTP services on localhost ports)                                 │
│                                                                              │
│  • Orpheus (2000-2003) - MIDI generation, bridge, classifier, loops         │
│  • MusicGen (2006) - Text-to-music                                          │
│  • CLAP (2007) - Audio embeddings                                           │
│  • YuE (2008) - Text-to-song with vocals                                    │
│  • BeatThis (2012) - Beat detection                                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Crate Responsibilities

### hooteproto
**Wire protocol types for ZMQ messaging**

- `Envelope` - Message wrapper with tracing metadata
- `Payload` - All message types (tool calls, responses, events)
- `JobId`, `JobStatus`, `JobInfo` - Shared job system types
- `Broadcast` - PUB/SUB event types

### hootenanny
**Control plane and orchestration**

- MCP server with tools for music generation, CAS, graph queries
- Job system for async GPU operations
- HTTP clients for GPU services (Orpheus, MusicGen, etc.)
- Artifact tracking and lineage
- ZMQ client for chaosgarden control

### luanette
**Lua scripting engine**

- Sandboxed Lua runtime for workflow scripts
- ZMQ server for receiving script execution requests
- MCP proxy for calling upstream tools
- Job system for tracking script execution

### holler
**MCP gateway and CLI**

- HTTP/SSE server for MCP clients
- Routes tool calls to backends via ZMQ
- CLI subcommands for direct tool invocation
- ZMQ SUB for broadcast events (→ SSE to clients)

### chaosgarden
**Realtime audio daemon**

- PipeWire integration for audio routing
- Timeline-based playback with transport controls
- ZMQ server for control messages
- Audio graph queries via Trustfall

### baton
**Generic MCP server library**

- Axum-based MCP transport (SSE, streamable HTTP)
- Session management
- Tool, resource, prompt types
- Used by hootenanny and luanette for MCP handling

### cas
**Content Addressable Storage**

- BLAKE3 hashing
- File-based store with metadata
- Shared by hootenanny, chaosgarden, luanette

### abc
**ABC notation parsing**

- Parser for ABC musical notation
- MIDI conversion
- Validation

### audio-graph-mcp
**Trustfall adapter for audio queries**

- Unified query layer over artifacts, devices, connections
- GraphQL-like queries via Trustfall


**LLM integration**

- Chat state machine
- Multi-backend support (DeepSeek, Ollama)
- Agent spawning for sub-tasks

## Data Flow

### Tool Call Flow
```
Client → Holler (HTTP) → ZMQ → Hootenanny/Luanette → GPU Service
                                        ↓
                                    JobStore
                                        ↓
Client ← Holler (SSE) ← ZMQ ← Hootenanny/Luanette
```

### Job Lifecycle
```
1. Tool call arrives (e.g., orpheus_generate)
2. JobStore.create_job() → returns JobId immediately
3. Background task spawns, calls GPU service
4. JobStore.mark_running()
5. GPU service returns result
6. JobStore.mark_complete() or mark_failed()
7. Client polls with job_poll or job_status
```

### Broadcast Flow
```
Backend (hootenanny/chaosgarden) → ZMQ PUB → Holler SUB → SSE → Clients
```

## Shared Types (hooteproto)

The following types are defined once in hooteproto and used by multiple crates:

- `JobId` - UUID-based job identifier
- `JobStatus` - Pending, Running, Complete, Failed, Cancelled
- `JobInfo` - Full job metadata with timestamps
- `JobStoreStats` - Aggregate statistics

This ensures wire compatibility and consistent behavior across the system.
