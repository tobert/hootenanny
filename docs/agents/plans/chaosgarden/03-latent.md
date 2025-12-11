# 03: Latent Lifecycle

**File:** `src/latent.rs`
**Focus:** Latent region lifecycle, state tracking, HITL approval, mixing-in
**Dependencies:** `primitives`, receives state updates via ZMQ from hootenanny

---

## Task

Create `crates/chaosgarden/src/latent.rs` with LatentManager that handles the lifecycle: state tracking, progress updates, approval decisions, and mixing-in to playback.

**Why this matters:** Latent regions are the async primitive—intent visible before realization. This is what makes generative workflows continuous rather than batch. The creative process made visible.

**Key architectural note:** Chaosgarden does NOT dispatch jobs. Hootenanny owns job dispatch to GPU workers. Chaosgarden receives state updates via ZMQ Shell commands (`UpdateLatentProgress`, `ResolveLatent`) and tracks the state for playback decisions.

**Deliverables:**
1. `latent.rs` with LatentManager and lifecycle state tracking
2. Shell command handlers for latent state updates from hootenanny
3. HITL approval flow (events published on IOPub)
4. Mixing-in strategies for introducing resolved content
5. Tests using mock ZMQ messages

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ Job dispatch — hootenanny handles this, we just track state
- ❌ Voice/text HITL interface — future work
- ❌ Playback engine — task 04
- ❌ Complex crossfade algorithms — start simple
- ❌ Latent-to-latent dependencies — handled by Lua scripts in hootenanny (see below)

Focus ONLY on the latent state tracking and event flow within chaosgarden.

---

## Multi-Step Generation: Scripts as Graphs

**Q: Can one latent depend on another's resolution?**

**A:** Yes, but not via explicit dependency fields. Use Lua scripts via `luanette`.

A script *is* a dependency graph:

```lua
local drums = generate("drums")              -- Job A
local bass  = generate("bass", {context=drums})  -- Job B (depends on A)
local mix   = combine(drums, bass)           -- Job C (depends on A and B)
return mix
```

Variable passing defines dependencies. The runtime enforces order. OTLP tracing visualizes it.

**Why this approach:**
- Keeps `Behavior::Latent` simple (just tracks one job)
- LLMs write scripts better than complex JSON DAGs
- Scripts enable control flow, retries, parallelism
- Testable before committing to timeline

See DETAIL.md "Why Scripts as Graphs?" for full rationale.

---

## The Latent Lifecycle

```
┌──────────┐     submit      ┌─────────┐
│ Pending  │ ───────────────▶│ Running │
└──────────┘                 └────┬────┘
                                  │
                    job completes │
                                  ▼
                            ┌──────────┐
                            │ Resolved │ ◀─── artifact in CAS
                            └────┬────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                  │
              ▼                  ▼                  ▼
        ┌──────────┐      ┌──────────┐      ┌──────────┐
        │ Approved │      │ Rejected │      │  Failed  │
        └────┬─────┘      └──────────┘      └──────────┘
             │
             │ mixing-in
             ▼
      ┌─────────────┐
      │  Playback   │
      └─────────────┘
```

**Key insight:** Resolution is continuous, not a phase. Multiple latents churn simultaneously. Some crystallize into the performance. Others dissolve.

---

## Types

```rust
/// Event emitted when latent state changes
#[derive(Debug, Clone)]
pub enum LatentEvent {
    /// Job submitted to generation system
    JobStarted {
        region_id: Uuid,
        job_id: String,
    },
    /// Progress update from job
    Progress {
        region_id: Uuid,
        progress: f32,
    },
    /// Generation complete, artifact ready
    Resolved {
        region_id: Uuid,
        artifact_id: String,
        content_hash: String,
        content_type: ContentType,
    },
    /// Content approved for playback
    Approved {
        region_id: Uuid,
    },
    /// Content rejected by human/agent
    Rejected {
        region_id: Uuid,
        reason: Option<String>,
    },
    /// Generation failed
    Failed {
        region_id: Uuid,
        error: String,
    },
    /// Content mixed into playback
    MixedIn {
        region_id: Uuid,
        at_beat: Beat,
        strategy: MixInStrategy,
    },
}

/// How to introduce resolved content into playback
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum MixInStrategy {
    /// Hard cut at beat boundary
    #[default]
    HardCut,
    /// Linear crossfade over N beats
    Crossfade { beats: f64 },
    /// Wait for generated musical transition
    Bridge { bridge_job_id: Option<String> },
}

/// Configuration for the latent manager
pub struct LatentConfig {
    /// Auto-approve from these tools (skip HITL)
    pub auto_approve_tools: HashSet<String>,
    /// Default mix-in strategy
    pub default_mix_in: MixInStrategy,
    /// Maximum concurrent generation jobs
    pub max_concurrent_jobs: usize,
}

/// Manages latent region lifecycle within chaosgarden
/// NOTE: Does NOT dispatch jobs - receives state updates from hootenanny via ZMQ
pub struct LatentManager {
    config: LatentConfig,
    iopub_publisher: Arc<dyn IOPubPublisher>,  // to publish events
    pending_approvals: HashMap<Uuid, PendingApproval>,
}

/// Artifact awaiting approval decision
#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub region_id: Uuid,
    pub artifact_id: String,
    pub content_hash: String,
    pub content_type: ContentType,
    pub resolved_at: Instant,
    pub generated_by: Option<Uuid>,  // Participant ID who generated this
}

/// Records an approval/rejection decision for audit trail
#[derive(Debug, Clone)]
pub struct ApprovalDecision {
    pub region_id: Uuid,
    pub decided_by: Uuid,           // Participant ID who made decision
    pub decision: Decision,
    pub decided_at: Instant,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Rejected,
    VariationRequested,
}

// NOTE: Chaosgarden does NOT dispatch jobs. These types represent state
// updates received from hootenanny via ZMQ Shell commands.

#[derive(Debug, Clone)]
pub enum JobStatus {
    Running { progress: f32 },
    Complete { artifact_id: String, content_hash: String, content_type: ContentType },
    Failed { error: String },
}
```

---

## LatentManager Methods

```rust
impl LatentManager {
    /// Create with configuration and IOPub publisher
    pub fn new(
        config: LatentConfig,
        iopub_publisher: Arc<dyn IOPubPublisher>,
    ) -> Self;

    // === State update handlers (called when ZMQ messages arrive from hootenanny) ===

    /// Handle job started notification from hootenanny
    pub fn handle_job_started(&mut self, region_id: Uuid, job_id: String, regions: &mut [Region]);

    /// Handle progress update from hootenanny
    pub fn handle_progress(&mut self, region_id: Uuid, progress: f32, regions: &mut [Region]);

    /// Handle job completion from hootenanny
    pub fn handle_resolved(
        &mut self,
        region_id: Uuid,
        artifact_id: String,
        content_hash: String,
        content_type: ContentType,
        regions: &mut [Region],
    );

    /// Handle job failure from hootenanny
    pub fn handle_failed(&mut self, region_id: Uuid, error: String, regions: &mut [Region]);

    /// Get regions awaiting approval
    pub fn pending_approvals(&self) -> Vec<&PendingApproval>;

    /// Approve a resolved region for playback
    /// `decided_by` is the Participant ID (human or agent) making the decision
    pub fn approve(
        &self,
        region_id: Uuid,
        decided_by: Uuid,
        regions: &mut [Region],
    ) -> Result<LatentEvent>;

    /// Reject a resolved region
    /// `decided_by` is the Participant ID making the decision
    pub fn reject(
        &self,
        region_id: Uuid,
        decided_by: Uuid,
        reason: Option<String>,
        regions: &mut [Region],
    ) -> Result<LatentEvent>;

    /// Request variation (reject + create new latent with adjusted params)
    /// `decided_by` is the Participant ID requesting the variation
    pub fn request_variation(
        &self,
        region_id: Uuid,
        decided_by: Uuid,
        param_adjustments: serde_json::Value,
        regions: &mut [Region],
    ) -> Result<(LatentEvent, Uuid)>;

    /// Schedule mixing-in of approved content
    pub fn schedule_mix_in(
        &self,
        region_id: Uuid,
        at_beat: Beat,
        strategy: MixInStrategy,
    ) -> Result<MixInSchedule>;

    /// Cancel a running generation job
    pub fn cancel(&self, region_id: Uuid, regions: &mut [Region]) -> Result<()>;
}
```

---

## HITL Approval Flow

The approval flow is event-driven, not blocking:

1. **Resolved event** → UI/agent receives notification
2. **Audition** → Human previews artifact (separate from main playback)
3. **Decision** → `approve()`, `reject()`, or `request_variation()`
4. **Mix-in** → If approved, schedule when to introduce

```rust
/// Integration point for HITL interfaces
pub trait ApprovalHandler: Send + Sync {
    /// Called when content is ready for approval
    fn on_resolved(&self, approval: &PendingApproval);

    /// Called to present audition interface
    fn request_audition(&self, artifact_id: &str) -> Result<()>;
}
```

**Future:** Voice interface where human says "sounds good" or "try again with more energy" and agent parses intent.

---

## Mixing-In Strategies

When approved content enters playback:

### HardCut (default)
- Wait for next beat boundary
- Swap content instantly
- Simplest, works for most cases

### Crossfade
- Overlap old and new content
- Linear gain ramp over N beats
- Good for smooth transitions

### Bridge
- Use `orpheus_bridge` to generate musical transition
- Creates new latent for the bridge
- Bridge resolves → then mix in original content
- Most sophisticated, requires additional generation

```rust
#[derive(Debug, Clone)]
pub struct MixInSchedule {
    pub region_id: Uuid,
    pub target_beat: Beat,
    pub strategy: MixInStrategy,
    pub bridge_latent: Option<Uuid>,  // if using Bridge strategy
}
```

---

## Integration with Playback

LatentManager produces `MixInSchedule` items. Playback engine consumes them:

```rust
/// Playback queries this to know when to introduce content
pub trait MixInSource {
    fn pending_mix_ins(&self) -> Vec<MixInSchedule>;
    fn acknowledge_mix_in(&self, region_id: Uuid);
}
```

The playback engine (task 04) checks `pending_mix_ins()` each frame and applies them at the scheduled beat.

---

## Concurrent Generation

Multiple latents can generate simultaneously:

```rust
impl LatentManager {
    /// How many jobs are currently running
    pub fn active_job_count(&self) -> usize;

    /// Can we submit more jobs?
    pub fn can_submit(&self) -> bool {
        self.active_job_count() < self.config.max_concurrent_jobs
    }

    /// Submit up to N pending regions (respects max_concurrent)
    pub fn submit_batch(&self, regions: &mut [Region], max: usize) -> Result<Vec<String>>;
}
```

---

## Quality Filtering (Optional)

Simple filters can auto-reject poor generations:

```rust
pub mod filters {
    /// Reject if duration differs too much from requested
    pub fn duration_tolerance(tolerance_beats: f64)
        -> Box<dyn Fn(&ResolvedContent, &Region) -> FilterDecision>;

    /// Reject if classifier scores below threshold
    pub fn min_classifier_score(classifier: &str, min: f64)
        -> Box<dyn Fn(&ResolvedContent, &Region) -> FilterDecision>;
}

pub enum FilterDecision {
    Accept,
    Reject { reason: String },
    RequestVariation { adjustments: serde_json::Value },
}
```

Filters run automatically before HITL approval. Failed filters can auto-retry.

---

## Event Flow Example

```
1. Region created: Latent { tool: "orpheus_generate", status: Pending }
2. submit() called → job dispatched → status: Running, job_id set
3. poll_jobs() → progress updates → LatentEvent::Progress emitted
4. poll_jobs() → job complete → status: Resolved, LatentEvent::Resolved emitted
5. UI shows artifact for audition
6. Human approves → approve() → status: Approved, LatentEvent::Approved emitted
7. schedule_mix_in() → MixInSchedule created
8. Playback engine introduces content at scheduled beat
9. LatentEvent::MixedIn emitted
```

---

## Acceptance Criteria

- [ ] `submit()` dispatches job and updates region state
- [ ] `poll_jobs()` tracks progress and detects completion
- [ ] Resolved regions appear in `pending_approvals()`
- [ ] `approve()` / `reject()` transitions state correctly
- [ ] `request_variation()` creates new latent with adjusted params
- [ ] `schedule_mix_in()` produces valid schedule
- [ ] Events emitted for all state transitions
- [ ] Auto-approve works for configured tools
- [ ] `max_concurrent_jobs` is respected
