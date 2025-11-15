# HalfRemembered MCP: System Dynamics and Interaction Model

## Overview

This document explores the dynamic behaviors and interactions within the HalfRemembered MCP system. While the conceptual domain model defines our musical entities, this analysis focuses on how these entities live, transform, and collaborate in real-time. We examine state management, concurrency patterns, and the delicate dance between human creativity and AI assistance, all while maintaining the strict performance guarantees required for real-time audio.

---

## 1. The Lifecycle of a Musical Event

### Conceptual Journey

A musical event's lifecycle represents a transformation pipeline from abstract intention to concrete sound. Each stage adds context, applies constraints, and prepares the event for its ultimate realization as audio.

### Stage Analysis

#### 1.1 Origination

Events can emerge from three primary sources, each with distinct characteristics:

**Human Input:**
- **MIDI Keyboard:** Raw MIDI messages (note-on, velocity, channel) arrive with hardware timestamps
- **Mouse/Touch:** Graphical input requires conversion from screen coordinates to musical parameters
- **Text Commands:** Semantic descriptions ("play a C major arpeggio") need parsing and interpretation
- **Characteristics:** Low latency requirements, immediate feedback expected, often imprecise timing

**AI Generation:**
- **Pattern-Based:** Expanding templates with variation parameters
- **Constraint-Based:** Solving musical puzzles within given rules
- **ML-Based:** Neural network outputs requiring quantization and validation
- **Characteristics:** Can generate in batches, timing is calculated rather than performed, perfect precision

**Pattern Expansion:**
- **Template Instantiation:** Converting abstract patterns to concrete events
- **Parameter Binding:** Applying pattern variables (velocity curves, timing shuffle)
- **Recursive Expansion:** Patterns containing patterns, requiring stack-based evaluation
- **Characteristics:** Deterministic (usually), can be pre-computed, requires context resolution

**Data Structure at Origin:**
```rust
// Conceptual representation
enum EventSource {
    HumanInput {
        device_id: DeviceId,
        timestamp: HardwareTimestamp,
        latency_compensation: Duration,
    },
    AIAgent {
        agent_id: AgentId,
        generation_context: GenerationContext,
        confidence: f32,
    },
    PatternExpansion {
        pattern_id: PatternId,
        expansion_params: HashMap<String, Value>,
        parent_pattern: Option<PatternId>,
    },
}
```

#### 1.2 Placement & Context

The event must find its position in both musical and absolute time:

**Temporal Anchoring:**
- **Musical Time:** Position as bars:beats:ticks relative to tempo map
- **Absolute Time:** Milliseconds from timeline start for sample-accurate playback
- **Relative Time:** Offset from pattern start or previous event
- **Conversion Function:** `musical_to_absolute(bar, beat, tick, tempo_map) -> milliseconds`

**Track Assignment:**
- **Direct Placement:** Event explicitly targets a track
- **Rule-Based Routing:** Events routed based on instrument type or pitch range
- **Load Balancing:** Distributing polyphonic events across available voices
- **Conflict Resolution:** Handling overlapping notes on monophonic tracks

**Context Inheritance:**
```rust
// The event inherits context from its placement
struct PlacedEvent {
    event: Event,
    track_context: TrackContext,  // volume, pan, effects
    timeline_context: TimelineContext,  // tempo, time_signature
    harmonic_context: Option<HarmonicContext>,  // current key, chord
}
```

#### 1.3 Transformation Pipeline

Events flow through a series of transformations, each potentially modifying the event:

**Quantization:**
- **Grid Snapping:** Aligning timing to nearest subdivision
- **Strength Parameter:** 0% (no quantization) to 100% (perfect grid)
- **Swing/Shuffle:** Systematic timing alterations for groove
- **Preserves:** Original timing stored for "undo" capability

**Scale/Key Filtering:**
- **Constraint Application:** Snapping pitches to nearest scale degree
- **Chromatic Permissions:** Allowing specific "outside" notes
- **Modal Interchange:** Borrowing from parallel scales
- **Preserves:** Original pitch for tension/resolution analysis

**Humanization:**
- **Timing Variance:** Adding micro-timing deviations (±10-50ms)
- **Velocity Variation:** Natural dynamics within phrases
- **Pitch Variation:** Subtle detuning for organic feel
- **Correlation:** Related events (chords) maintain relationships

**Effect Application:**
- **Track Effects:** Delay might create echo events
- **Pattern Effects:** Arpeggiator expands chords to sequences
- **Global Effects:** Transpose all events by interval
- **Side Effects:** New events created, not just modifications

**Transformation Order Matters:**
```
Original Event
    → Quantization (timing adjustment)
    → Scale Filtering (pitch adjustment)  
    → Humanization (subtle variations)
    → Effect Processing (potentially generating new events)
    → Final Event(s)
```

#### 1.4 Instrument Interpretation

The instrument receives the transformed event and converts it to synthesis parameters:

**Message Translation:**
- **Note Events:** Convert to oscillator frequency, envelope trigger
- **Control Events:** Map to synthesis parameters (filter cutoff, LFO rate)
- **Expression Events:** Modify ongoing notes (pitch bend, aftertouch)
- **Program Changes:** Load different presets or wavetables

**Voice Allocation:**
- **Monophonic:** New note cuts off previous (with optional glide)
- **Polyphonic:** Assign to available voice, steal oldest if needed
- **Multitimbral:** Route to appropriate timbre based on channel/program
- **Round-Robin:** Cycle through sample variations for realism

**Parameter Mapping:**
```rust
// The instrument interprets events according to its architecture
trait InstrumentVoice {
    fn trigger_note(&mut self, pitch: Pitch, velocity: Velocity) -> VoiceId;
    fn release_note(&mut self, voice_id: VoiceId);
    fn modulate(&mut self, voice_id: VoiceId, param: Parameter, value: f32);
    fn render(&mut self, buffer: &mut [f32], sample_rate: u32);
}
```

#### 1.5 Feedback Loop

The system's current state influences future event generation:

**State Observation:**
- **Harmonic Analysis:** Current chord and key detection
- **Rhythmic Analysis:** Detecting pattern and groove
- **Dynamic Analysis:** Overall volume and energy level
- **Polyphonic Analysis:** Voice leading and counterpoint

**AI Decision Making:**
- **Continuation:** Extending existing musical phrases
- **Response:** Call-and-response with human input
- **Variation:** Creating related but different material
- **Contrast:** Intentionally breaking patterns

**Feedback Latency:**
- **Immediate:** React to individual notes (echo, harmonization)
- **Phrase-Level:** Wait for complete phrase before responding
- **Section-Level:** Analyze larger structures before contributing
- **Adaptive:** Adjust response time based on musical context

---

## 2. State Persistence and Granularity

### Storage Architecture Strategies

#### 2.1 Monolithic Approach

**Single-Key Storage:**
```rust
// Entire project as one serialized blob
struct Project {
    timeline: Timeline,
    tracks: Vec<Track>,
    instruments: Vec<Instrument>,
    patterns: Vec<Pattern>,
}
// Stored as: key="project:id" → value=bincode::serialize(&project)
```

**Advantages:**
- **Simplicity:** One read to load entire project
- **Consistency:** Atomic updates, no partial states
- **Compression:** Better compression ratios on large blob
- **Versioning:** Easy to snapshot entire state

**Disadvantages:**
- **Memory:** Must load entire project even for small edits
- **Concurrency:** Lock entire project for any change
- **Network:** Transmit full state for collaboration
- **Scalability:** Large projects become unwieldy

#### 2.2 Granular Approach

**Multi-Key Storage:**
```rust
// Decomposed storage with reference IDs
// Timeline metadata
key="timeline:id" → TimelineMetadata

// Individual tracks  
key="track:timeline_id:track_id" → Track

// Events batched by time window
key="events:track_id:window_id" → Vec<Event>

// Shared patterns
key="pattern:id" → Pattern

// Instrument presets
key="instrument:id" → Instrument
```

**Advantages:**
- **Selective Loading:** Load only needed tracks
- **Fine-Grained Locking:** Lock individual tracks for editing
- **Incremental Updates:** Transmit only changes
- **Caching:** Keep frequently used patterns in memory
- **Scalability:** Handles large projects efficiently

**Disadvantages:**
- **Complexity:** Managing references between entities
- **Consistency:** Ensuring referential integrity
- **Transaction Overhead:** Multiple reads/writes per operation
- **Fragmentation:** Many small keys in database

#### 2.3 Hybrid Approach (Recommended)

**Chunked Storage with Intelligent Boundaries:**
```rust
// Core project structure (small, frequently accessed)
key="project:id:core" → ProjectCore {
    timeline_metadata,
    track_list,
    tempo_map,
}

// Track chunks (medium, independently editable)
key="track:id:chunk:n" → TrackChunk {
    events: Vec<Event>,  // 16-bar chunks
    automation: Vec<Automation>,
}

// Shared resources (large, cached)
key="resources:patterns" → PatternLibrary
key="resources:instruments" → InstrumentLibrary

// Collaboration layer (small, high-frequency updates)
key="collab:track_id:suggestions" → Vec<Suggestion>
```

**Optimization Strategies:**
- **Lazy Loading:** Load track chunks as timeline scrolls
- **Write-Ahead Log:** Queue changes before persisting
- **Bloom Filters:** Quickly check pattern usage without loading
- **Reference Counting:** Track shared resource usage

### Transaction Patterns

**Optimistic Concurrency Control:**
```rust
// Each entity has a version number
struct VersionedTrack {
    version: u64,
    track: Track,
}

// Update with version check
fn update_track(id: TrackId, update: TrackUpdate) -> Result<(), ConflictError> {
    let (current_version, track) = load_track(id)?;
    let new_track = apply_update(track, update);
    compare_and_swap(id, current_version, new_track)?;
    Ok(())
}
```

---

## 3. Modeling the Human-AI Collaboration

### 3.1 Suggestion Layer Architecture

**Parallel Reality Model:**
```rust
// Suggestions exist in parallel to committed state
struct TrackWithSuggestions {
    committed: Track,  // The "real" track
    suggestions: Vec<Suggestion>,
    active_preview: Option<SuggestionId>,  // Currently auditioning
}

struct Suggestion {
    id: SuggestionId,
    source: AgentId,
    content: SuggestionContent,
    confidence: f32,
    created_at: Timestamp,
    state: SuggestionState,
}

enum SuggestionState {
    Proposed,        // Initial state
    Previewing,      // Being auditioned
    Accepted,        // Merged into committed
    Rejected,        // Explicitly declined
    Expired,         // Timed out
}

enum SuggestionContent {
    ReplaceEvents { range: TimeRange, events: Vec<Event> },
    InsertEvents { position: TimePosition, events: Vec<Event> },
    TransformEvents { range: TimeRange, transform: Transform },
    CompletePhrase { after: TimePosition, completion: Melody },
}
```

**Interaction Flow:**
1. AI generates suggestion based on context
2. Suggestion added to track's suggestion layer
3. Human can preview (hear without committing)
4. Human accepts (merges) or rejects
5. Accepted suggestions become part of history

### 3.2 Attribution and Provenance

**Event Genealogy:**
```rust
// Rich provenance tracking
struct EventProvenance {
    original_source: Source,
    transformations: Vec<Transformation>,
    contributors: Vec<ContributorAction>,
}

struct Transformation {
    timestamp: Timestamp,
    actor: Actor,
    operation: Operation,
    parameters: HashMap<String, Value>,
}

enum Actor {
    Human { user_id: UserId },
    Agent { agent_id: AgentId, model: String },
    System { process: String },
}

enum Operation {
    Transpose { interval: Interval },
    Quantize { strength: f32 },
    Humanize { amount: f32 },
    Generate { algorithm: String },
    Edit { field: String, old_value: Value, new_value: Value },
}

// Example provenance chain:
// 1. Human plays melody on keyboard
// 2. AI-A suggests harmonic accompaniment
// 3. Human accepts but transposes up a fifth
// 4. AI-B adds rhythmic variations
// 5. System quantizes to grid
```

**Credit Assignment:**
- Each transformation preserves previous attribution
- Relative contribution weights can be calculated
- UI can show contribution visualization
- Supports "blame" view for debugging

### 3.3 Agency and Permissions

**Role-Based Track Control:**
```rust
struct TrackPermissions {
    owner: Actor,
    permissions: HashMap<Actor, Permission>,
}

bitflags! {
    struct Permission: u32 {
        const VIEW = 0b00000001;
        const SUGGEST = 0b00000010;
        const EDIT = 0b00000100;
        const DELETE = 0b00001000;
        const CHANGE_PERMISSIONS = 0b00010000;
    }
}

// Example scenarios:
// - Human owns melody track, AI can only suggest
// - AI owns generative drums, human can override
// - Shared ownership for collaborative sections
```

**Delegation Patterns:**
```rust
enum DelegationMode {
    // Human maintains full control
    Supervised { approval_required: bool },
    
    // AI has autonomy within boundaries  
    Bounded { constraints: Constraints },
    
    // AI generates, human curates
    Generative { human_filter: FilterCriteria },
    
    // Full AI autonomy
    Autonomous { safety_checks: Vec<SafetyCheck> },
}
```

### 3.4 Collaborative Primitives

**Conflict Resolution:**
```rust
enum ConflictResolution {
    // Last write wins
    LastWriteWins,
    
    // Explicit priority ordering
    PriorityBased { priority: HashMap<Actor, u32> },
    
    // Attempt automatic merge
    ThreeWayMerge { base: State, theirs: State, mine: State },
    
    // Queue for human decision
    HumanArbitration { options: Vec<State> },
}
```

**Collaborative Operations:**
- **Branching:** Create alternate versions of sections
- **Merging:** Combine contributions from multiple sources
- **Voting:** Multiple agents vote on alternatives
- **Consensus:** Require agreement before proceeding

---

## 4. Real-time Safety and Performance Constraints

### 4.1 Data Access Patterns

**Real-time Thread Requirements:**
```rust
// MUST be accessible without blocking
struct RealTimeData {
    // Immutable during playback
    event_buffer: Arc<[Event]>,  // Pre-sorted, pre-transformed
    
    // Lock-free updates
    playhead: AtomicU64,
    transport_state: AtomicTransportState,
    
    // Wait-free reads
    tempo_map: Arc<TempoMap>,  // Immutable snapshot
    
    // Pre-calculated
    samples_per_tick: f32,
    current_bpm: f32,
}

// NEVER access from RT thread
struct NonRealTimeData {
    // Requires allocation
    patterns: HashMap<PatternId, Pattern>,
    
    // Requires locks
    track_settings: Mutex<TrackSettings>,
    
    // I/O operations
    sample_library: SampleLoader,
    
    // Complex calculations
    harmonic_analyzer: Analyzer,
}
```

### 4.2 Ownership and Concurrency Strategy

**Triple-Buffer Pattern for Updates:**
```rust
// Three buffers: RT reading, UI writing, swap buffer
struct TripleBuffer<T> {
    buffers: [T; 3],
    read_idx: AtomicU8,   // RT thread reads
    write_idx: AtomicU8,  // UI thread writes
    swap_idx: AtomicU8,   // Ready to swap
}

impl<T> TripleBuffer<T> {
    fn read(&self) -> &T {
        &self.buffers[self.read_idx.load(Ordering::Acquire)]
    }
    
    fn write(&mut self) -> &mut T {
        &mut self.buffers[self.write_idx.load(Ordering::Acquire)]
    }
    
    fn swap(&self) {
        // Lock-free swap of indices
    }
}
```

**Ring Buffer for Events:**
```rust
// Lock-free SPSC queue for event communication
struct EventRingBuffer {
    buffer: Box<[MaybeUninit<Event>; CAPACITY]>,
    read_pos: CachePadded<AtomicUsize>,
    write_pos: CachePadded<AtomicUsize>,
}

impl EventRingBuffer {
    fn push(&self, event: Event) -> Result<(), Event> {
        // Wait-free write if space available
    }
    
    fn pop(&self) -> Option<Event> {
        // Wait-free read if data available
    }
}
```

**Message Passing for Complex Operations:**
```rust
// Non-RT thread sends commands, RT thread sends responses
enum AudioCommand {
    LoadInstrument { id: InstrumentId, preset: Preset },
    UpdateEffect { track: TrackId, effect: Effect },
    SeekTo { position: TimePosition },
}

enum AudioResponse {
    InstrumentLoaded { id: InstrumentId },
    PositionUpdate { position: TimePosition },
    ClippingDetected { track: TrackId },
}
```

### 4.3 Flattened Timeline Representation

**Pre-computed Event Schedule:**
```rust
// Before playback, flatten complex structure to simple list
struct FlattenedTimeline {
    // All events sorted by timestamp, pre-transformed
    events: Vec<ScheduledEvent>,
    
    // Index for binary search by time
    time_index: Vec<(Timestamp, usize)>,
    
    // Next event per track for voice allocation
    next_event_per_track: [Option<usize>; MAX_TRACKS],
}

struct ScheduledEvent {
    timestamp: u64,  // Samples from start
    track_id: u8,    // Compact representation
    event_data: CompactEvent,  // Minimized for cache
}

// 16 bytes total for cache line efficiency
struct CompactEvent {
    note: u8,
    velocity: u8,
    duration: u16,  // In samples
    instrument_id: u8,
    flags: u8,      // Note-on, note-off, etc.
    _padding: [u8; 2],
}
```

### 4.4 Performance Optimization Strategies

**Memory Layout:**
```rust
// Structure of Arrays for SIMD processing
struct EventBatch {
    timestamps: Vec<f32>,    // Aligned for SIMD
    pitches: Vec<f32>,       // Aligned for SIMD
    velocities: Vec<f32>,    // Aligned for SIMD
    durations: Vec<f32>,     // Aligned for SIMD
}

// Process 4 events at once with SIMD
fn process_events_simd(batch: &EventBatch) {
    // Use std::simd for vectorized operations
}
```

**Allocation Strategies:**
```rust
// Pre-allocate all buffers
struct AudioContext {
    // Object pools
    event_pool: Pool<Event>,
    buffer_pool: Pool<Vec<f32>>,
    
    // Scratch buffers
    temp_buffer: [f32; MAX_BLOCK_SIZE],
    
    // Never allocate in audio callback
    #[cfg(debug_assertions)]
    allocation_detector: AllocationDetector,
}
```

**Priority Inversion Prevention:**
```rust
// RT thread never waits on non-RT thread
struct AudioThread {
    // Only try-lock, never block
    settings: Arc<RwLock<Settings>>,
    
    fn update_settings(&mut self) {
        if let Ok(settings) = self.settings.try_read() {
            // Apply settings
        } else {
            // Use cached settings
        }
    }
}
```

---

## System-Wide Design Implications

### Architectural Principles

1. **Separation of Concerns:** Clear boundaries between real-time and non-real-time code
2. **Immutability Where Possible:** Reduce synchronization needs
3. **Message Passing Over Shared State:** Avoid lock contention
4. **Pre-computation:** Do complex work before real-time processing
5. **Graceful Degradation:** System remains musical even under load

### Implementation Priorities

1. **Correctness First:** Get the model right before optimizing
2. **Measure Everything:** Profile before making performance assumptions
3. **Progressive Enhancement:** Start simple, add complexity gradually
4. **Test Under Load:** Simulate worst-case scenarios
5. **Document Invariants:** Make assumptions explicit in code

### Future Considerations

- **Distributed Collaboration:** Multiple MCP instances coordinating
- **Machine Learning Integration:** Training on user interactions
- **Visual Programming:** Node-based composition interface
- **Live Coding:** Real-time code evaluation for music
- **Hardware Acceleration:** GPU/DSP for synthesis

---

## Conclusion

This system dynamics model provides a framework for implementing the HalfRemembered MCP with proper consideration for real-time constraints, collaborative workflows, and musical expressiveness. The key insight is that we need multiple representations of our domain model:

1. **Conceptual Model:** For human understanding and AI reasoning
2. **Persistent Model:** For storage and collaboration
3. **Runtime Model:** For real-time audio processing

By maintaining clear transformations between these representations and respecting the boundaries of each domain, we can build a system that is both musically expressive and technically robust.
