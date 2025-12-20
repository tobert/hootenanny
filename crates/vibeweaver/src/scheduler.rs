//! Rule scheduler with SDN-inspired trigger matching

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;

use crate::db::Database;
use crate::session::{Action, Priority, Rule, RuleId, SessionId, Trigger};
use crate::zmq_client::Broadcast;

/// Trigger type for indexing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerType {
    Beat,
    Marker,
    Deadline,
    Artifact,
    JobComplete,
    Transport,
}

impl TriggerType {
    pub fn from_trigger(trigger: &Trigger) -> Self {
        match trigger {
            Trigger::Beat { .. } => TriggerType::Beat,
            Trigger::Marker { .. } => TriggerType::Marker,
            Trigger::Deadline { .. } => TriggerType::Deadline,
            Trigger::Artifact { .. } => TriggerType::Artifact,
            Trigger::JobComplete { .. } => TriggerType::JobComplete,
            Trigger::Transport { .. } => TriggerType::Transport,
        }
    }
}

/// Index for O(1) trigger type lookup
pub struct RuleIndex {
    by_trigger: HashMap<TriggerType, Vec<Rule>>,
}

impl RuleIndex {
    pub fn new() -> Self {
        Self {
            by_trigger: HashMap::new(),
        }
    }

    /// Add rule to index
    pub fn insert(&mut self, rule: Rule) {
        let trigger_type = TriggerType::from_trigger(&rule.trigger);
        let bucket = self.by_trigger.entry(trigger_type).or_default();

        // Insert sorted by priority
        let pos = bucket
            .iter()
            .position(|r| r.priority > rule.priority)
            .unwrap_or(bucket.len());
        bucket.insert(pos, rule);
    }

    /// Remove rule from index
    pub fn remove(&mut self, id: &RuleId) {
        for bucket in self.by_trigger.values_mut() {
            bucket.retain(|r| &r.id != id);
        }
    }

    /// Get rules matching a trigger type
    pub fn get(&self, trigger_type: TriggerType) -> &[Rule] {
        self.by_trigger
            .get(&trigger_type)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Rebuild index from rules
    pub fn rebuild(&mut self, rules: Vec<Rule>) {
        self.by_trigger.clear();
        for rule in rules {
            self.insert(rule);
        }
    }

    /// Clear all rules
    pub fn clear(&mut self) {
        self.by_trigger.clear();
    }
}

impl Default for RuleIndex {
    fn default() -> Self {
        Self::new()
    }
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
        other
            .start_by_beat
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

/// Generation timing statistics
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

impl Scheduler {
    /// Create scheduler for session
    pub fn new(db: Arc<Database>, session_id: SessionId) -> Result<Self> {
        Ok(Self {
            db,
            session_id,
            index: RuleIndex::new(),
            agenda: BinaryHeap::new(),
            generation_stats: HashMap::new(),
            current_tempo_bpm: 120.0,
        })
    }

    /// Load rules from database into index
    pub fn load_rules(&mut self) -> Result<()> {
        let rules = self.db.get_rules_by_session(&self.session_id)?;
        self.index.rebuild(rules);

        // Also load generation stats
        // TODO: Load from database

        Ok(())
    }

    /// Add a new rule
    pub fn add_rule(&mut self, rule: Rule) -> Result<RuleId> {
        let id = rule.id.clone();
        self.db.insert_rule(&rule)?;
        self.index.insert(rule);
        Ok(id)
    }

    /// Remove a rule
    pub fn remove_rule(&mut self, id: &RuleId) -> Result<()> {
        self.db.delete_rule(id)?;
        self.index.remove(id);
        Ok(())
    }

    /// Process a broadcast, return actions to execute
    pub fn process_broadcast(&mut self, broadcast: &Broadcast) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut rules_to_delete = Vec::new();

        match broadcast {
            Broadcast::BeatTick { beat, tempo_bpm } => {
                self.current_tempo_bpm = *tempo_bpm;

                for rule in self.index.get(TriggerType::Beat) {
                    if let Trigger::Beat { divisor } = &rule.trigger {
                        if self.match_beat(*beat, *divisor) {
                            actions.push(rule.action.clone());

                            // Update fired count
                            let _ = self.db.update_rule_fired(&rule.id, Utc::now());

                            if rule.one_shot {
                                rules_to_delete.push(rule.id.clone());
                            }
                        }
                    }
                }
            }
            Broadcast::MarkerReached { name, .. } => {
                for rule in self.index.get(TriggerType::Marker) {
                    if let Trigger::Marker { name: trigger_name } = &rule.trigger {
                        if self.match_marker(name, trigger_name) {
                            actions.push(rule.action.clone());

                            let _ = self.db.update_rule_fired(&rule.id, Utc::now());

                            if rule.one_shot {
                                rules_to_delete.push(rule.id.clone());
                            }
                        }
                    }
                }
            }
            Broadcast::ArtifactCreated { tags, .. } => {
                for rule in self.index.get(TriggerType::Artifact) {
                    if let Trigger::Artifact { tag } = &rule.trigger {
                        let matches = match tag {
                            Some(t) => tags.iter().any(|at| at == t),
                            None => true, // Match any artifact
                        };

                        if matches {
                            actions.push(rule.action.clone());

                            let _ = self.db.update_rule_fired(&rule.id, Utc::now());

                            if rule.one_shot {
                                rules_to_delete.push(rule.id.clone());
                            }
                        }
                    }
                }
            }
            Broadcast::JobStateChanged { job_id, state, .. } => {
                if state == "complete" || state == "failed" {
                    for rule in self.index.get(TriggerType::JobComplete) {
                        if let Trigger::JobComplete {
                            job_id: trigger_job_id,
                        } = &rule.trigger
                        {
                            if trigger_job_id == job_id {
                                actions.push(rule.action.clone());

                                let _ = self.db.update_rule_fired(&rule.id, Utc::now());

                                if rule.one_shot {
                                    rules_to_delete.push(rule.id.clone());
                                }
                            }
                        }
                    }
                }
            }
            Broadcast::TransportStateChanged { state, .. } => {
                for rule in self.index.get(TriggerType::Transport) {
                    if let Trigger::Transport {
                        state: trigger_state,
                    } = &rule.trigger
                    {
                        if trigger_state == state {
                            actions.push(rule.action.clone());

                            let _ = self.db.update_rule_fired(&rule.id, Utc::now());

                            if rule.one_shot {
                                rules_to_delete.push(rule.id.clone());
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Clean up one-shot rules
        for id in rules_to_delete {
            let _ = self.remove_rule(&id);
        }

        actions
    }

    /// Check deadlines against current position, return actions ready to dispatch
    pub fn check_deadlines(&mut self, position_beats: f64, gpu_busy: bool) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut rules_to_remove = Vec::new();

        // Check agenda for items ready to dispatch
        while let Some(pending) = self.agenda.peek() {
            if pending.start_by_beat <= position_beats {
                if !gpu_busy || pending.priority == Priority::Critical {
                    let pending = self.agenda.pop().unwrap();
                    actions.push(pending.action);
                    rules_to_remove.push(pending.rule_id);
                } else {
                    break; // GPU busy, wait for next check
                }
            } else {
                break; // Not yet time
            }
        }

        // Remove dispatched rules
        for id in rules_to_remove {
            self.index.remove(&id);
        }

        actions
    }

    /// Update generation stats after job completes
    pub fn record_generation_time(&mut self, space: &str, duration_ms: u64) -> Result<()> {
        let stats = self
            .generation_stats
            .entry(space.to_string())
            .or_insert(GenerationStats {
                space: space.to_string(),
                avg_duration_ms: duration_ms,
                sample_count: 0,
            });

        // Update running average
        let new_count = stats.sample_count + 1;
        stats.avg_duration_ms =
            (stats.avg_duration_ms * stats.sample_count + duration_ms) / new_count;
        stats.sample_count = new_count;

        // Persist to database
        self.db
            .update_generation_stats(space, stats.avg_duration_ms)?;

        Ok(())
    }

    /// Estimate duration for a space (for deadline scheduling)
    pub fn estimate_duration(&self, space: &str) -> u64 {
        self.generation_stats
            .get(space)
            .map(|s| s.avg_duration_ms)
            .unwrap_or(5000) // Default 5 seconds
    }

    /// Set current tempo (affects deadline timing)
    pub fn set_tempo(&mut self, bpm: f64) {
        self.current_tempo_bpm = bpm;
    }

    /// Match beat trigger
    fn match_beat(&self, beat: f64, divisor: u32) -> bool {
        if divisor == 0 {
            return false;
        }

        // Fire on beats divisible by divisor
        let beat_int = beat.floor() as u32;
        beat_int.is_multiple_of(divisor) && (beat - beat.floor()).abs() < 0.1
    }

    /// Match marker trigger
    fn match_marker(&self, name: &str, trigger_name: &str) -> bool {
        name == trigger_name
    }

    /// Calculate start_by_beat for deadline rules
    pub fn calculate_start_by(&self, deadline_beat: f64, space: &str, priority: Priority) -> f64 {
        let estimated_duration_ms = self.estimate_duration(space) as f64;

        // Convert ms to beats
        let beats_per_ms = self.current_tempo_bpm / 60_000.0;
        let estimated_beats = estimated_duration_ms * beats_per_ms;

        // Apply safety margin
        let safety_margin = priority.safety_margin();

        deadline_beat - (estimated_beats * safety_margin)
    }

    /// Add a deadline rule to the agenda
    pub fn schedule_deadline(&mut self, rule: Rule, space: &str) {
        if let Trigger::Deadline { beat: deadline } = &rule.trigger {
            let start_by = self.calculate_start_by(*deadline, space, rule.priority);

            self.agenda.push(PendingAction {
                rule_id: rule.id.clone(),
                action: rule.action.clone(),
                priority: rule.priority,
                deadline_beat: Some(*deadline),
                start_by_beat: start_by,
            });

            self.index.insert(rule);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_matching() {
        let db = Arc::new(Database::open_memory().unwrap());
        let session = db.create_session("test", None, 120.0).unwrap();
        let scheduler = Scheduler::new(db, session.id).unwrap();

        // Divisor 4 should match beats 0, 4, 8, 12...
        assert!(scheduler.match_beat(0.0, 4));
        assert!(!scheduler.match_beat(1.0, 4));
        assert!(!scheduler.match_beat(2.0, 4));
        assert!(!scheduler.match_beat(3.0, 4));
        assert!(scheduler.match_beat(4.0, 4));
        assert!(scheduler.match_beat(8.0, 4));

        // Handle near-integer beats
        assert!(scheduler.match_beat(4.05, 4));
        assert!(!scheduler.match_beat(4.5, 4));
    }

    #[test]
    fn test_rule_index() {
        let mut index = RuleIndex::new();

        let rule1 = Rule::new(SessionId::new(), Trigger::Beat { divisor: 4 }, Action::Play)
            .with_priority(Priority::Normal);

        let rule2 = Rule::new(
            SessionId::new(),
            Trigger::Beat { divisor: 8 },
            Action::Pause,
        )
        .with_priority(Priority::High);

        index.insert(rule1);
        index.insert(rule2);

        let beat_rules = index.get(TriggerType::Beat);
        assert_eq!(beat_rules.len(), 2);
        // High priority should come first
        assert_eq!(beat_rules[0].priority, Priority::High);
    }

    #[test]
    fn test_pending_action_ordering() {
        let mut heap = BinaryHeap::new();

        heap.push(PendingAction {
            rule_id: RuleId::new(),
            action: Action::Play,
            priority: Priority::Normal,
            deadline_beat: Some(100.0),
            start_by_beat: 90.0,
        });

        heap.push(PendingAction {
            rule_id: RuleId::new(),
            action: Action::Pause,
            priority: Priority::High,
            deadline_beat: Some(80.0),
            start_by_beat: 70.0,
        });

        // Earlier start_by should come out first
        let first = heap.pop().unwrap();
        assert_eq!(first.start_by_beat, 70.0);
    }
}
