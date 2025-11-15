# Plan 05: Agent Collaboration & Request Queue

**Status**: Planned for Phase 4
**Dependencies**: Requires Plan 03 (Musical Domain) complete
**Timeline**: After basic conversation tree works
**Priority**: High - enables true multi-agent jamming

## Overview

Implement the Agent Request Queue system that allows agents to discover each other's capabilities and delegate musical tasks. This transforms our system from parallel monologues into true collaboration where agents build on each other's strengths.

## Before Starting

📝 **Read [test-driven-approach.md](../test-driven-approach.md)** first. Tests should simulate realistic jam sessions where agents genuinely build on each other's ideas.

See [realistic-jam-test.md](realistic-jam-test.md) for the creative vision and [deterministic-collaboration-test.md](deterministic-collaboration-test.md) for practical mock-based testing with `mockall`.

## Goals

1. **Capability Registry**: Agents advertise what they can do
2. **Request Queue**: Priority-based task delegation system
3. **Request Routing**: Find the best agent for each task
4. **Chain Workflows**: Multi-step generation pipelines
5. **Graceful Fallback**: Handle agent failures elegantly

## Core Components

### 1. Capability System
```rust
pub struct AgentCapabilities {
    generation: Vec<GenerationCapability>,
    analysis: Vec<AnalysisCapability>,
    performance: PerformanceProfile,
}
```

### 2. Request Queue
```rust
pub struct AgentRequestQueue {
    pending: PriorityQueue<AgentRequest>,
    in_progress: HashMap<RequestId, InProgressRequest>,
    router: RequestRouter,
}
```

### 3. Request Types
- Generation (melody, bass, drums, harmony)
- Analysis (emotional, structural, harmonic)
- Transformation (transpose, style transfer)
- Evaluation (branch scoring, quality assessment)

## Success Criteria

- [ ] Agents can discover available specialists
- [ ] Claude can request bass line from BassBot
- [ ] Requests route to most capable agent
- [ ] Failed requests fall back gracefully
- [ ] Request chains execute in order
- [ ] Performance metrics tracked per agent
- [ ] Circuit breakers prevent cascading failures

## Implementation Steps

### Step 1: Capability Registry (2 days)
- Define capability types
- Implement registry with discovery
- Create capability matching algorithm
- Add performance profiling

### Step 2: Request Queue (3 days)
- Build priority queue structure
- Implement request lifecycle
- Add timeout handling
- Create dead letter queue

### Step 3: Routing Strategies (2 days)
- Capability-based routing
- Load balancing
- Performance-based selection
- Fallback chains

### Step 4: Request Chains (2 days)
- Define chain syntax
- Implement dependency resolution
- Add parallel execution where possible
- Handle partial failures

### Step 5: Quality of Service (2 days)
- Circuit breakers per agent
- Request deduplication
- Result caching
- SLA monitoring

## Integration Points

### With Plan 03 (Musical Domain)
- Requests carry MusicalContext
- Results become Events in conversation
- Branch operations trigger requests

### With Plan 04 (Lua Patterns)
- Lua scripts can submit requests
- Pattern generators register as agents
- Scripts can handle simple requests

### With Future Plans
- Real-time audio agents register capabilities
- Human users appear as agents with full capabilities
- External services (cloud models) as agents

## Example Scenarios

### Scenario 1: Bass Generation
```rust
// Claude needs a bass line
let request = AgentRequest::Generate {
    target: GenerationTarget::BassLine,
    context: musical_context,
    constraints: BassConstraints::WalkingBass,
};

// System finds BassBot has best capability match
let agent = registry.find_best_match(&request);

// BassBot generates and returns
let bass_events = agent.process(request).await?;
```

### Scenario 2: Complex Arrangement
```rust
// Chain multiple specialists
let chain = RequestChain::new()
    .then(harmony_agent, "Generate chord progression")
    .parallel(vec![
        (bass_agent, "Create bass line"),
        (drum_agent, "Add rhythm"),
    ])
    .then(mixer_agent, "Combine tracks");

let result = queue.execute_chain(chain).await?;
```

## Key Innovations

1. **Heterogeneous Agents**: Different models with different strengths
2. **Dynamic Discovery**: New agents can join anytime
3. **Graceful Degradation**: System works even if some agents fail
4. **Request Batching**: Similar requests processed together
5. **Learning**: Track which agents excel at what

## Files to Create

```
src/
├── collaboration/
│   ├── mod.rs
│   ├── capabilities.rs    # Capability definitions
│   ├── registry.rs        # Agent registry
│   ├── queue.rs           # Request queue
│   ├── routing.rs         # Routing strategies
│   └── chains.rs          # Request chains
```

## Next Steps (Plan 06)

After agent collaboration works:
1. Real-time audio synthesis agents
2. Cloud model integration
3. Human-in-the-loop agents
4. Distributed agent networks

## Notes

This system is crucial for the vision of agents truly jamming together. Without it, agents can only respond to the shared conversation. With it, they can actively collaborate, delegate to specialists, and build complex musical structures together.

The request queue essentially creates a "musical microservices" architecture where each agent is a specialized service that can be composed into workflows.

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>
**Date**: 2025-11-15
**Status**: Planned for after musical foundation