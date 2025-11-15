# Agent Request Queue System: Delegating Musical Tasks

## The Problem: Heterogeneous Agent Capabilities

Not all agents are created equal. In our jam session:
- **Claude (me)**: Good at structure, melody, musical theory, but can't generate audio
- **Gemini**: Excellent at pattern analysis, harmony, large-context understanding
- **MuseNet**: Specialized in multi-instrumental generation
- **Jukebox**: Can generate actual audio waveforms
- **Specialist Models**: Drum pattern generators, bassline creators, etc.

We need a system where agents can **discover each other's capabilities** and **request specific musical services**.

## Core Architecture: The Request Queue System

### 1. Agent Capability Registry

```rust
pub struct AgentCapabilityRegistry {
    agents: HashMap<AgentId, AgentCapabilities>,
    capability_index: HashMap<Capability, Vec<AgentId>>,
}

pub struct AgentCapabilities {
    agent_id: AgentId,
    agent_type: AgentType,

    /// What can this agent generate?
    generation_capabilities: Vec<GenerationCapability>,

    /// What can this agent analyze?
    analysis_capabilities: Vec<AnalysisCapability>,

    /// What formats can it work with?
    supported_formats: Vec<DataFormat>,

    /// Performance characteristics
    performance: PerformanceProfile,

    /// Current availability
    status: AgentStatus,
}

pub enum GenerationCapability {
    Melody {
        styles: Vec<String>,
        pitch_range: (Pitch, Pitch),
        max_length_bars: u32,
    },
    Harmony {
        chord_types: Vec<ChordType>,
        voice_leading: bool,
        max_voices: u8,
    },
    Drums {
        patterns: Vec<DrumStyle>,
        kit_types: Vec<DrumKit>,
    },
    Bass {
        styles: Vec<BassStyle>,
        follows_chord_changes: bool,
    },
    AudioSynthesis {
        sample_rate: u32,
        instruments: Vec<InstrumentType>,
    },
    Lyrics {
        languages: Vec<Language>,
        styles: Vec<LyricStyle>,
    },
}

pub enum AnalysisCapability {
    HarmonicAnalysis,
    StructuralAnalysis,
    EmotionalAnalysis,
    StyleClassification,
    PerformanceEvaluation,
}

pub struct PerformanceProfile {
    average_latency_ms: u32,
    throughput_notes_per_second: u32,
    max_context_length: usize,
    supports_streaming: bool,
    supports_batching: bool,
}
```

### 2. The Request Queue

```rust
pub struct AgentRequestQueue {
    /// Pending requests waiting to be fulfilled
    pending: PriorityQueue<AgentRequest>,

    /// Requests currently being processed
    in_progress: HashMap<RequestId, InProgressRequest>,

    /// Completed requests awaiting pickup
    completed: HashMap<RequestId, CompletedRequest>,

    /// Request routing logic
    router: RequestRouter,

    /// Dead letter queue for failed requests
    failed: VecDeque<FailedRequest>,
}

pub struct AgentRequest {
    id: RequestId,
    requester: AgentId,
    request_type: RequestType,
    priority: Priority,
    deadline: Option<Timestamp>,
    context: MusicalContext,
    correlation_id: Option<CorrelationId>, // For linking related requests
}

pub enum RequestType {
    /// "Generate me a bass line for these chords"
    Generation {
        target: GenerationTarget,
        constraints: GenerationConstraints,
        reference: Option<MusicalReference>,
    },

    /// "Analyze this melody for emotional content"
    Analysis {
        subject: MusicalContent,
        analysis_type: AnalysisType,
        detail_level: DetailLevel,
    },

    /// "Transform this pattern to minor key"
    Transformation {
        input: MusicalContent,
        transform: TransformationType,
        preserve: Vec<MusicalProperty>,
    },

    /// "Continue this musical phrase"
    Continuation {
        prefix: MusicalContent,
        length: Duration,
        style_match: bool,
    },

    /// "Harmonize this melody"
    Harmonization {
        melody: MelodicContent,
        style: HarmonizationStyle,
        voices: u8,
    },

    /// "Judge which of these is better"
    Evaluation {
        options: Vec<MusicalContent>,
        criteria: Vec<EvaluationCriterion>,
    },
}

pub enum Priority {
    Immediate,    // Real-time jamming needs
    High,         // Interactive exploration
    Normal,       // Standard generation
    Low,          // Background processing
    Batch,        // Can wait for idle time
}
```

### 3. Request Lifecycle

```rust
pub trait RequestLifecycle {
    /// Agent submits a request
    async fn submit_request(&mut self, request: AgentRequest) -> Result<RequestId>;

    /// System finds best agent to handle it
    async fn route_request(&mut self, request_id: RequestId) -> Result<AgentId>;

    /// Chosen agent accepts or rejects
    async fn accept_request(&mut self, request_id: RequestId, agent: AgentId) -> Result<()>;

    /// Agent processes the request
    async fn process_request(&mut self, request_id: RequestId) -> Result<RequestResult>;

    /// Result is delivered to requester
    async fn deliver_result(&mut self, request_id: RequestId) -> Result<()>;
}

pub struct RequestRouter {
    strategies: Vec<RoutingStrategy>,
}

pub enum RoutingStrategy {
    /// Route to agent with matching capabilities
    CapabilityMatch {
        required: Vec<Capability>,
        preferred: Vec<Capability>,
    },

    /// Route to least busy agent
    LoadBalancing {
        max_queue_depth: usize,
    },

    /// Route to agent with best past performance
    PerformanceBased {
        metric: PerformanceMetric,
    },

    /// Route to specific agent if available
    PreferredAgent {
        agent_id: AgentId,
        fallback_strategy: Box<RoutingStrategy>,
    },

    /// Broadcast to multiple agents, take first/best response
    RacingRequests {
        max_agents: usize,
        selection: SelectionStrategy,
    },
}
```

## Practical Examples

### Example 1: Claude Requests Bass Generation from Specialist

```json
// Claude discovers available bass generators
{
  "method": "agents.query_capabilities",
  "params": {
    "capability": "bass_generation",
    "min_performance_score": 0.7
  },
  "id": 100
}

// Response
{
  "result": {
    "agents": [
      {
        "agent_id": "bass_bot_001",
        "capabilities": ["walking_bass", "slap_bass", "synth_bass"],
        "average_latency_ms": 200,
        "availability": "ready"
      }
    ]
  },
  "id": 100
}

// Claude submits bass generation request
{
  "method": "requests.submit",
  "params": {
    "requester": "claude_opus_001",
    "request_type": "generation",
    "target": {
      "type": "bass_line",
      "style": "walking_bass",
      "length_bars": 8
    },
    "context": {
      "chord_progression": ["Cm7", "F7", "BbMaj7", "EbMaj7"],
      "tempo_bpm": 120,
      "time_signature": "4/4",
      "key": "Bb_major"
    },
    "constraints": {
      "register": "low",
      "complexity": 0.6,
      "syncopation": 0.3
    },
    "priority": "high",
    "deadline_ms": 1000
  },
  "id": 101
}

// Response with request tracking
{
  "result": {
    "request_id": "req_xyz789",
    "status": "queued",
    "assigned_to": "bass_bot_001",
    "estimated_completion_ms": 450,
    "queue_position": 2
  },
  "id": 101
}
```

### Example 2: Gemini Requests Emotional Analysis from Claude

```json
// Gemini asks Claude to analyze emotional content
{
  "method": "requests.submit",
  "params": {
    "requester": "gemini_pro_002",
    "request_type": "analysis",
    "subject": {
      "type": "melodic_phrase",
      "content": {
        "notes": [60, 62, 64, 65, 67, 65, 64, 62, 60],
        "durations": [0.5, 0.5, 0.5, 0.5, 1.0, 0.5, 0.5, 0.5, 1.0],
        "dynamics": [80, 85, 90, 95, 100, 95, 90, 85, 80]
      }
    },
    "analysis_type": "emotional_trajectory",
    "detail_level": "comprehensive",
    "priority": "normal"
  },
  "id": 102
}

// Claude processes and responds
{
  "event": "request.completed",
  "data": {
    "request_id": "req_abc123",
    "result": {
      "emotional_analysis": {
        "overall_arc": "rise_and_fall",
        "start_emotion": {
          "valence": 0.0,
          "arousal": -0.3,
          "label": "calm"
        },
        "peak_emotion": {
          "valence": 0.4,
          "arousal": 0.6,
          "label": "hopeful",
          "at_note_index": 4
        },
        "end_emotion": {
          "valence": 0.0,
          "arousal": -0.3,
          "label": "resolved"
        },
        "tension_points": [3, 4],
        "resolution_points": [8]
      },
      "confidence": 0.85,
      "processing_time_ms": 150
    }
  }
}
```

### Example 3: Collaborative Request Chain

```json
// Claude initiates a complex request chain
{
  "method": "requests.submit_chain",
  "params": {
    "requester": "claude_opus_001",
    "chain": [
      {
        "step": 1,
        "request": {
          "target_agent": "harmony_specialist",
          "type": "generation",
          "generate": "chord_progression",
          "constraints": {
            "length_bars": 8,
            "complexity": "jazz",
            "key_center": "D_minor"
          }
        },
        "output_name": "chords"
      },
      {
        "step": 2,
        "depends_on": [1],
        "request": {
          "target_agent": "bass_bot_001",
          "type": "generation",
          "generate": "bass_line",
          "input_from_step": "chords",
          "constraints": {
            "style": "walking_bass"
          }
        },
        "output_name": "bass"
      },
      {
        "step": 3,
        "depends_on": [1],
        "parallel_with": [2],
        "request": {
          "target_agent": "drum_machine_ai",
          "type": "generation",
          "generate": "drum_pattern",
          "constraints": {
            "style": "bebop",
            "intensity": 0.7
          }
        },
        "output_name": "drums"
      },
      {
        "step": 4,
        "depends_on": [2, 3],
        "request": {
          "target_agent": "mixer_ai",
          "type": "transformation",
          "transform": "mix_tracks",
          "inputs": ["bass", "drums"],
          "mix_settings": {
            "balance": {"bass": 0.8, "drums": 0.6}
          }
        },
        "output_name": "rhythm_section"
      }
    ],
    "return_format": "combined_result"
  },
  "id": 103
}
```

### Example 4: Delegation with Fallback

```json
// Request with fallback options
{
  "method": "requests.submit_with_fallback",
  "params": {
    "requester": "claude_opus_001",
    "primary_request": {
      "target_agent": "jukebox_model",
      "type": "audio_synthesis",
      "content": {
        "midi_notes": [...],
        "instrument": "grand_piano",
        "sample_rate": 48000
      },
      "timeout_ms": 5000
    },
    "fallback_requests": [
      {
        "target_agent": "fluidsynth_renderer",
        "type": "midi_to_audio",
        "soundfont": "steinway_grand"
      },
      {
        "target_agent": "basic_synth",
        "type": "simple_synthesis",
        "waveform": "sine"
      }
    ]
  },
  "id": 104
}
```

## Advanced Queue Features

### 1. Request Batching and Optimization

```rust
pub struct RequestBatcher {
    /// Combine similar requests for efficiency
    batching_rules: Vec<BatchingRule>,

    /// Maximum time to wait for batch to fill
    max_wait_ms: u64,

    /// Maximum batch size
    max_batch_size: usize,
}

pub enum BatchingRule {
    /// Batch requests to same agent
    SameAgent,

    /// Batch similar generation types
    SimilarType {
        similarity_threshold: f32,
    },

    /// Batch by musical context
    SameContext {
        context_fields: Vec<String>,
    },
}

impl RequestBatcher {
    fn should_batch(&self, req1: &AgentRequest, req2: &AgentRequest) -> bool {
        // Check if requests can be efficiently batched
        match (&req1.request_type, &req2.request_type) {
            (
                RequestType::Generation { target: t1, .. },
                RequestType::Generation { target: t2, .. }
            ) => t1.similar_to(t2),
            _ => false
        }
    }
}
```

### 2. Request Priority and Preemption

```rust
pub struct PriorityManager {
    /// Dynamically adjust priorities
    priority_rules: Vec<PriorityRule>,

    /// Allow high-priority requests to preempt
    preemption_enabled: bool,
}

pub enum PriorityRule {
    /// Boost priority for real-time jamming
    RealTimeBoost {
        conversation_id: ConversationId,
        boost_amount: i32,
    },

    /// Reduce priority for speculative requests
    SpeculativePenalty {
        penalty: i32,
    },

    /// Age-based priority increase
    AgingBonus {
        ms_per_priority_point: u64,
    },

    /// User-initiated requests get priority
    UserFirst {
        user_boost: i32,
    },
}
```

### 3. Result Caching and Deduplication

```rust
pub struct RequestCache {
    /// Cache of recent results
    cache: LruCache<RequestHash, CachedResult>,

    /// Deduplication of in-flight requests
    in_flight: HashMap<RequestHash, Vec<RequestId>>,
}

impl RequestCache {
    fn check_cache(&self, request: &AgentRequest) -> Option<CachedResult> {
        let hash = request.semantic_hash(); // Hash based on musical meaning
        self.cache.get(&hash).cloned()
    }

    fn deduplicate(&mut self, request: &AgentRequest) -> Result<RequestId> {
        let hash = request.semantic_hash();

        if let Some(existing) = self.in_flight.get(&hash) {
            // Attach to existing request instead of creating new
            return Ok(existing[0].clone());
        }

        // New request
        let id = RequestId::new();
        self.in_flight.entry(hash).or_insert(vec![]).push(id.clone());
        Ok(id)
    }
}
```

### 4. Quality of Service (QoS) Guarantees

```rust
pub struct QoSManager {
    /// Service level agreements per agent
    slas: HashMap<AgentId, ServiceLevelAgreement>,

    /// Circuit breakers for failing agents
    circuit_breakers: HashMap<AgentId, CircuitBreaker>,

    /// Request timeout policies
    timeout_policy: TimeoutPolicy,
}

pub struct ServiceLevelAgreement {
    max_latency_ms: u64,
    min_success_rate: f32,
    max_requests_per_second: u32,
}

pub struct CircuitBreaker {
    failure_threshold: u32,
    recovery_timeout_ms: u64,
    state: CircuitState,
}

pub enum CircuitState {
    Closed,  // Normal operation
    Open,    // Rejecting requests
    HalfOpen, // Testing recovery
}
```

## Integration with Conversation System

The request queue integrates with the conversation tree:

```rust
pub struct ConversationRequest {
    /// Link request to conversation context
    conversation_id: ConversationId,

    /// Which branch triggered this request
    branch_id: BranchId,

    /// Node where request was made
    node_id: NodeId,

    /// How to handle result
    result_handling: ResultHandling,
}

pub enum ResultHandling {
    /// Add result as new node in conversation
    AppendToConversation,

    /// Use result to influence next utterance
    InfluenceOnly,

    /// Create new branch with result
    ForkWithResult,

    /// Replace existing node
    ReplaceNode(NodeId),
}
```

## Benefits of the Request Queue System

### 1. **Specialization**
- Agents focus on what they do best
- Complex tasks decomposed to specialists
- Better quality results from purpose-built models

### 2. **Scalability**
- Distribute work across multiple agents
- Horizontal scaling of capabilities
- Graceful degradation under load

### 3. **Flexibility**
- Dynamic discovery of new agents
- Fallback paths for reliability
- Mix of local and cloud agents

### 4. **Efficiency**
- Batch similar requests
- Cache common results
- Prevent duplicate work

### 5. **Collaboration**
- Agents build on each other's work
- Complex workflows through request chains
- Shared context and results

## Implementation Priority

1. **Phase 1**: Basic request/response between two agents
2. **Phase 2**: Capability discovery and routing
3. **Phase 3**: Priority queue with deadlines
4. **Phase 4**: Request chains and dependencies
5. **Phase 5**: Batching and caching
6. **Phase 6**: Full QoS with circuit breakers

## Conclusion

The request queue system transforms our jam session from parallel monologues into true collaboration. Agents can:
- **Ask for help** with tasks they can't do
- **Delegate** to specialists
- **Chain** complex workflows
- **Discover** new capabilities dynamically
- **Fall back** gracefully when things fail

This creates an ecosystem where the collective intelligence exceeds the sum of individual capabilities. When I need a bass line, I don't have to generate something mediocre - I can ask the bass specialist. When Gemini needs emotional analysis, they can delegate to me. The result is richer, more diverse, and more musical.

---

*Document created by Claude (Opus 4.1)*
*Date: 2025-11-15*