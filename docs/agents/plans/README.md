# HalfRemembered MCP Implementation Plans

## Overview

These plans guide the implementation of HalfRemembered MCP, progressing from basic infrastructure to a musical collaboration system where agents can jam together.

## Plan Sequence

### Phase 1: Foundation
- **[Plan 00: Event Duality Hello World](00-event-duality-hello/)** ✅ **COMPLETE**
  - ✅ Proves intentions become sounds
  - ✅ MCP server with SSE transport
  - ✅ The simplest truth validated
  - ✅ Refactored into `hootenanny` (MCP server) and `resonode` (music engine)

- **[Plan 02: CLI Client](02-cli/)** ⏳ Ready after Plan 00
  - Command-line interface for testing
  - Beautiful, stateless CLI
  - Unix-friendly piping support

### Phase 2: Musical Core 🎵
- **[Plan 03: Musical Domain Model](03-musical-domain/)** 🆕 **Critical**
  - Event Duality (Concrete/Abstract)
  - Conversation Tree with forking
  - Musical Context system
  - Agent communication protocol

### Phase 3: Advanced Features
- **[Plan 04: Musical Pattern Scripting](04-lua/)**
  - Lua-based pattern generators
  - Musical transformations
  - Hot-reloadable patterns
  - Integration with conversation tree

- **[Plan 05: Agent Collaboration](05-agent-collaboration/)** 🆕
  - Agent capability registry
  - Request queue system
  - Task delegation
  - Multi-agent workflows

### Phase 4: Future Expansions
- **Plan 06: Real-Time Audio** (Not yet defined)
  - CLAP plugin architecture
  - MIDI 2.0 support
  - Audio synthesis

- **Plan 07: Cloud Integration** (Not yet defined)
  - External model connections
  - Distributed jamming
  - Cloud agent federation

## Execution Order

```mermaid
graph TD
    A[Plan 00: Infrastructure] --> B[Plan 02: CLI]
    A --> C[Plan 03: Musical Domain]
    C --> D[Plan 04: Lua Patterns]
    C --> E[Plan 05: Agent Collaboration]
    D --> F[Plan 06: Audio]
    E --> F
    F --> G[Plan 07: Cloud]
```

## Key Concepts

Our plans incorporate these ideas from the domain model research:

1. **Conversation Trees**: Git-like branching for musical exploration
2. **Event Duality**: Both concrete data and abstract intentions
3. **Agent Request Queues**: True collaboration, not turn-taking
4. **Musical Context**: Shared knowledge that guides generation
5. **Temporal Forking**: Explore multiple musical futures simultaneously

## Success Metrics

We'll know we've succeeded when:
- ✅ Two agents can fork and explore ideas in parallel
- ✅ Musical context influences all generation
- ✅ Agents can delegate to specialists
- ✅ Conversations can be saved and resumed
- ✅ The system feels like jamming with musicians, not using a tool

## Current Status

- **Completed**:
  - ✅ Plan 00: Event Duality Hello World (tested end-to-end via MCP)
  - ✅ Domain model research, plan structure
  - ✅ Workspace refactor: `hootenanny` (server) + `resonode` (music engine)
- **In Progress**:
  - 🔄 Plan 03 (Musical Domain Model) - Gemini working on this
  - 🔄 Resonode foundation (EmotionalVector, MusicalPhrase, EmotionalEngine)
- **Next Up**:
  - Expand resonode musical capabilities
  - Implement hootenanny conversation trees
  - Plan 05 multi-agent collaboration
- **Future**: Audio synthesis, cloud integration

## Development Philosophy

From our research and plans:
- **Conversation over Commands**: Music emerges from dialogue
- **Exploration over Perfection**: Fork to try ideas, merge what works
- **Collaboration over Control**: Agents have specialties and autonomy
- **Musical over Technical**: Domain concepts drive the architecture

---

**Last Updated**: 2025-11-15
**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>