# Deep Prompts & Resources Integration

**Goal**: Evolve hootenanny's MCP prompts and resources from simple data dumps to semantically-rich interfaces that understand musical context, conversation history, and artifact relationships.

## The Three-Layer Semantic Stack

Hootenanny has a unique architecture worth preserving:

```
┌─────────────────────────────────────────────────────────────┐
│ INTENTION LAYER (Abstract Events + Prompts)                 │
│ "Play C softly with sadness"                                │
│ Human-like, emotional, directional                          │
├─────────────────────────────────────────────────────────────┤
│ EMOTIONAL LAYER (EmotionalVector)                           │
│ valence: -0.3, arousal: 0.2, agency: -0.2                   │
│ Bridge between intent and execution                         │
├─────────────────────────────────────────────────────────────┤
│ REALIZATION LAYER (ConcreteEvent → MIDI)                    │
│ pitch: 60, velocity: 40, duration: 700ms                    │
│ Deterministic transformation                                │
└─────────────────────────────────────────────────────────────┘
```

**Key insight**: Prompts and resources should expose all three layers, enabling agents to reason about intent, emotion, AND sound.

## Current State

### Resources (handler.rs)
| URI | What it exposes |
|-----|-----------------|
| `graph://identities` | Audio device list |
| `graph://connections` | Patch cables |
| `session://tree` | Summary only (node_count, root) |

### Prompts (handler.rs)
| Name | Context injected |
|------|------------------|
| `ensemble-jam` | Device list |
| `describe-setup` | Device list + connections |
| `patch-synth` | Single device lookup |
| `sequence-idea` | Device list |

**Gap**: No access to conversation history, artifacts, emotional state, or musical context.

## Proposed Architecture

### Phase 1: Conversation Tree Resources
Expose the git-like conversation structure for agent reasoning.

### Phase 2: Artifact & CAS Resources
Make generated MIDI queryable by tag, variation, lineage.

### Phase 3: Musical Context Resources
Expose tempo, key, harmony, constraints as live data.

### Phase 4: Music-Aware Prompts
Prompts that understand what's been generated and suggest next steps.

### Phase 5: Emotional Intelligence Prompts
Prompts that reason about emotional arcs and realization mapping.

## Task Overview

| Task | Description | Complexity |
|------|-------------|------------|
| [Task 01](task_01_conversation_resources.md) | Conversation tree resources | Medium |
| [Task 02](task_02_artifact_resources.md) | Artifact store resources | Medium |
| [Task 03](task_03_musical_context.md) | Musical context resources | Low |
| [Task 04](task_04_music_aware_prompts.md) | Generation-aware prompts | Medium |
| [Task 05](task_05_emotional_prompts.md) | Emotional reasoning prompts | High |

## Design Principles

1. **Semantic over syntactic**: Resources should convey meaning, not just data
2. **Composable**: Resources should work together (e.g., artifact + emotion)
3. **Temporal**: Support querying across time (branch history, emotional arc)
4. **Agent-friendly**: Prompts should enable informed decision-making

## Key Files

| File | Purpose |
|------|---------|
| `src/api/handler.rs` | Current prompts/resources implementation |
| `src/conversation.rs` | ConversationTree, branches, nodes |
| `src/artifact_store.rs` | Artifact metadata, tags, variation sets |
| `src/domain/context.rs` | MusicalContext, TimeMap |
| `src/domain.rs` | Event, EmotionalVector, realization |
| `src/mcp_tools/local_models.rs` | Orpheus integration, CAS |
