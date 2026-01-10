# 04-scheduler: Rule Scheduler

**File:** `crates/vibeweaver/src/scheduler.rs`
**Dependencies:** 02-session, 03-zmq
**Unblocks:** 07-mcp

---

## Task

Create the trigger-action scheduler with SDN-inspired rule matching and priority queue for deadline-based work.

## Deliverables

- `crates/vibeweaver/src/scheduler.rs`
- Rule index by trigger type
- Priority queue for pending actions
- Unit tests

## Types

```rust
use std::collections::{HashMap, BinaryHeap};
use std::cmp::Ordering;
use std::sync::Arc;
use crate::db::{Database, Rule, RuleId, SessionId, Trigger, Action, Priority};
use crate::zmq::Broadcast;
use anyhow::Result;

/// Index for O(1) trigger type lookup
pub struct RuleIndex {
    /// Rules indexed by trigger type, sorted by priority within each bucket
    by_trigger: HashMap<TriggerType, Vec<Rule>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerType {
    Beat,
    Marker,
    Deadline,
    Artifact,
    JobComplete,
    Transport,
}

/// Pending action in the priority queue
#[derive(Debug, Clone)]
pub struct PendingAction {
    pub rule_id: RuleId,
    pub action: Action,
    pub priority: Priority,
    pub deadline_beat: Option<f64>,
    pub start_by_beat: f64,
}

impl Ord for PendingAction {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower start_by_beat = higher priority (process sooner)
        // Then by Priority enum (Critical < High < Normal < Low < Idle)
        other.start_by_beat
            .partial_cmp(&self.start_by_beat)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.priority.cmp(&other.priority))
    }
}

impl PartialOrd for PendingAction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for PendingAction {}

impl PartialEq for PendingAction {
    fn eq(&self, other: &Self) -> bool {
        self.rule_id == other.rule_id
    }
}

/// Generation timing statistics for deadline estimation
#[derive(Debug, Clone)]
pub struct GenerationStats {
    pub space: String,
    pub avg_duration_ms: u64,
    pub sample_count: u64,
}

/// The scheduler
pub struct Scheduler {
    db: Arc<Database>,
    session_id: SessionId,
    index: RuleIndex,
    agenda: BinaryHeap<PendingAction>,
    generation_stats: HashMap<String, GenerationStats>,
    current_tempo_bpm: f64,
}

impl RuleIndex {
    pub fn new() -> Self;

    /// Add rule to index
    pub fn insert(&mut self, rule: Rule);

    /// Remove rule from index
    pub fn remove(&mut self, id: &RuleId);

    /// Get rules matching a trigger type
    pub fn get(&self, trigger_type: TriggerType) -> &[Rule];

    /// Rebuild index from database
    pub fn rebuild(&mut self, rules: Vec<Rule>);
}

impl Scheduler {
    /// Create scheduler for session
    pub fn new(db: Arc<Database>, session_id: SessionId) -> Result<Self>;

    /// Load rules from database into index
    pub fn load_rules(&mut self) -> Result<()>;

    /// Add a new rule
    pub fn add_rule(&mut self, rule: Rule) -> Result<RuleId>;

    /// Remove a rule
    pub fn remove_rule(&mut self, id: &RuleId) -> Result<()>;

    /// Process a broadcast, return actions to execute
    pub fn process_broadcast(&mut self, broadcast: &Broadcast) -> Vec<Action>;

    /// Check deadlines against current position, return actions ready to dispatch
    pub fn check_deadlines(&mut self, position_beats: f64, gpu_busy: bool) -> Vec<Action>;

    /// Update generation stats after job completes
    pub fn record_generation_time(&mut self, space: &str, duration_ms: u64) -> Result<()>;

    /// Estimate duration for a space (for deadline scheduling)
    pub fn estimate_duration(&self, space: &str) -> u64;

    /// Set current tempo (affects deadline timing)
    pub fn set_tempo(&mut self, bpm: f64);
}

// --- Matching helpers ---

impl Scheduler {
    /// Match Beat trigger
    fn match_beat(&self, beat: f64, divisor: u32) -> bool;

    /// Match Marker trigger
    fn match_marker(&self, name: &str, trigger_name: &str) -> bool;

    /// Calculate start_by_beat for deadline rules
    fn calculate_start_by(&self, deadline_beat: f64, space: &str, priority: Priority) -> f64;
}
```

## Scheduling Algorithm

For deadline-triggered work:
```
start_by = deadline - estimated_duration - safety_margin
           where safety_margin = 1.5x for critical, 1.2x for high, 1.0x otherwise

When start_by <= current_position:
  → dispatch if GPU available, else queue
```

## Definition of Done

```bash
cargo fmt --check -p vibeweaver
cargo clippy -p vibeweaver -- -D warnings
cargo test -p vibeweaver scheduler::
```

## Acceptance Criteria

- [ ] Rules indexed by trigger type
- [ ] Beat divisor matching works (e.g., divisor=4 fires on beats 0, 4, 8...)
- [ ] Marker name matching (exact match)
- [ ] Deadline rules added to agenda with correct start_by
- [ ] Priority ordering in agenda
- [ ] One-shot rules removed after firing
