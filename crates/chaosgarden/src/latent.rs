//! Latent region lifecycle management
//!
//! Handles state tracking, progress updates, HITL approval, and mixing-in
//! for latent regions. Receives state updates from hootenanny via ZMQ.
//!
//! **Key architectural note:** Chaosgarden does NOT dispatch jobs. Hootenanny
//! owns job dispatch to GPU workers. We receive state updates and track them
//! for playback decisions.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::primitives::{Beat, ContentType, LatentStatus, Region, ResolvedContent};

/// Event emitted when latent state changes
#[derive(Debug, Clone)]
pub enum LatentEvent {
    JobStarted {
        region_id: Uuid,
        job_id: String,
    },
    Progress {
        region_id: Uuid,
        progress: f32,
    },
    Resolved {
        region_id: Uuid,
        artifact_id: String,
        content_hash: String,
        content_type: ContentType,
    },
    Approved {
        region_id: Uuid,
    },
    Rejected {
        region_id: Uuid,
        reason: Option<String>,
    },
    Failed {
        region_id: Uuid,
        error: String,
    },
    MixedIn {
        region_id: Uuid,
        at_beat: Beat,
        strategy: MixInStrategy,
    },
}

/// How to introduce resolved content into playback
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum MixInStrategy {
    #[default]
    HardCut,
    Crossfade {
        beats: f64,
    },
}

/// Artifact awaiting approval decision
#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub region_id: Uuid,
    pub artifact_id: String,
    pub content_hash: String,
    pub content_type: ContentType,
    pub resolved_at: Instant,
    pub generated_by: Option<Uuid>,
}

/// Records an approval/rejection decision for audit trail
#[derive(Debug, Clone)]
pub struct ApprovalDecision {
    pub region_id: Uuid,
    pub decided_by: Uuid,
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

/// Schedule for mixing in approved content
#[derive(Debug, Clone)]
pub struct MixInSchedule {
    pub region_id: Uuid,
    pub target_beat: Beat,
    pub strategy: MixInStrategy,
}

/// Trait for publishing events to IOPub channel
pub trait IOPubPublisher: Send + Sync {
    fn publish(&self, event: LatentEvent);
}

/// Configuration for the latent manager
#[derive(Debug, Clone)]
pub struct LatentConfig {
    pub auto_approve_tools: HashSet<String>,
    pub default_mix_in: MixInStrategy,
    pub max_concurrent_jobs: usize,
}

impl Default for LatentConfig {
    fn default() -> Self {
        Self {
            auto_approve_tools: HashSet::new(),
            default_mix_in: MixInStrategy::HardCut,
            max_concurrent_jobs: 4,
        }
    }
}

/// Manages latent region lifecycle within chaosgarden
///
/// Does NOT dispatch jobs - receives state updates from hootenanny via ZMQ
pub struct LatentManager {
    config: LatentConfig,
    publisher: Arc<dyn IOPubPublisher>,
    pending_approvals: HashMap<Uuid, PendingApproval>,
    mix_in_queue: Vec<MixInSchedule>,
    active_jobs: HashSet<Uuid>,
    decision_log: Vec<ApprovalDecision>,
}

impl LatentManager {
    pub fn new(config: LatentConfig, publisher: Arc<dyn IOPubPublisher>) -> Self {
        Self {
            config,
            publisher,
            pending_approvals: HashMap::new(),
            mix_in_queue: Vec::new(),
            active_jobs: HashSet::new(),
            decision_log: Vec::new(),
        }
    }

    /// Handle job started notification from hootenanny
    pub fn handle_job_started(&mut self, region_id: Uuid, job_id: String, regions: &mut [Region]) {
        if let Some(region) = regions.iter_mut().find(|r| r.id == region_id) {
            region.start_job(job_id.clone());
            self.active_jobs.insert(region_id);

            self.publisher
                .publish(LatentEvent::JobStarted { region_id, job_id });
        }
    }

    /// Handle progress update from hootenanny
    pub fn handle_progress(&mut self, region_id: Uuid, progress: f32, regions: &mut [Region]) {
        if let Some(region) = regions.iter_mut().find(|r| r.id == region_id) {
            region.update_progress(progress);

            self.publisher.publish(LatentEvent::Progress {
                region_id,
                progress,
            });
        }
    }

    /// Handle job completion from hootenanny
    pub fn handle_resolved(
        &mut self,
        region_id: Uuid,
        artifact_id: String,
        content_hash: String,
        content_type: ContentType,
        regions: &mut [Region],
    ) {
        let Some(region) = regions.iter_mut().find(|r| r.id == region_id) else {
            return;
        };

        region.resolve(ResolvedContent {
            content_hash: content_hash.clone(),
            content_type,
            artifact_id: artifact_id.clone(),
        });

        self.active_jobs.remove(&region_id);

        self.publisher.publish(LatentEvent::Resolved {
            region_id,
            artifact_id: artifact_id.clone(),
            content_hash: content_hash.clone(),
            content_type,
        });

        let tool = self.get_tool_name(region);
        if self.config.auto_approve_tools.contains(&tool) {
            region.approve();
            self.publisher.publish(LatentEvent::Approved { region_id });
        } else {
            self.pending_approvals.insert(
                region_id,
                PendingApproval {
                    region_id,
                    artifact_id,
                    content_hash,
                    content_type,
                    resolved_at: Instant::now(),
                    generated_by: None,
                },
            );
        }
    }

    /// Handle job failure from hootenanny
    pub fn handle_failed(&mut self, region_id: Uuid, error: String, regions: &mut [Region]) {
        if let Some(region) = regions.iter_mut().find(|r| r.id == region_id) {
            region.fail();
            self.active_jobs.remove(&region_id);

            self.publisher
                .publish(LatentEvent::Failed { region_id, error });
        }
    }

    /// Get regions awaiting approval
    pub fn pending_approvals(&self) -> Vec<&PendingApproval> {
        self.pending_approvals.values().collect()
    }

    /// Approve a resolved region for playback
    pub fn approve(
        &mut self,
        region_id: Uuid,
        decided_by: Uuid,
        regions: &mut [Region],
    ) -> Result<LatentEvent, LatentError> {
        let Some(region) = regions.iter_mut().find(|r| r.id == region_id) else {
            return Err(LatentError::RegionNotFound(region_id));
        };

        if region.latent_status() != Some(LatentStatus::Resolved) {
            return Err(LatentError::InvalidState {
                region_id,
                expected: "Resolved",
                actual: format!("{:?}", region.latent_status()),
            });
        }

        region.approve();
        self.pending_approvals.remove(&region_id);

        self.decision_log.push(ApprovalDecision {
            region_id,
            decided_by,
            decision: Decision::Approved,
            decided_at: Instant::now(),
            reason: None,
        });

        let event = LatentEvent::Approved { region_id };
        self.publisher.publish(event.clone());

        Ok(event)
    }

    /// Reject a resolved region
    pub fn reject(
        &mut self,
        region_id: Uuid,
        decided_by: Uuid,
        reason: Option<String>,
        regions: &mut [Region],
    ) -> Result<LatentEvent, LatentError> {
        let Some(region) = regions.iter_mut().find(|r| r.id == region_id) else {
            return Err(LatentError::RegionNotFound(region_id));
        };

        if region.latent_status() != Some(LatentStatus::Resolved) {
            return Err(LatentError::InvalidState {
                region_id,
                expected: "Resolved",
                actual: format!("{:?}", region.latent_status()),
            });
        }

        region.reject();
        self.pending_approvals.remove(&region_id);

        self.decision_log.push(ApprovalDecision {
            region_id,
            decided_by,
            decision: Decision::Rejected,
            decided_at: Instant::now(),
            reason: reason.clone(),
        });

        let event = LatentEvent::Rejected { region_id, reason };
        self.publisher.publish(event.clone());

        Ok(event)
    }

    /// Schedule mixing-in of approved content
    pub fn schedule_mix_in(
        &mut self,
        region_id: Uuid,
        at_beat: Beat,
        strategy: Option<MixInStrategy>,
    ) -> Result<MixInSchedule, LatentError> {
        let schedule = MixInSchedule {
            region_id,
            target_beat: at_beat,
            strategy: strategy.unwrap_or(self.config.default_mix_in),
        };

        self.mix_in_queue.push(schedule.clone());
        Ok(schedule)
    }

    /// Get pending mix-ins for playback engine
    pub fn pending_mix_ins(&self) -> &[MixInSchedule] {
        &self.mix_in_queue
    }

    /// Acknowledge that a mix-in has been applied
    pub fn acknowledge_mix_in(&mut self, region_id: Uuid) {
        self.mix_in_queue.retain(|s| s.region_id != region_id);
    }

    /// Mark a mix-in as complete and emit event
    pub fn complete_mix_in(&mut self, region_id: Uuid, at_beat: Beat, strategy: MixInStrategy) {
        self.acknowledge_mix_in(region_id);
        self.publisher.publish(LatentEvent::MixedIn {
            region_id,
            at_beat,
            strategy,
        });
    }

    /// How many jobs are currently running
    pub fn active_job_count(&self) -> usize {
        self.active_jobs.len()
    }

    /// Can we submit more jobs?
    pub fn can_submit(&self) -> bool {
        self.active_job_count() < self.config.max_concurrent_jobs
    }

    /// Get the decision log
    pub fn decision_log(&self) -> &[ApprovalDecision] {
        &self.decision_log
    }

    fn get_tool_name(&self, region: &Region) -> String {
        match &region.behavior {
            crate::primitives::Behavior::Latent { tool, .. } => tool.clone(),
            _ => String::new(),
        }
    }
}

/// Errors from latent operations
#[derive(Debug, Clone)]
pub enum LatentError {
    RegionNotFound(Uuid),
    InvalidState {
        region_id: Uuid,
        expected: &'static str,
        actual: String,
    },
    NotLatent(Uuid),
}

impl std::fmt::Display for LatentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LatentError::RegionNotFound(id) => write!(f, "region not found: {}", id),
            LatentError::InvalidState {
                region_id,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "invalid state for region {}: expected {}, got {}",
                    region_id, expected, actual
                )
            }
            LatentError::NotLatent(id) => write!(f, "region {} is not latent", id),
        }
    }
}

impl std::error::Error for LatentError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockPublisher {
        events: Mutex<Vec<LatentEvent>>,
    }

    impl MockPublisher {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<LatentEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl IOPubPublisher for MockPublisher {
        fn publish(&self, event: LatentEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn create_latent_region() -> Region {
        Region::latent(
            Beat(0.0),
            Beat(4.0),
            "orpheus_generate",
            serde_json::json!({}),
        )
    }

    #[test]
    fn test_job_started() {
        let publisher = Arc::new(MockPublisher::new());
        let mut manager = LatentManager::new(LatentConfig::default(), publisher.clone());
        let mut regions = vec![create_latent_region()];
        let region_id = regions[0].id;

        manager.handle_job_started(region_id, "job_123".to_string(), &mut regions);

        assert_eq!(regions[0].latent_status(), Some(LatentStatus::Running));
        assert_eq!(manager.active_job_count(), 1);

        let events = publisher.events();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LatentEvent::JobStarted { .. }));
    }

    #[test]
    fn test_progress_update() {
        let publisher = Arc::new(MockPublisher::new());
        let mut manager = LatentManager::new(LatentConfig::default(), publisher.clone());
        let mut regions = vec![create_latent_region()];
        let region_id = regions[0].id;

        manager.handle_job_started(region_id, "job_123".to_string(), &mut regions);
        manager.handle_progress(region_id, 0.5, &mut regions);

        let events = publisher.events();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[1],
            LatentEvent::Progress { progress, .. } if (progress - 0.5).abs() < 0.001
        ));
    }

    #[test]
    fn test_resolved_goes_to_pending() {
        let publisher = Arc::new(MockPublisher::new());
        let mut manager = LatentManager::new(LatentConfig::default(), publisher.clone());
        let mut regions = vec![create_latent_region()];
        let region_id = regions[0].id;

        manager.handle_job_started(region_id, "job_123".to_string(), &mut regions);
        manager.handle_resolved(
            region_id,
            "artifact_456".to_string(),
            "hash_abc".to_string(),
            ContentType::Midi,
            &mut regions,
        );

        assert_eq!(regions[0].latent_status(), Some(LatentStatus::Resolved));
        assert_eq!(manager.active_job_count(), 0);
        assert_eq!(manager.pending_approvals().len(), 1);
    }

    #[test]
    fn test_auto_approve() {
        let publisher = Arc::new(MockPublisher::new());
        let mut config = LatentConfig::default();
        config
            .auto_approve_tools
            .insert("orpheus_generate".to_string());

        let mut manager = LatentManager::new(config, publisher.clone());
        let mut regions = vec![create_latent_region()];
        let region_id = regions[0].id;

        manager.handle_job_started(region_id, "job_123".to_string(), &mut regions);
        manager.handle_resolved(
            region_id,
            "artifact_456".to_string(),
            "hash_abc".to_string(),
            ContentType::Midi,
            &mut regions,
        );

        assert_eq!(regions[0].latent_status(), Some(LatentStatus::Approved));
        assert_eq!(manager.pending_approvals().len(), 0);

        let events = publisher.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, LatentEvent::Approved { .. })));
    }

    #[test]
    fn test_manual_approve() {
        let publisher = Arc::new(MockPublisher::new());
        let mut manager = LatentManager::new(LatentConfig::default(), publisher.clone());
        let mut regions = vec![create_latent_region()];
        let region_id = regions[0].id;
        let human_id = Uuid::new_v4();

        manager.handle_job_started(region_id, "job_123".to_string(), &mut regions);
        manager.handle_resolved(
            region_id,
            "artifact_456".to_string(),
            "hash_abc".to_string(),
            ContentType::Midi,
            &mut regions,
        );

        let result = manager.approve(region_id, human_id, &mut regions);
        assert!(result.is_ok());
        assert_eq!(regions[0].latent_status(), Some(LatentStatus::Approved));
        assert!(regions[0].is_playable());

        assert_eq!(manager.decision_log().len(), 1);
        assert_eq!(manager.decision_log()[0].decided_by, human_id);
        assert_eq!(manager.decision_log()[0].decision, Decision::Approved);
    }

    #[test]
    fn test_reject() {
        let publisher = Arc::new(MockPublisher::new());
        let mut manager = LatentManager::new(LatentConfig::default(), publisher.clone());
        let mut regions = vec![create_latent_region()];
        let region_id = regions[0].id;
        let human_id = Uuid::new_v4();

        manager.handle_job_started(region_id, "job_123".to_string(), &mut regions);
        manager.handle_resolved(
            region_id,
            "artifact_456".to_string(),
            "hash_abc".to_string(),
            ContentType::Midi,
            &mut regions,
        );

        let result = manager.reject(
            region_id,
            human_id,
            Some("too busy".to_string()),
            &mut regions,
        );

        assert!(result.is_ok());
        assert_eq!(regions[0].latent_status(), Some(LatentStatus::Rejected));
        assert!(!regions[0].is_playable());
    }

    #[test]
    fn test_failed() {
        let publisher = Arc::new(MockPublisher::new());
        let mut manager = LatentManager::new(LatentConfig::default(), publisher.clone());
        let mut regions = vec![create_latent_region()];
        let region_id = regions[0].id;

        manager.handle_job_started(region_id, "job_123".to_string(), &mut regions);
        manager.handle_failed(region_id, "GPU out of memory".to_string(), &mut regions);

        assert_eq!(regions[0].latent_status(), Some(LatentStatus::Failed));
        assert_eq!(manager.active_job_count(), 0);
    }

    #[test]
    fn test_schedule_mix_in() {
        let publisher = Arc::new(MockPublisher::new());
        let mut manager = LatentManager::new(LatentConfig::default(), publisher.clone());
        let region_id = Uuid::new_v4();

        let schedule = manager
            .schedule_mix_in(region_id, Beat(16.0), None)
            .unwrap();

        assert_eq!(schedule.region_id, region_id);
        assert_eq!(schedule.target_beat.0, 16.0);
        assert_eq!(schedule.strategy, MixInStrategy::HardCut);

        assert_eq!(manager.pending_mix_ins().len(), 1);

        manager.acknowledge_mix_in(region_id);
        assert_eq!(manager.pending_mix_ins().len(), 0);
    }

    #[test]
    fn test_max_concurrent_jobs() {
        let publisher = Arc::new(MockPublisher::new());
        let mut config = LatentConfig::default();
        config.max_concurrent_jobs = 2;

        let mut manager = LatentManager::new(config, publisher.clone());
        let mut regions = vec![
            create_latent_region(),
            create_latent_region(),
            create_latent_region(),
        ];

        manager.handle_job_started(regions[0].id, "job_1".to_string(), &mut regions);
        assert!(manager.can_submit());

        manager.handle_job_started(regions[1].id, "job_2".to_string(), &mut regions);
        assert!(!manager.can_submit());

        manager.handle_resolved(
            regions[0].id,
            "artifact".to_string(),
            "hash".to_string(),
            ContentType::Audio,
            &mut regions,
        );
        assert!(manager.can_submit());
    }

    #[test]
    fn test_approve_wrong_state() {
        let publisher = Arc::new(MockPublisher::new());
        let mut manager = LatentManager::new(LatentConfig::default(), publisher.clone());
        let mut regions = vec![create_latent_region()];
        let region_id = regions[0].id;
        let human_id = Uuid::new_v4();

        let result = manager.approve(region_id, human_id, &mut regions);
        assert!(matches!(result, Err(LatentError::InvalidState { .. })));
    }
}
