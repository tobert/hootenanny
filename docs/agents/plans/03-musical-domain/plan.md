# Plan 03: Musical Domain Model - Implementation Prompts

## Prompt 1: Core Musical Types

```
Create the core musical type system for HalfRemembered MCP.

In src/domain/mod.rs, create the module structure.
In src/domain/music.rs, implement:

1. Basic musical types:
   - Note (with MIDI 2.0 fields: pitch, velocity_u16, articulation)
   - Pitch (with frequency and MIDI note number)
   - Duration (musical and absolute time)
   - Velocity (16-bit for MIDI 2.0)
   - Articulation enum (Legato, Staccato, Tenuto, etc.)

2. Harmonic types:
   - Key enum (C_major, A_minor, etc.)
   - Scale (with interval patterns)
   - Chord (root + quality + voicing)
   - ChordQuality enum (Major, Minor, Diminished, etc.)

3. Temporal types:
   - TimeSignature
   - Tempo (BPM with decimal precision)
   - MusicalTime (bars:beats:ticks)
   - AbsoluteTime (milliseconds)

Make everything serializable with serde and provide Display implementations.
Use the newtype pattern for type safety (e.g., struct Velocity(u16)).

Add conversion methods between musical and absolute time.
Include builder patterns for complex types like Chord.
```

## Prompt 2: Event Duality System

```
Implement the Event Duality system that makes HalfRemembered unique.

In src/domain/events.rs:

1. Create the dual Event enum:
   ```rust
   pub enum Event {
       Concrete(ConcreteEvent),
       Abstract(AbstractEvent),
   }
   ```

2. ConcreteEvent variants:
   - Note(NoteEvent) - single note with all MIDI 2.0 data
   - Chord(ChordEvent) - multiple simultaneous notes
   - Control(ControlEvent) - parameter changes
   - Pattern(PatternInstance) - expanded pattern reference

3. AbstractEvent variants:
   - Prompt(PromptEvent) - natural language instruction
   - Constraint(ConstraintEvent) - musical rules/boundaries
   - Orchestration(OrchestrationEvent) - multi-agent coordination
   - Intention(IntentionEvent) - musical goals

4. Each event needs:
   - Timestamp (when it occurs/applies)
   - Source (which agent created it)
   - Context (key, tempo at that moment)
   - Unique ID for tracking

5. Implement methods:
   - is_concrete() / is_abstract()
   - to_concrete() - for Abstract events that can generate Concrete
   - applies_to(TimeRange) - check if event is relevant

Include comprehensive tests showing event creation and transformation.
```

## Prompt 3: Conversation Tree Implementation

```
Build the Conversation Tree that enables musical forking and merging.

In src/conversation/tree.rs:

1. Core structures:
   ```rust
   pub struct ConversationTree {
       nodes: HashMap<NodeId, ConversationNode>,
       branches: HashMap<BranchId, Branch>,
       root: NodeId,
       current_heads: HashSet<BranchId>,
   }

   pub struct ConversationNode {
       id: NodeId,
       parent: Option<NodeId>,
       children: Vec<NodeId>,
       content: Event,
       branch_id: BranchId,
       timestamp: Timestamp,
       author: AgentId,
   }

   pub struct Branch {
       id: BranchId,
       name: String,
       base_node: NodeId,
       head_node: NodeId,
       state: BranchState,
   }
   ```

2. Essential operations:
   - new() - create tree with root
   - add_utterance() - add event to current branch
   - fork() - create new branch from node
   - merge() - combine branches
   - prune() - remove failed branches
   - switch_branch() - change active branch

3. Traversal methods:
   - ancestors(node) - walk up to root
   - descendants(node) - all children recursively
   - branch_history(branch) - all nodes in branch
   - find_common_ancestor(node1, node2)

4. Fork reasons enum:
   - ExploreAlternative(description)
   - AgentDisagreement(agents)
   - UserRequest(reason)
   - EmotionalDivergence

5. Merge strategies:
   - Overlay - combine both branches
   - Replace - take one branch
   - Interleave - alternate events
   - Manual - user selects per event

Include visualization method that outputs tree structure as ASCII art.
```

## Prompt 4: Musical Context System

```
Create the Musical Context system that provides shared knowledge.

In src/domain/context.rs:

1. Main context structure:
   ```rust
   pub struct MusicalContext {
       // Temporal maps (things change over time)
       tempo_map: TimeMap<Tempo>,
       key_map: TimeMap<Key>,
       time_signature_map: TimeMap<TimeSignature>,
       chord_progression: TimeMap<Chord>,

       // Current state
       emotional_state: EmotionalVector,
       energy_level: f32,
       complexity: f32,

       // Constraints
       scale_constraints: Option<Scale>,
       rhythm_constraints: Option<RhythmPattern>,
   }
   ```

2. TimeMap for temporal values:
   ```rust
   pub struct TimeMap<T> {
       changes: BTreeMap<MusicalTime, T>,
   }
   ```
   - at(time) - get value at time
   - set(time, value) - add change point
   - range(start, end) - get all changes

3. Context queries:
   - current_key(at_time)
   - current_chord(at_time)
   - is_note_valid(note, at_time)
   - suggest_next_note(previous, at_time)

4. Context influence:
   - apply_to_event(event) - constrain to context
   - evaluate_fitness(event) - how well it fits
   - generate_variations(event) - contextual alternatives

5. Emotional system:
   ```rust
   pub struct EmotionalVector {
       valence: f32,    // happy-sad
       arousal: f32,    // calm-excited
       dominance: f32,  // submissive-dominant
   }
   ```

Include methods for context interpolation and blending multiple contexts.
```

## Prompt 5: Agent Communication Protocol

```
Implement the agent communication system for musical collaboration.

In src/domain/messages.rs:

1. Core message types:
   ```rust
   pub enum JamMessage {
       // Musical communication
       Intention {
           agent: AgentId,
           planned: Event,
           confidence: f32,
           timing: TimingIntention,
       },

       Acknowledgment {
           agent: AgentId,
           heard: NodeId,
           interpretation: MusicalInterpretation,
           response_type: ResponseType,
       },

       Suggestion {
           agent: AgentId,
           suggestion: Event,
           rationale: String,
           target_branch: Option<BranchId>,
       },

       // Branch operations
       ForkRequest {
           agent: AgentId,
           from_node: NodeId,
           reason: ForkReason,
       },

       BranchEvaluation {
           agent: AgentId,
           branch: BranchId,
           score: f32,
           continue: bool,
       },
   }
   ```

2. Response types:
   - WillComplement
   - WillContrast
   - WillImitate
   - WillDevelop
   - WillRest

3. Message routing:
   ```rust
   pub struct MessageRouter {
       subscribers: HashMap<AgentId, MessageFilter>,
       message_queue: VecDeque<(JamMessage, AgentId)>,
   }
   ```

4. Conversation protocol:
   - join_conversation(agent)
   - leave_conversation(agent)
   - broadcast(message)
   - send_to(agent, message)
   - request_response(agent, message) -> Future<Response>

5. Message serialization for MCP transport.

Include examples of typical message exchanges during a jam session.
```

## Prompt 6: MCP Musical Extensions

```
Extend the MCP protocol with musical operations.

In src/mcp_extensions/musical.rs:

1. Add musical MCP methods:
   ```rust
   #[rpc]
   impl MusicalMCP {
       // Conversation management
       async fn create_conversation(&self, context: MusicalContext) -> ConversationId;
       async fn join_conversation(&self, id: ConversationId, agent: AgentId) -> Result<()>;

       // Tree operations
       async fn fork_branch(&self, from: NodeId, reason: String) -> BranchId;
       async fn merge_branches(&self, from: BranchId, into: BranchId) -> NodeId;
       async fn prune_branch(&self, branch: BranchId) -> Result<()>;

       // Musical operations
       async fn add_event(&self, branch: BranchId, event: Event) -> NodeId;
       async fn evaluate_branch(&self, branch: BranchId) -> f32;
       async fn get_context(&self, at_time: MusicalTime) -> MusicalContext;

       // Real-time coordination
       async fn subscribe_events(&self) -> EventStream;
       async fn broadcast_message(&self, msg: JamMessage) -> Result<()>;
   }
   ```

2. Streaming support:
   - Event stream for real-time updates
   - Branch change notifications
   - Context change broadcasts

3. Error handling:
   - BranchNotFound
   - MergeConflict
   - InvalidMusicalOperation
   - AgentNotAuthorized

4. Integration with base MCP server from Plan 00.

5. WebSocket message framing for musical data.

Test with multiple connected clients forking and merging.
```

## Prompt 7: Persistence and Serialization

```
Implement persistence for musical conversations using sled.

In src/persistence/mod.rs:

1. Storage schema:
   ```rust
   pub struct ConversationStore {
       db: sled::Db,
       conversations: sled::Tree,  // id -> Conversation
       nodes: sled::Tree,          // node_id -> Node
       branches: sled::Tree,       // branch_id -> Branch
       events: sled::Tree,         // event_id -> Event
   }
   ```

2. Serialization strategy:
   - Use bincode for compact binary format
   - Version all structures for migration
   - Compress large event sequences

3. Operations:
   - save_conversation(conversation)
   - load_conversation(id) -> Conversation
   - save_checkpoint(branch) - partial save
   - list_conversations() -> Vec<ConversationMeta>
   - export_as_midi(branch) - for external use

4. Lazy loading:
   - Load conversation metadata eagerly
   - Load nodes/events on demand
   - Cache recently accessed branches

5. Transaction support for atomic operations:
   - Fork + add event as transaction
   - Merge as atomic operation

Include migration utilities for schema updates.
```

## Prompt 8: Testing and Examples

```
Create comprehensive tests and example usage.

In examples/two_agent_jam.rs:

Create a complete example showing:
1. Two agents starting a conversation
2. One plays a simple melody
3. Other forks to explore harmony and bass
4. They evaluate each other's branches
5. Best branch gets merged
6. They continue building on it

In tests/:

1. Unit tests for each component:
   - Event creation and transformation
   - Tree forking and merging
   - Context queries
   - Message routing

2. Integration tests:
   - Full conversation flow
   - Multi-agent coordination
   - Branch conflict resolution
   - Performance under load

3. Property tests with proptest:
   - Tree operations maintain invariants
   - Merge is associative
   - Context interpolation is smooth

4. Benchmarks:
   - Tree operations with many branches
   - Event serialization speed
   - Context query performance

Document performance characteristics and limits.
```

## Success Checklist

After implementing these prompts:

- [ ] Musical types model real musical concepts
- [ ] Events can be both concrete and abstract
- [ ] Conversation tree supports branching workflows
- [ ] Musical context influences generation
- [ ] Agents can communicate musically
- [ ] MCP protocol extended for music
- [ ] Conversations persist across sessions
- [ ] Examples demonstrate the system

## Next Steps

With this foundation, we can:
1. Add the Agent Request Queue (Plan 04)
2. Implement real-time audio (Plan 05)
3. Create Lua pattern generators (Plan 06)
4. Build the CLI for human interaction

---

**Note**: Each prompt builds on the previous. Complete them in order for best results.