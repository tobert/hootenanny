# Plan 03: Musical Domain Model

**Status**: Ready to implement
**Dependencies**: Plan 00 (core MCP server) should be functional
**Timeline**: 1-2 weeks
**Priority**: Critical - this is the heart of our system

## Overview

Implement the core musical domain model based on our research. This plan introduces the concepts that make HalfRemembered unique: **Conversation Trees**, **Event Duality**, and **Musical Context**.

## Before Starting

📝 **Read [test-driven-approach.md](../test-driven-approach.md)** first. The critical tests there drive our domain model design. Write those failing tests before implementing any musical types.

## Goals

1. **Event Duality**: Implement both Concrete (performance data) and Abstract (intentions) events
2. **Conversation Tree**: Build the branching, forkable structure for musical exploration
3. **Musical Context**: Create the shared knowledge system that guides generation
4. **Agent Communication**: Basic inter-agent musical messages
5. **Branch Management**: Fork, prune, merge operations

## Core Components

### 1. Event System
```rust
pub enum Event {
    Concrete(ConcreteEvent),  // Notes, MIDI data
    Abstract(AbstractEvent),   // Prompts, constraints
}
```

### 2. Conversation Tree
```rust
pub struct ConversationTree {
    root: NodeId,
    nodes: HashMap<NodeId, ConversationNode>,
    branches: HashMap<BranchId, Branch>,
}
```

### 3. Musical Context
```rust
pub struct MusicalContext {
    key: Key,
    tempo: Tempo,
    time_signature: TimeSignature,
    emotional_state: EmotionalVector,
}
```

## Success Criteria

- [ ] Can create and traverse a conversation tree
- [ ] Can fork branches for parallel exploration
- [ ] Can merge compatible branches
- [ ] Events maintain both concrete and abstract representations
- [ ] Musical context influences event generation
- [ ] Basic agent-to-agent communication works
- [ ] Can serialize/deserialize conversations for persistence

## Implementation Steps

### Step 1: Core Types (2 days)
- Define Event, ConcreteEvent, AbstractEvent enums
- Implement Note with MIDI 2.0 fields
- Create MusicalContext struct
- Add basic musical types (Key, Scale, Chord)

### Step 2: Conversation Tree (3 days)
- Implement ConversationNode and Branch
- Add fork() and merge() operations
- Create tree traversal methods
- Add pruning for failed branches

### Step 3: Agent Messages (2 days)
- Define JamMessage enum
- Implement Intention, Acknowledgment, Suggestion
- Create basic message routing
- Add branch evaluation messages

### Step 4: MCP Integration (2 days)
- Extend MCP protocol for musical operations
- Add conversation.fork, conversation.merge methods
- Implement branch switching during playback
- Create event streaming over WebSocket

### Step 5: Testing & Examples (2 days)
- Create example two-agent jam session
- Test fork and merge operations
- Demonstrate parallel exploration
- Document the API

## Files to Create

```
src/
├── domain/
│   ├── mod.rs
│   ├── events.rs         # Event, Concrete, Abstract
│   ├── conversation.rs   # Tree, Node, Branch
│   ├── context.rs        # MusicalContext
│   ├── messages.rs       # JamMessage types
│   └── music.rs          # Note, Chord, Key, Scale
├── conversation/
│   ├── mod.rs
│   ├── tree.rs           # Tree operations
│   ├── forking.rs        # Fork/merge logic
│   └── routing.rs        # Message routing
└── mcp_extensions/
    ├── mod.rs
    └── musical.rs        # Musical MCP methods
```

## Integration Points

### With Plan 00 (MCP Server)
- Extends the base MCP server with musical methods
- Uses WebSocket transport for real-time communication
- Leverages existing error handling and logging

### With Plan 02 (CLI)
- CLI gains musical commands:
  - `hrmcp conversation new`
  - `hrmcp fork <reason>`
  - `hrmcp merge <branch>`
  - `hrmcp branch list`

### With Future Plan 04 (Lua Tools)
- Lua scripts can create musical events
- Scripts can listen to conversation events
- Pattern generators as Lua tools

## Key Innovations from Research

1. **Conversation-Centric**: Music emerges from dialogue, not dictation
2. **Git-like Branching**: Explore multiple ideas simultaneously
3. **Lazy Realization**: Keep possibilities open until performance
4. **Agent Collaboration**: Agents build on each other's work

## Example Usage

```rust
// Create a conversation
let mut conversation = Conversation::new();
let root = conversation.root();

// Agent 1 plays a melody
let melody = ConcreteEvent::melody(vec![C, E, G, F, E, D, C]);
let node1 = conversation.add_utterance(root, Agent::Claude, melody);

// Agent 2 forks to explore two responses
let harmony_branch = conversation.fork(node1, "explore_harmony");
let bass_branch = conversation.fork(node1, "explore_bassline");

// Add content to branches
conversation.add_to_branch(harmony_branch, harmony_events);
conversation.add_to_branch(bass_branch, bass_events);

// Evaluate and merge the best
if conversation.evaluate(harmony_branch) > 0.7 {
    conversation.merge(harmony_branch, main_branch);
}
```

## Dependencies to Add

```toml
# Musical types
midi-types = "0.1"  # MIDI 2.0 support

# Tree operations
petgraph = "0.6"    # Graph algorithms

# Serialization
bincode = "1.3"     # Fast binary serialization
```

## Next Steps (Plan 04)

After this foundation:
1. **Agent Request Queue**: Agents delegating tasks
2. **Capability Discovery**: Finding specialist agents
3. **Complex Workflows**: Multi-step generation chains

## Notes

This plan represents the **core innovation** of HalfRemembered. Without this musical domain model, we're just another MCP server. With it, we enable genuine musical collaboration between humans and AI.

The conversation tree with forking is what allows agents to "jam" rather than just take turns. This is the difference between sequential generation and true creative exploration.

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>
**Date**: 2025-11-15
**Status**: Ready for implementation