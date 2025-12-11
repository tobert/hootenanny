# Chaosgarden Design Rationale

**Purpose:** Deep design context for revision sessions. Read this when you need to understand *why* decisions were made, not *what* to build.

---

## Why Not a DAW?

DAWs are workstations: one human controlling everything through visual interfaces.
Chaosgarden is a performance space: multiple participants collaborating through shared abstractions.

| DAW Assumption | Hootenanny Reality |
|----------------|-------------------|
| One human operator | Humans, AI models, analog systems as peers |
| Visual interface primary | Queries, voice, text as equal interfaces |
| Tools serve the master | Participants coordinate as equals |
| Human handles complexity | Agents absorb complexity, humans focus on music |
| Static arrangement | Living graph of latents resolving into sound |

The graph isn't about DSP routing efficiency. It's about giving every participant—human, model, or machine—a shared representation to reason about and contribute to.

### The Hootenanny Model

We're not building a workstation with AI features. We're building a performance space where the boundaries between performer and tool dissolve.

**Participants, not roles:**
- A human might play keys, or approve generated content, or tune parameters
- An agent might generate melodies, or critique mix balance, or handle transitions
- An analog synth might contribute noise, or trigger events, or modulate others
- A classifier model might tag what it hears, or propose variations, or learn preferences

No hierarchy. No master. Capabilities differ, but participation is equal.

**The graph as shared consciousness:**
Every participant sees the same structure. Queries let anyone ask anything about the current state. Latent regions make intention visible before realization. The performance is legible to all.

**Complexity absorbed, not displayed:**
DAWs show complexity because humans must manage it. Hootenanny absorbs complexity into agents and policies. Humans see what matters: the music taking shape, the choices to make, the moments of creation.

---

## Why Four Primitives?

We tried many decompositions. Four survived:

| Primitive | Why It's Primitive |
|-----------|-------------------|
| **Time** | Everything else depends on time representation. Musical vs physical is a fundamental choice. |
| **Signal** | Typed data flow enables compile-time correctness. Audio/MIDI/Control/Trigger have different semantics. |
| **Node** | Uniform processing interface lets AI models and DSP share the same graph. |
| **Region** | Timeline behaviors need identity, position, duration. This is the minimal representation. |

**What we rejected:**
- "Clip" — too specific, implies audio. Region with PlayContent behavior is more general.
- "Track" — organizational, not primitive. Built from regions + routing.
- "Parameter" — subsumed by Control signals and node descriptors.

---

## Why Latent?

A region can be **resolved** (content exists in CAS) or **latent** (content is being generated).

Latent regions are placeholders with intent:
- "4 bars of jazz piano starting at beat 16" — we know what we want, not what it is yet
- Points to a running job, not a CAS hash
- Carries generation parameters, constraints, context

When the job completes, latent → resolved. The region now points to an artifact.

### Why This Matters

Traditional DAWs have no representation for "work in progress." You either have content or you don't. But generative workflows are continuous:

```
[latent] → forward() → [latent] → forward() → [resolved]
    ↓                      ↓
  job starts           job refines
```

Multiple latent regions can be resolving simultaneously. Some resolve quickly (local models), some take minutes (cloud inference), some never resolve (rejected by human, superseded by better option).

The graph is alive. Latents churn. Some crystallize into the performance. Others dissolve. This is the creative process made visible.

### UI Vision

Imagine a high-density display showing:
- All active graphs as interconnected nodes
- Latent regions pulsing, showing job progress
- Resolution events as crystallization—latent snaps into concrete waveform
- New latents spawning as agents propose ideas
- The whole system breathing: `forward()`, create, resolve, `forward()`...

Not a static arrangement view. A living process.

---

## How Generation Meets Playback

Generation and playback are concurrent processes connected by resolution:

```
┌─────────────────────────────────────────────────────────┐
│                  LATENT SPACE                           │
│                                                         │
│   ┌─────────┐  ┌─────────┐  ┌─────────┐               │
│   │ latent  │  │ latent  │  │ latent  │  ← Jobs run   │
│   │ ░░░░░░░ │  │ ▓▓▓░░░░ │  │ ▓▓▓▓▓▓▓ │    async     │
│   └────┬────┘  └────┬────┘  └────┬────┘               │
│        │            │         ┌──┘                     │
│        │            │         ▼ resolves!              │
└────────┼────────────┼─────────┼─────────────────────────┘
         │            │         │
         │            │    ┌────▼─────┐
         │            │    │ artifact │ → HITL approval?
         │            │    │ in CAS   │   agent decision?
         │            │    └────┬─────┘
         │            │         │ approved
┌────────┼────────────┼─────────┼─────────────────────────┐
│        │            │         ▼                         │
│   [ playing... ▶▶▶▶▶▶▶▶▶▶▶▶▶ mixing in ▶▶▶ ]          │
│                                                         │
│                  REALTIME PLAYBACK                      │
└─────────────────────────────────────────────────────────┘
```

The playback timeline sees resolved regions. Latent regions are visible but silent—potential futures, not yet actualized. When a latent resolves:

1. Artifact lands in CAS with full lineage
2. System notifies interested parties (agents, UI, HITL flow)
3. Someone decides: mix it in, hold it, discard it, request variation
4. If approved: schedule entry at suitable boundary, crossfade applied

No central coordinator decides. Participants coordinate through shared state and policies. The graph is the coordination mechanism.

### The Mixing-In Decision

When new content is ready, someone must decide: introduce it now, or wait?

This is a natural HITL moment:
- Present the artifact (waveform, MIDI piano roll, audio preview)
- Human auditions via headphones, approves or requests variation
- Approved content enters playback at next suitable boundary

For autonomous operation, agents can make these decisions using heuristics, classifiers, or learned preferences—but the human always has override capability.

**Crossfade strategies** (simple defaults, user choice):
- Hard cut at beat boundary
- Linear crossfade over N beats
- Generated transition (use `orpheus_bridge` to create musical connection)

Start simple. Add sophistication as we learn what works.

---

## Why Musical Time Primary?

DAWs traditionally use samples as ground truth, converting to beats for display. This causes drift and complexity.

**Our choice:** Beats are primary. Samples are derived via TempoMap.

**Implications:**
- A region at beat 4 stays at beat 4 regardless of tempo changes
- Tempo automation "just works" — it changes the TempoMap, not region positions
- Live tempo sync updates TempoMap, everything follows
- Sample-accurate rendering derives positions at render time

**Trade-off:** Quantization when converting beats→samples. We accept this; music is inherently quantized.

---

## Why Signals Are Typed?

Four signal types with different merge semantics:

| Type | Merge Behavior | Why |
|------|----------------|-----|
| Audio | Additive (sum) | Physics: sound waves superpose |
| MIDI | Event union (sorted) | Multiple instruments, one timeline |
| Control | Average | Competing automation should blend |
| Trigger | Union (sorted) | Events don't cancel each other |

**Alternative considered:** Untyped buffers with runtime checks. Rejected because:
- Compile-time errors are better than runtime errors
- Type information enables optimization (e.g., MIDI buffers are sparse)
- Schema for Trustfall queries needs types

---

## Why Nodes Are Uniform?

AI models (Orpheus, RAVE, Notochord) and DSP (gain, EQ, compressor) share the same `Node` trait.

**Why this matters:**
- Graph doesn't care what's inside a node
- Routing is declarative, not procedural
- Participants can reason about the graph without understanding node internals
- Latency compensation works uniformly

**External process pattern:** Most AI models run in Python. Chaosgarden nodes wrap IPC:
- RPC for offline generation (blocks until result, produces latent→resolved)
- Ring buffers for real-time streaming (non-blocking read)
- Async jobs for slow generation (latent regions track progress)

The graph sees uniform nodes. Communication complexity is encapsulated.

### Capabilities

Nodes declare what they can do:
- `realtime: bool` — can meet audio deadlines
- `offline: bool` — can block for extended processing
- `latency_samples: u64` — processing delay for PDC
- `signal_types: Vec<SignalType>` — what it accepts/produces

Participants query capabilities to understand what's possible. A human asking "can we run this live?" gets a clear answer from the graph.

> **Follow-up:** Design a registry/discovery system for capabilities. Participants should be able to advertise what they can do, discover what others offer, and compose workflows from available capabilities. This enables a true ensemble where new participants can join and contribute without central coordination.

---

## Why Regions Have Behaviors?

A region is position + duration + behavior. Behaviors include:
- **PlayContent** — play audio/MIDI from CAS
- **Latent** — generation in progress, displays intent and job status
- **ApplyProcessing** — modulate a parameter over time
- **EmitTrigger** — fire a discrete event

**Why not separate types?** We considered AudioRegion, MidiRegion, GenerativeRegion...

Rejected because:
- Regions transform: latent → resolved → playable
- Uniform querying: "all regions in chorus" shouldn't care about type
- Behaviors are extensible via Custom variant

---

## Why Content-Addressed Storage?

All content (audio, MIDI, generated) lives in CAS (content-addressed storage).

**Benefits:**
- **Lineage:** Know what generated what (artifact parent tracking)
- **Deduplication:** Same content = same hash = one copy
- **Reproducibility:** Same generation params = same hash = can verify
- **Caching:** Already have this hash? Don't regenerate

**Integration:** Chaosgarden uses `hootenanny` crate for CAS. Regions reference content by hash.

---

## Why Trustfall?

Trustfall isn't about optimizing known queries. It's about enabling questions we haven't thought of yet.

Today a participant might ask: "What regions are in the chorus?"
Tomorrow: "Show me a spectrogram of the loudest 4 bars"
Next month: "Which artifacts have similar harmonic content to this reference?"

The schema grows as we discover what participants need to perceive:
- **Classifiers** — genre, mood, energy, instrumentation
- **Visualizations** — spectrograms, waveforms, MIDI piano rolls
- **Heuristics** — density, complexity, tension scores
- **Relationships** — similarity, lineage, influence, contrast

By making everything queryable through one layer, we let experimentation drive capability—not upfront requirements.

### What Gets Exposed

The query layer exposes:
- Regions (by position, by tag, by resolution state, by latent/resolved)
- Nodes (by type, by capability, graph traversal)
- Time (conversions, tempo map queries)
- Artifacts (by lineage, by classifier tags, by creation time)
- Jobs (running latents, completion estimates, parameters)

Participants use queries to reason about the performance and decide what to do next.

---

## Cross-Cutting Concerns

### Error Handling in Nodes

Nodes can fail. Two error types:
- **Skipped** — transient (network timeout, buffer underrun). Output silence, try again.
- **Failed** — permanent (process died). Mark node failed, skip in future.

The render loop continues either way. No panics in the hot path.

### Latency Compensation (PDC)

Network AI models have variable latency. The graph compensates:
1. Each node reports latency (atomically updated for network nodes)
2. Background thread calculates compensation delays
3. Faster paths get delayed to match slowest path
4. Delay lines are pre-allocated (no hot path allocation)

### Buffer Management

**Hot path rule:** The render loop never allocates.

All buffers pre-allocated during graph compilation:
- Output buffers for each node
- Input gather structures
- Delay lines for PDC

This is non-negotiable for real-time audio.

### Multi-Graph Variants

Multiple compiled graphs can exist simultaneously (A/B comparison, undo stack).

Each is a "slot" with:
- Active/inactive state
- Gain (for crossfading)
- Fade rate

Crossfade between variants for smooth transitions.

---

## Integration Points

### With Hootenanny (via ZMQ)

Chaosgarden communicates with hootenanny via ZMQ sockets:
- **Shell channel**: Receives commands (CreateRegion, ResolveLatent, Play, Pause)
- **IOPub channel**: Publishes events (PlaybackPosition, LatentResolved)
- **Control channel**: Emergency stop, shutdown
- **Query channel**: Trustfall queries about graph state

Chaosgarden does NOT:
- Access CAS directly (hootenanny sends file paths)
- Dispatch jobs (hootenanny handles job dispatch to workers)
- Know about MCP (hrmcp talks to hootenanny, not chaosgarden)

### With Trustfall

The query layer exposes chaosgarden's state to Trustfall. Queries arrive via the Query ZMQ socket. See "Why Trustfall?" above.

### With PipeWire

External I/O uses PipeWire for:
- Hardware audio I/O
- Inter-app routing
- MIDI devices

Feature-gated (`pipewire` feature). Chaosgarden works without it (offline/file rendering only).

---

## Why URI-Namespaced Capabilities?

Participants need to discover what each other can do. We studied several capability systems:

| System | Pattern | Lesson |
|--------|---------|--------|
| **GStreamer Caps** | Negotiation via structured media types | Caps are composable; elements declare, pipeline negotiates |
| **LV2 Features** | RDF vocabulary for extensible metadata | URIs as namespaces - infinitely extensible |
| **CLAP Extensions** | Host/plugin capability handshake | Both sides declare; extensions can be stable or draft |
| **Vulkan Features** | Flat boolean struct + extension chaining | Core features fixed; extensions chain additional structs |
| **PipeWire Properties** | Key-value dictionary with conventions | Loose typing, namespaced string keys |

### Our Design Choices

**URI namespacing (like LV2):** Any participant can define new capabilities without central coordination. `gen:midi`, `hitl:approve`, `model:orpheus` — the colon-separated structure is a convention, not a hierarchy.

**Declaration over negotiation (unlike GStreamer):** Participants declare their capabilities; the system synthesizes alignment. This keeps participants simple. A model doesn't negotiate — it says what it can do, and workflow composition finds compatible participants.

**Flat URIs with inferred relationships:** `gen:midi:jazz` doesn't inherit from `gen:midi`. If we need relationships, we infer them from prefix matching. This keeps the type system simple and avoids ontology debates.

**Pull-based discovery:** Participants poll the registry for fresh information. No broadcast events. This scales better and avoids coordination complexity.

**Dynamic availability:** Capabilities can become available or unavailable at runtime. A GPU might fail, a model might be loading, a human might step away. The `available` flag on each capability reflects current state.

### What Capabilities Are Not

Capabilities are **not** permissions. Any participant can attempt any action. Capabilities are **advertisements** — "I can do this" — enabling workflow composition and discovery.

Capabilities are **not** hierarchical. There's no `gen` parent capability that `gen:midi` inherits from. Flat URIs with prefix matching is simpler and sufficient.

Capabilities are **not** versioned (yet). If we need `gen:midi:v2`, we'll add it, but capability semantics should be stable enough that versions are rarely needed.

### How Capabilities Integrate

The capability registry is the **coordination backbone**:

```
┌───────────────┐     registers as      ┌────────────────────┐
│   02-graph    │ ───────────────────▶  │                    │
│  (add_node)   │     Participant       │                    │
└───────────────┘                       │                    │
                                        │  CapabilityRegistry │
┌───────────────┐     queries for       │                    │
│   03-latent   │ ◀──────────────────── │                    │
│(find_providers)│    gen:* capabilities │                    │
└───────────────┘                       │                    │
                                        │                    │
┌───────────────┐     exposes via       │                    │
│   06-query    │ ◀──────────────────── │                    │
│  (Trustfall)  │     GraphQL schema    └────────────────────┘
└───────────────┘
```

When a Node is added to the Graph:
1. Graph calls `capability_registry.register(participant)` with node's `NodeCapabilities` converted to URIs
2. Later, LatentManager can query "who provides `gen:midi`?" to find available generators
3. Trustfall queries let any participant ask "what can X do?" or "who can do Y?"

This keeps coordination decentralized — no conductor, just shared state that everyone can query.

### Why Repair-First Identity?

Devices move. USB ports change. Network addresses shift. Rigid identity matching fails in the real world.

**The problem:**
- Eurorack unplugged Tuesday, plugged into different USB port Thursday
- MIDI controller moved from laptop to rack server
- Network synth got a new IP after DHCP renewal

**Our approach:** Identity hints, not identity keys.

`IdentityHints` is a bag of optional clues:
- `serial_number` — best signal when present, but many devices don't have one
- `usb_vendor_id` + `usb_product_id` — good for device type, not instance
- `user_label` — human-assigned name like "atobey's eurorack"
- `ipv4_address`, `ipv6_address` — for network devices

When a device appears, we don't demand exact match. We compute `match_score()` against existing participants and return:
- **Exact** — high confidence, auto-link
- **Candidates** — possible matches, human picks
- **NoMatch** — genuinely new device

The key is **easy repair**: when ambiguous, ask. Make asking cheap. The human says "yes, that's the same eurorack" and we update the hints with new path info.

Over a month-long project, devices will come and go. The identity system should make reconnection a 2-second interaction, not a mystery debugging session.

---

## Why Generational Tracking?

Over time, state accumulates: participants come and go, artifacts are generated and rejected, regions are superseded. Without grooming, queries slow down and storage fills.

### The Problem

| Component | What Grows | Without Grooming |
|-----------|-----------|------------------|
| CapabilityRegistry | Participants | Offline models linger forever |
| CAS | Artifacts | Rejected generations never cleaned |
| Regions | Timeline items | Failed/rejected regions clutter queries |
| Audit log | Decisions | Unbounded history |

### Our Approach: Generations + Tombstones

**Generations** are logical epochs. The system tracks a `current_generation: u64` that advances at session boundaries or major events. Every entity tracks:
- `created_generation` — when it was born
- `last_touched_generation` — when it was last actively used

**Tombstones** are soft deletes. Instead of removing an entity, we mark it tombstoned:
- `tombstoned_at: Option<DateTime<Utc>>`
- `tombstoned_generation: Option<Generation>`

Tombstoned entities are filtered from normal queries but remain accessible for:
- Debugging ("what happened to that model?")
- Recovery ("oops, un-tombstone that")
- Future pruning ("delete tombstoned items older than generation N")

**Permanent flag** prevents tombstoning. User can mark important items as `permanent: true` — they're immune to grooming.

### Lifecycle State Machine

```
                    touch()
    ┌─────────────────────────────────────┐
    │                                     │
    ▼                                     │
┌───────┐                           ┌─────┴─────┐
│ Alive │ ────── tombstone() ─────▶ │Tombstoned │
└───────┘                           └───────────┘
    │                                     │
    │ set_permanent(true)                 │ prune() [future]
    ▼                                     ▼
┌───────────┐                       ┌─────────┐
│ Permanent │                       │ Removed │
└───────────┘                       └─────────┘
```

### What We're NOT Building Yet

- **Automatic grooming** — on-demand only for now
- **Pruning** — tombstone is sufficient, actual deletion later
- **Compaction** — summarizing old data (e.g., decision logs) is future work
- **Session/project scoping** — left fuzzy intentionally

### Why This Design?

1. **Filtering is cheap** — `WHERE NOT tombstoned` is trivial
2. **Recovery is possible** — soft deletes are forgiving
3. **Generations enable time-travel** — "show me state as of generation N"
4. **Permanent flag gives control** — users pin what matters
5. **Prune can wait** — we'll add it when the system gets slow

---

## Why Scripts as Graphs?

We initially considered formalizing latent dependencies with a DAG or a "Generative Graph" structure (e.g., Region A must resolve before Region B starts). We rejected this in favor of **"Code as Graph"**.

### The Insight

A Lua script *is* a dependency graph.

```lua
local drums = generate("drums")             -- Node A
local bass  = generate("bass", {in=drums})  -- Node B (depends on A)
```

The variable passing defines the dependency. The runtime enforces the order.

### Why This Wins

1.  **Simplifies the Primitive:** `Behavior::Latent` handles a Job. That job can be a Lua script. The primitive doesn't need to know about dependency management.
2.  **LLM Resonance:** LLMs are excellent at writing code (scripts) but struggle with constructing complex, syntactically correct JSON DAGs with UUID references.
3.  **Flexibility:** Scripts enable control flow (loops, conditionals), retries, and parallel execution logic that static graphs cannot express easily.
4.  **Traceability:** Distributed tracing (OTLP) visualizes the script execution as the "graph" in real-time.
5.  **Verifiability:** Code is testable. We can run integration tests on the fly (e.g., "does this script produce valid MIDI?") before committing the result to the timeline.

This leverages the existing `luanette` crate, turning "orchestration" into "scripting," which aligns with the project's philosophy.

---

## Why ZeroMQ? Why Separate Daemons?

### The Realization

Chaosgarden started as a library. Then we asked: "What if the orchestrator crashes? Does audio stop?" The answer should be no.

**Autonomous systems need process isolation.** A performance engine that dies when its controller hiccups isn't autonomous—it's fragile.

### Jupyter as Precedent

Jupyter kernels have run this architecture for a decade:
- **5 ZMQ sockets** with distinct roles (Control, Shell, IOPub, Stdin, Heartbeat)
- **Process isolation** — kernel crash doesn't kill frontend
- **Language agnostic** — any process can speak the protocol
- **Bidirectional async** — requests, replies, and broadcasts

We adapt this for audio:

| Jupyter | Chaosgarden |
|---------|-------------|
| Shell (execute code) | Shell (create region, resolve latent, set tempo) |
| IOPub (outputs) | IOPub (LatentResolved, PlaybackPosition) |
| Control (interrupt) | Control (stop, pause, shutdown) |
| Stdin (input request) | Query (Trustfall queries) |
| Heartbeat | Heartbeat |

### System Topology

```
┌─────────────────────────────────────────────────────────────────────┐
│                              hrmcp                                   │
│  (MCP proxy — translates HTTP/SSE to ZMQ, thin glue)                │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ ZMQ
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│                           HOOTENANNY                                 │
│  (control plane — CAS, jobs, luanette, worker registry)             │
└───────────────────────────────┬─────────────────────────────────────┘
        │                       │                       │
        │ ZMQ                   │ ZMQ                   │ ZMQ
        ▼                       ▼                       ▼
┌───────────────┐      ┌───────────────┐      ┌───────────────┐
│  CHAOSGARDEN  │      │   Worker 1    │      │   Worker N    │
│  (RT audio)   │      │   (GPU)       │      │   (GPU)       │
└───────────────┘      └───────────────┘      └───────────────┘
```

**Component roles:**
- **hrmcp** — MCP-to-ZMQ proxy. Thin. Stateless. External interface.
- **hootenanny** — Control plane. CAS, jobs, scripts (luanette merged), worker registry.
- **chaosgarden** — RT audio. Playback, graph, PipeWire, timeline.
- **workers** — GPU inference. Connect to hootenanny, pull jobs.

**ZMQ is the universal internal protocol.** MCP is just one external interface via hrmcp.

### Why This Wins

1. **Process isolation** — GPU worker crash doesn't kill audio
2. **Horizontal scaling** — add GPUs by starting workers
3. **Multi-machine** — tcp:// instead of ipc://
4. **RT priority** — only chaosgarden needs it
5. **Crash recovery** — restart components independently
6. **Testability** — mock any component via ZMQ

### Worker Pool Pattern

Workers connect to hootenanny, announce capabilities, pull jobs:

```
Worker → Hootenanny: "I'm worker_abc, I can do [orpheus_generate, rave_encode], RTX 4090"
Hootenanny: registers worker in pool

Later:
Hootenanny → Job Queue: PUSH job {tool: "orpheus_generate", params: {...}}
Worker (idle) ← Job Queue: PULL job
Worker: runs model, writes artifact to CAS
Worker → Hootenanny: PUB result {job_id, artifact_id, content_hash}
Hootenanny → Chaosgarden: ShellRequest::ResolveLatent { region_id, artifact_id }
```

This is PUSH/PULL for fair job distribution — idle workers grab work.

---

## Open Questions

_Record unresolved design questions here._

| Question | Context | Status |
|----------|---------|--------|
| Capability registry/discovery | How do participants advertise and discover capabilities? | **Designed** (08-capabilities) |
| Latent dependency chains | Can one latent depend on another's resolution? | **Designed** (Lua scripts via `luanette`) |
| ZMQ architecture | How do daemons communicate? | **Designed** (Jupyter-inspired 5-socket protocol) |
| Worker pool | How do GPU workers register and receive jobs? | **Designed** (PUSH/PULL via hootenanny) |
| Voice/text HITL interface | How does approval flow work with speech? | Future |

---

## Why No Lua in Chaosgarden?

We considered embedding Lua (via `mlua`) in chaosgarden for low-latency script execution. We rejected this.

### The Trade-off

| Approach | Latency | RT Safety | Complexity |
|----------|---------|-----------|------------|
| Lua in chaosgarden | ~0ms | ❌ GC pauses, allocations | Two Lua runtimes |
| Dispatch to hootenanny | ~1-5ms | ✓ Pure Rust RT path | Single Lua runtime |

### Why Always Dispatch

**RT safety is non-negotiable.** Lua's garbage collector can pause unpredictably. Even with careful tuning, we'd be fighting the runtime instead of trusting it. The whole point of separating chaosgarden is RT isolation.

**Scripts run at musical time, not sample rate.** Generation jobs take seconds. Human decisions take longer than ZMQ round-trip. The 1-5ms dispatch latency is inaudible and irrelevant at beat boundaries.

**Single Lua runtime is simpler.** Hootenanny already has luanette. Scripts can access CAS, dispatch jobs, query workers directly. No need to sync state between two runtimes.

**Crash isolation.** A bad script crashes hootenanny, not audio playback. The show continues.

### What About Hot-Path Automation?

For frame-rate parameter changes:

1. **Pre-baked curves** — `ApplyProcessing` regions with compiled `Vec<CurvePoint>` (already in design)
2. **ZMQ parameter updates** — Hootenanny sends `SetParameter` at 10-100Hz, plenty for musical automation
3. **Future: WASM** — If we ever need custom RT logic, sandboxed WASM with no allocations is safer than Lua

**Decision: Chaosgarden stays pure Rust. Lua lives in hootenanny only.**

---

## Rejected Alternatives

_Record alternatives we considered and rejected._

| Alternative | Why Rejected |
|-------------|--------------|
| Lua embedded in chaosgarden | RT safety risk (GC pauses); single runtime in hootenanny is simpler |
| Sample-based time primary | Drift, complexity with tempo changes |
| Untyped signal buffers | Runtime errors, no optimization |
| Separate region types | Transformation complexity, querying pain |
| Unified offline/realtime graph mode | Conceptually muddled; generation and playback are different activities connected by resolution |
| Central "conductor" component | Implies hierarchy; coordination should emerge from shared state and participant policies |
| DAW-style visual-first interface | Assumes single human operator; we want multiple participants with diverse interfaces |
| GStreamer-style capability negotiation | Too complex; participants should be simple, declare only |
| Hierarchical capability URIs (inheritance) | Ontology debates, complexity; flat URIs with prefix matching suffices |
| Push-based capability broadcast | Coordination complexity; pull-based polling is simpler and scales |
| Capability versioning by default | Premature; capability semantics should be stable, add versions if needed |
| Explicit dependency graphs for latents | Complex JSON DAGs are brittle; Lua scripts naturally handle dependencies via variable passing |
