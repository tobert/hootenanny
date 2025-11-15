# HalfRemembered MCP Project Context

## Core Mission
Build an extensible MCP (Model Context Protocol) server that enables multi-agent collaboration for code review and, ultimately, creative music generation. The project serves as the foundation for a human-AI music ensemble.

## What We're Building

**halfremembered-mcp** is a WebSocket-based MCP server that provides:
- A foundational architecture for musical conversation between AI agents.
- A core data model based on "Event Duality" (Abstract and Concrete events).
- An extensible tool system for music generation and, eventually, other creative tasks.
- Multi-client WebSocket transport, enabling multiple agents to connect simultaneously.

## The Ensemble

### Current Team (Building Phase)
- **Amy Tobey (Human):** Vision holder, orchestrator, parallel agent coordinator
- **🤖 Claude:** Documentation, architecture, agent collaboration patterns
- **💎 Gemini:** WebSocket architecture, implementation plans, domain model refinement
- **Offline Models (DeepSeek, etc.):** Future performers via Ollama

### Future Performers (Music Phase)
- Music generation models running locally (ROCm GPU)
- VST plugins as MCP clients (experimental vision)
- Multiple agents composing together in real-time

## Technical Stack

**Language**: Rust (edition 2021)
**Async Runtime**: Tokio
**MCP SDK**: rmcp (Rust SDK for Model Context Protocol)
**Transport**: WebSocket (`rmcp::transport::websocket`) on 127.0.0.1:8080
**Serialization**: Serde, Schemars
**Error Handling**: anyhow::Result with context
**State (Phase 2)**: sled (embedded database)
**Version Control**: jj (Jujutsu) with git colocate

## Architecture

The core architecture is based on the "Event Duality" paradigm and a conversational, branching model of time inspired by git.

```
┌─────────────────────────────────────────────────────────────┐
│                    Multi-Agent Clients                       │
│  🤖 Claude Code    💎 Gemini    🦙 Local Agents   🎹 VSTs   │
└────────────┬────────────┬────────────┬────────────┬─────────┘
             │            │            │            │
             └────────────┴────────────┴────────────┘
                          │
                    WebSocket :8080
                          │
             ┌────────────▼────────────┐
             │   halfremembered-mcp    │
             │ (Event Duality Engine)  │
             └────────────┬────────────┘
                          │
     ┌────────────────────┴────────────────────┐
     │           Universal Timeline           │
     │ (Holds Abstract & Concrete Event Trees)│
     └────────────────────┬────────────────────┘
                          │
          ┌───────────────┴───────────────┐
          │                               │
    ┌─────▼─────┐                 ┌───────▼──────┐
    │  Agentic  │                 │  Performance │
    │  Streams  │                 │   Streams    │
    └───────────┘                 └──────────────┘
```

## Key Decisions Made

### Strategic Pivot on Initial Plan (💎 Gemini & Human)
**Decision**: The original `00-init` plan (focused on a generic DeepSeek tool server) is misaligned with the core musical vision. The new `00-init` plan will focus on implementing a "hello world" that directly demonstrates the "Event Duality" architecture.
**Why**: Building a generic tool server first would require significant rework. Starting with a minimal version of the *correct* architecture ensures all progress is foundational.
**Benefit**: Establishes the right patterns from day one; avoids building a throwaway prototype.
**Date**: 2025-11-15

### Multi-Agent Temporal Forking (🤖 Claude & 💎 Gemini)
**Decision**: Adopt a conversational, branching model for musical collaboration, similar to git. Agents can "fork" musical ideas to explore them in parallel.
**Why**: Better models the non-linear, collaborative nature of real-world musical creation and resolves creative disagreements productively.
**Enhancements (Gemini)**: The protocol was amended to include structured `ForkReason` enums, an `AttentionFocus` mechanism, and richer `JamMessage` feedback to make it more effective for autonomous agents.
**Date**: 2025-11-15

### WebSocket Transport (💎 Gemini)
**Decision**: Use WebSocket instead of stdio transport.
**Why**: Enable multiple agents to connect simultaneously, which is essential for the ensemble vision.
**Date**: 2025-11-15

## Roadmap & Phases

### Phase 0: Research & Planning (Complete) ✅
- Foundational research documents created.
- Core architectural principles (Event Duality, Temporal Forking) defined.
- Development guidelines and memory system established.

### Phase 1: Core Domain & Event Duality "Hello World" (Current)
**Goal**: Ship a minimal, working MCP server that proves the core architectural pattern.
**Plan**: A new `docs/agents/plans/00-init/plan.md` will be created.
**Success Criteria**:
- ✅ Project builds with a minimal `src/domain.rs`.
- ✅ An `AbstractEvent` can be sent to a tool.
- ✅ The tool returns a corresponding `ConcreteEvent`.
- ✅ The entire exchange is validated via MCP Inspector.

**Deliverables**:
- `halfremembered_mcp` binary.
- A `src/domain.rs` file with minimal `AbstractEvent` and `ConcreteEvent` enums.
- A single tool that converts one to the other.
- A new, aligned `00-init/plan.md`.

### Phase 2: The Non-Real-Time Engine (Planned)
**Goal**: Build the main application logic, including the `UniversalTimeline` and persistence.
**Plan**: TBD, will follow the `implementation-vision.md` roadmap.

### Phase 3: The Real-Time Engine (Vision)
**Goal**: Create the audio-thread-safe playback engine within a CLAP plugin.

### Phase 4: The Collaborative Layer (Vision)
**Goal**: Implement suggestion layers, human-in-the-loop curation, and advanced agent behaviors.

## Current Status (2025-11-15, 23:30)

**Where We Are**: Phase 0 complete. Ready to begin Phase 1 implementation.
**Latest Work**:
- 🤖 Claude: Created Event Duality Hello World plan, comprehensive domain model documentation
- Plans restructured: Old DeepSeek plan archived, new music-first approach ready

**Next Steps**:
1. **Execute `/docs/agents/plans/00-event-duality-hello/plan.md`** - 30 min to first sound
2. Test with MCP Inspector: intention → sound transformation
3. Expand to full domain model (Plan 03)

**Blockers**: None. Implementation path is crystal clear.

## Active Questions
- The previous questions about DeepSeek and the old MVP are no longer relevant.
- **New Question**: What are the most essential, minimal `AbstractEvent` and `ConcreteEvent` to implement for the "hello world" proof of concept? (e.g., `SayHello` -> `Greeting`? or something more musical like `PlayNote` -> `NotePlayed`?)

## Handoff Notes

### For Future Sessions
- **The plan has changed.** We are no longer building a DeepSeek tool server first. The priority is the core musical domain model.
- Refer to `implementation-vision.md` and this document for the current roadmap.
- The next action is to create a new `00-init/plan.md`.

---

**Last Updated**: 2025-11-15 by 💎 Gemini
**Status**: Phase 1 plan being revised.
**Next Milestone**: A working "Event Duality Hello World" MVP.