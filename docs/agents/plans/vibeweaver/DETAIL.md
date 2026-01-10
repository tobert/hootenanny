# Vibeweaver Design Rationale

**Purpose:** Deep context for revision sessions. Read when you need to understand *why*.

---

## Why "Vibeweaver"?

Weaving vibes together—ML generation, real-time input, scheduled events, latent potential. The name evokes the session concept: creative direction that persists and informs decisions.

---

## Why Embed Python in Rust?

Vibeweaver is a separate process (crash protection) with Python embedded via PyO3:

1. **Interpreter control** — Own the state, inject variables, hot-reload code
2. **Direct FFI** — Rust functions callable from Python, no serialization overhead
3. **Session ownership** — State lives in Rust (sqlite), Python has a view

The alternative (Python subprocess over ZMQ) adds latency and sync complexity.

---

## Why Python?

AI agents naturally express music-making in Python. Luanette exists but agents don't reach for it—there's mechanical sympathy with Python because of ML ecosystem training data.

---

## Core Types

### Session

```rust
struct Session {
    id: SessionId,
    name: String,
    vibe: Option<String>,      // Creative direction: "dark minimal techno"
    tempo_bpm: f64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

### Rule (unified: callbacks + latent jobs)

```rust
struct Rule {
    id: RuleId,
    session_id: SessionId,
    trigger: Trigger,
    action: Action,
    priority: Priority,        // Critical > High > Normal > Low > Idle
    enabled: bool,
    one_shot: bool,            // Delete after firing?
    fired_count: u64,
    last_fired_at: Option<DateTime<Utc>>,
}

enum Trigger {
    Beat { divisor: u32 },                    // Every N beats
    Marker { name: String },                  // Named position
    Deadline { beat: f64 },                   // Must complete by beat
    Artifact { tag: Option<String> },         // New artifact created
    JobComplete { job_id: JobId },            // Specific job done
    Transport { state: TransportState },      // Play/pause/stop
}

enum Action {
    Sample { space: String, prompt: String, inference: InferenceParams },
    Schedule { content: ContentRef, at: f64, duration: Option<f64>, gain: f64 },
    SampleAndSchedule { /* combines above */ },
    Play, Pause, Stop,
    Seek { beat: f64 },
    Audition { content: ContentRef, duration: f64 },
    Notify { message: String },
}

enum Priority { Critical, High, Normal, Low, Idle }
```

### Marker

```rust
struct Marker {
    id: MarkerId,
    session_id: SessionId,
    beat: f64,
    name: String,
    metadata: Option<serde_json::Value>,
}
```

### KernelState (the latent space)

The world Python sees when it wakes. Reconstructed from sqlite + broadcasts, snapshotted for fast restart.

```rust
struct KernelState {
    // From sqlite (persistent)
    session: Session,
    markers: Vec<Marker>,

    // From broadcasts (volatile, recoverable)
    transport: TransportState,
    beat: BeatState,
    jobs: HashMap<JobId, JobState>,
    recent_artifacts: Vec<ArtifactRef>,

    captured_at: DateTime<Utc>,
}

struct TransportState {
    state: Transport,  // Playing, Paused, Stopped
    position_beats: f64,
}

struct BeatState {
    current: f64,
    tempo_bpm: f64,
}

struct JobState {
    state: JobStatus,  // Pending, Running, Complete, Failed
    artifact_id: Option<ArtifactId>,
}

struct ArtifactRef {
    id: ArtifactId,
    content_hash: String,
    tags: Vec<String>,
}
```

**Hydration flow:**
1. Load session/markers from sqlite
2. Load latest snapshot (if exists) for jobs/artifacts
3. Subscribe to broadcasts, update state as they arrive
4. Python globals materialize from KernelState

---

## The Scheduler

Unified execution engine for all timed and reactive work.

### Why Unified?

Instead of Python callbacks (code, ephemeral), we use **scheduled rules** (data, persistent). Both latent jobs and reactive callbacks become trigger-action pairs that serialize to sqlite.

### Rule Matching (SDN-inspired)

Index by trigger type, scan within bucket:

```
TriggerType → Vec<Rule> (sorted by priority)
─────────────────────────────────────────────
Beat        → [rule_1, rule_5, ...]
Marker      → [rule_3, rule_8, ...]
Deadline    → [rule_2, rule_7, ...]
Artifact    → [rule_4, ...]
Transport   → [rule_6, ...]
```

On broadcast: O(1) lookup by type, O(n) scan within bucket, execute in priority order.

### Priority Scheduling

For deadline-triggered work:

```
start_by = deadline - estimated_duration - safety_margin
           where safety_margin = 1.5x for critical, 1.2x for high

When start_by <= current_position:
  → dispatch if GPU available, else queue
```

---

## The Broadcast Model

ZMQ broadcasts update vibeweaver state automatically:

| Broadcast | Effect |
|-----------|--------|
| `JobStateChanged` | Resolves awaited futures, updates job state |
| `ArtifactCreated` | Fires matching Artifact triggers |
| `TransportStateChanged` | Updates transport, fires Transport triggers |
| `BeatTick` | Fires matching Beat triggers |
| `MarkerReached` | Fires matching Marker triggers |

Between `weave_eval` calls, state is current—no polling needed.

---

## Async Patterns

**Fire and forget:** `schedule(kick, at=0)` — instant, no I/O

**Await completion:** `kick = await sample(...)` — returns when JobStateChanged arrives

**Parallel generation:** `a, b, c = await gather(sample(...), sample(...), sample(...))`

**Latent with deadline:** Creates a Deadline rule, scheduler decides when to compute

---

## Python API Surface

```python
# Session
session(name=None, vibe=None, load=None) -> Session
tempo(bpm: float)

# Generation
sample(space: str, prompt: str = None, inference: dict = None) -> Awaitable[Artifact]
latent(space: str, prompt: str, deadline: float, priority: str = "normal") -> LatentRef

# Timeline
schedule(content, at: float, duration: float = None, gain: float = 0.8)
audition(content, duration: float = None)

# Transport
play() / pause() / stop() / seek(beat: float)

# Reactive (creates rules)
@on_beat(divisor: int)
@on_marker(name: str)
@on_artifact(tag: str = None)

# State (read-only)
beat.current -> float
transport.state -> str
transport.position -> float
timeline.markers -> List[Marker]
```

---

## Sqlite Schema

```sql
-- Session metadata
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    vibe TEXT,
    tempo_bpm REAL NOT NULL DEFAULT 120.0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Scheduled rules (callbacks + latent jobs)
CREATE TABLE rules (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    trigger_type TEXT NOT NULL,
    trigger_params TEXT NOT NULL,
    action_type TEXT NOT NULL,
    action_params TEXT NOT NULL,
    priority TEXT NOT NULL DEFAULT 'normal',
    enabled INTEGER NOT NULL DEFAULT 1,
    one_shot INTEGER NOT NULL DEFAULT 0,
    fired_count INTEGER NOT NULL DEFAULT 0,
    last_fired_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_rules_session ON rules(session_id);
CREATE INDEX idx_rules_trigger ON rules(trigger_type, enabled);

-- Timeline markers
CREATE TABLE markers (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    beat REAL NOT NULL,
    name TEXT NOT NULL,
    metadata TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_markers_session_beat ON markers(session_id, beat);

-- History (for context restoration)
CREATE TABLE history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    action TEXT NOT NULL,
    params TEXT,
    result TEXT,
    success INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_history_session ON history(session_id, created_at DESC);

-- KernelState snapshots (Cap'n Proto, for fast restart)
CREATE TABLE kernel_snapshots (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    state_capnp BLOB NOT NULL,
    captured_at TEXT NOT NULL
);

-- Generation timing stats (for deadline estimation)
CREATE TABLE generation_stats (
    space TEXT PRIMARY KEY,
    avg_duration_ms INTEGER NOT NULL,
    sample_count INTEGER NOT NULL DEFAULT 1,
    last_updated TEXT NOT NULL
);
```

---

## Cap'n Proto Schema

```capnp
# hooteproto/schema/vibeweaver.capnp

struct KernelState {
    session @0 :Session;
    transport @1 :TransportState;
    beat @2 :BeatState;
    jobs @3 :List(JobEntry);
    recentArtifacts @4 :List(ArtifactRef);
    capturedAtNanos @5 :UInt64;
}

struct Session {
    id @0 :Text;
    name @1 :Text;
    vibe @2 :Text;
    tempoBpm @3 :Float64;
}

struct TransportState {
    state @0 :Text;
    positionBeats @1 :Float64;
}

struct BeatState {
    current @0 :Float64;
    tempo @1 :Float64;
}

struct JobEntry {
    id @0 :Text;
    state @1 :Text;
    artifactId @2 :Text;
}

struct ArtifactRef {
    id @0 :Text;
    contentHash @1 :Text;
    tags @2 :List(Text);
}
```

---

**What's NOT persisted:**
- Python globals (can't pickle functions)
- Timeline regions (live in chaosgarden)
- Full artifacts (live in hootenanny, referenced by ID)

---

## Context Restoration

After context compaction, `weave_session()` returns:

```
Session: warehouse_3am
Vibe: dark minimal techno, industrial
Tempo: 130 BPM

Active Rules: 3
  - Beat(divisor=32) → SampleAndSchedule (low priority)
  - Marker("drop") → Notify
  - Deadline(beat=256) → Sample (high priority, pending)

Markers: drop@256, release@288, outro@512

Recent History:
  - sample(orpheus_loops, "kick") → artifact_abc ✓
  - sample(orpheus_loops, "hats") → artifact_def ✓
  - schedule(artifact_abc, at=0) ✓
```

This is the context I need to resume coherently.

---

## Decisions Made

| Question | Decision |
|----------|----------|
| Jupyter kernel | No — build our own UI later |
| Concurrent sessions | Process-level isolation |
| Import sandbox | No restrictions for MVP |
| Hot reload | Yes |
| Async runtime | `pyo3-async-runtimes` |
| Callback persistence | Callbacks become rules (data) |

## Rejected Alternatives

| Alternative | Why Rejected |
|-------------|--------------|
| Python subprocess over ZMQ | Latency, session sync complexity |
| Pure Rust DSL | AI agents think in Python |
| Lua (luanette) | Agents don't reach for it naturally |
| Tool calls only | Too verbose, no composition |
| Pickle Python globals | Can't pickle functions, too fragile |
