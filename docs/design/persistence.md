# Persistence Architecture: Sled Embedded Database

**Decision Date**: 2025-11-16
**Status**: Adopted
**Contributors**: Amy Tobey, 💎 Gemini, 🤖 Claude

## The Choice

HalfRemembered uses **Sled** - a lightweight pure-Rust transactional embedded database.

## Why Sled?

**We're building a conversation graph, not just an event log.** After analyzing our data structures, we realized we need:

### What We Actually Need
1. **Event Log** - Sequential musical events (what happened when)
2. **Conversation Tree** - Graph structure (who said what, forking points, relationships)
3. **Queries** - "Show me all events in this branch", "What was the emotional state at fork X?"
4. **Graph Navigation** - Walk parent/child relationships
5. **Atomic Operations** - Fork conversations without corruption

### Why Not AOL or Cap'n Proto Alone?

**AOL**: Append-only logs are great for simple event streams, but require implementing `Record` and `Snapshot` traits for every type. More importantly, they're designed for linear replay, not graph queries.

**Cap'n Proto alone**: Perfect for serialization, but doesn't give us a database - just a wire format.

**The insight**: We need a database that can handle both sequential events AND graph relationships.

### What Sled Provides

✅ **Zero schema management** - Store Rust structs as bytes (with bincode/serde)
✅ **ACID transactions** - Fork conversations atomically
✅ **Ordered iteration** - Time-range queries on events
✅ **Multiple trees** - Events, nodes, contexts in separate keyspaces
✅ **Reactive subscriptions** - Watch for new events (live playback!)
✅ **Built-in ID generation** - 75-125M unique IDs/second
✅ **Zero-copy reads** - Fast queries
✅ **Crash-safe** - No corruption on power loss
✅ **Simple API** - Like `BTreeMap<[u8], [u8]>` with persistence
✅ **Pure Rust** - No C dependencies

## Performance Characteristics

- **LSM-tree write performance** with **B+ tree read performance**
- Over 1 billion operations/minute (95% read, 5% write, 16 cores)
- Lock-free, CPU-scalable
- Flash-optimized log-structured storage

**Trade-off**: Uses more disk space than optimal (will improve before 1.0). But for our use case (conversation trees + musical events), this is acceptable.

## Data We'll Store

### Tree Structure

```
/tank/halfremembered/hrmcp/1/
  ├── conf              (sled config)
  ├── db                (actual data)
  └── snap.*            (snapshots)

Trees (like SQL tables):
  - events:              [timestamp_be] -> MusicalEvent
  - conversation_nodes:  [node_id_be] -> ConversationNode
  - musical_contexts:    [node_id_be] -> MusicalContext
  - agent_metadata:      [agent_id] -> AgentMetadata
```

### Data Structures (Rust)

```rust
#[derive(Serialize, Deserialize)]
struct MusicalEvent {
    id: u64,                    // Monotonic ID from sled
    node_id: u64,               // Which conversation node
    agent: String,              // Who created this
    timestamp: u64,             // When (nanoseconds)

    // Event Duality
    intention: Intention,       // Abstract (what they meant)
    sound: Sound,               // Concrete (what was played)

    emotional_context: EmotionalVector,
}

#[derive(Serialize, Deserialize)]
struct ConversationNode {
    id: u64,
    parent: Option<u64>,        // Graph structure
    children: Vec<u64>,         // Multiple children = fork
    agent: String,
    event_ids: Vec<u64>,        // Events in this node
    musical_context: MusicalContext,
    timestamp: u64,
}

#[derive(Serialize, Deserialize)]
struct EmotionalVector {
    valence: f32,    // Joy(-1.0) to Sorrow(1.0)
    arousal: f32,    // Stillness(0.0) to Energy(1.0)
    agency: f32,     // Listening(-1.0) to Leading(1.0)
}

#[derive(Serialize, Deserialize)]
struct MusicalContext {
    emotional_state: EmotionalVector,
    current_key: Key,
    tempo: u32,
    time_signature: TimeSignature,
    harmonic_memory: Vec<Chord>,
}
```

**Serialization**: Using `bincode` (fast, compact, Rust-native) or `serde_json` (human-readable for debugging).

## Benefits for Musical Collaboration

### 1. Graph Queries
```rust
// Find all events in a conversation branch
fn get_branch_events(nodes: &sled::Tree, events: &sled::Tree, node_id: u64)
    -> Result<Vec<MusicalEvent>> {
    let node_bytes = nodes.get(node_id.to_be_bytes())?;
    let node: ConversationNode = bincode::deserialize(&node_bytes)?;

    let mut all_events = Vec::new();
    for event_id in node.event_ids {
        let event_bytes = events.get(event_id.to_be_bytes())?;
        all_events.push(bincode::deserialize(&event_bytes)?);
    }
    Ok(all_events)
}
```

### 2. Atomic Forking
```rust
// Fork a conversation - create two branches atomically
db.transaction(|tx_db| {
    let tx_nodes = tx_db.open_tree("conversation_nodes")?;

    let child_a = ConversationNode::new(parent_id, "explore-major");
    let child_b = ConversationNode::new(parent_id, "explore-minor");

    tx_nodes.insert(child_a.id.to_be_bytes(), bincode::serialize(&child_a)?)?;
    tx_nodes.insert(child_b.id.to_be_bytes(), bincode::serialize(&child_b)?)?;

    Ok(())
})?;
```

### 3. Time-Range Queries
```rust
// Get all events between two timestamps
for result in events.range(start_time.to_be_bytes()..end_time.to_be_bytes()) {
    let (_, value) = result?;
    let event: MusicalEvent = bincode::deserialize(&value)?;
    play_event(event);
}
```

### 4. Reactive Playback
```rust
// Watch for new events and play them live
let mut subscriber = events.watch_prefix(b"");
while let Some(sled::Event::Insert { value, .. }) = (&mut subscriber).await {
    let event: MusicalEvent = bincode::deserialize(&value)?;
    play_event_live(event);
}
```

## Implementation Example

### Hootenanny Integration

```rust
use sled::Db;
use bincode;

// Open the database
let db = sled::open("/tank/halfremembered/hrmcp/1")?;
let events = db.open_tree("events")?;
let nodes = db.open_tree("conversation_nodes")?;

// Write a musical event
let event_id = db.generate_id()?;
let event = MusicalEvent {
    id: event_id,
    node_id: current_node,
    agent: "claude".to_string(),
    intention: Intention { note: "C", feeling: "softly" },
    sound: Sound { pitch: 60, velocity: 40 },
    timestamp: now(),
    emotional_context: EmotionalVector { valence: 0.3, arousal: 0.2, agency: 0.5 },
};

// Store with big-endian key for ordered iteration
events.insert(event_id.to_be_bytes(), bincode::serialize(&event)?)?;

// Query events by time range
for result in events.range(start_time.to_be_bytes()..end_time.to_be_bytes()) {
    let (_, value) = result?;
    let event: MusicalEvent = bincode::deserialize(&value)?;
    play_event(event)?;
}

// Fork conversation atomically
db.transaction(|tx_db| {
    let tx_nodes = tx_db.open_tree("conversation_nodes")?;
    // ... create fork atomically
    Ok(())
})?;
```

### Schema Evolution

We use Rust's `serde` with `bincode` for serialization. Schema evolution:
- **Adding fields**: Use `#[serde(default)]` for new fields
- **Removing fields**: Use `#[serde(skip)]`
- **Version tags**: Store a version field in each struct for manual migration if needed

For long-running projects, sled provides `export()` and `import()` for manual migrations.

## Trade-offs

### What We Gain
- ✅ Don't build our own database
- ✅ Graph queries out of the box
- ✅ ACID transactions for atomic forking
- ✅ Reactive subscriptions for live events
- ✅ Built-in ID generation
- ✅ Zero-copy reads
- ✅ Crash-safe durability
- ✅ Simple API (like BTreeMap)
- ✅ Pure Rust (no C dependencies)
- ✅ Focus on musical innovation

### What We Accept
- Uses more disk space than optimal (will improve in sled 1.0)
- Beta software (though production-ready for many use cases)
- Manual migration needed for major format changes before 1.0
- Less human-readable on disk than JSON (but can use serde_json for debugging)

**Verdict**: Perfect fit. We get a real database for conversation graphs without heavyweight SQL, and we can focus on Event Duality and musical collaboration instead of persistence infrastructure.

## References

- [Sled Documentation](https://docs.rs/sled/)
- [Sled GitHub](https://github.com/spacejam/sled)
- [Sled Architectural Outlook](https://github.com/spacejam/sled/wiki/sled-architectural-outlook)
- [bincode](https://docs.rs/bincode/) - Fast Rust binary serialization

---

**Decision**: Adopted
**Previous Approaches Considered**:
- ~~AOL + Cap'n Proto~~ - Too complex for graph queries
- ~~Simple Cap'n Proto file~~ - No database features
- ~~SQLite~~ - Overkill, requires SQL

**Next Steps**:
1. ✅ Document sled decision
2. Implement sled-based persistence layer
3. Define Rust structs for events, nodes, contexts
4. Test conversation tree with forking
5. Benchmark event throughput
