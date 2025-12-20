//! KernelState - the latent space Python sees

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::session::SessionId;

/// The world Python sees when it wakes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelState {
    pub session_id: SessionId,
    pub session_name: String,
    pub session_vibe: Option<String>,
    pub tempo_bpm: f64,

    pub transport: Transport,
    pub beat: BeatInfo,

    pub jobs: HashMap<String, JobInfo>,
    pub recent_artifacts: Vec<ArtifactInfo>,

    pub captured_at: DateTime<Utc>,
}

impl KernelState {
    pub fn new(session_id: SessionId, session_name: String, tempo_bpm: f64) -> Self {
        Self {
            session_id,
            session_name,
            session_vibe: None,
            tempo_bpm,
            transport: Transport::Stopped,
            beat: BeatInfo {
                current: 0.0,
                tempo_bpm,
            },
            jobs: HashMap::new(),
            recent_artifacts: Vec::new(),
            captured_at: Utc::now(),
        }
    }

    /// Add artifact to recent list, keeping bounded
    pub fn add_artifact(&mut self, artifact: ArtifactInfo) {
        self.recent_artifacts.insert(0, artifact);
        if self.recent_artifacts.len() > 100 {
            self.recent_artifacts.truncate(100);
        }
    }

    /// Update job state
    pub fn update_job(&mut self, job_id: String, state: JobState, artifact_id: Option<String>) {
        self.jobs.insert(
            job_id.clone(),
            JobInfo {
                job_id,
                state,
                artifact_id,
            },
        );
    }

    /// Serialize to Cap'n Proto bytes
    pub fn to_capnp(&self) -> anyhow::Result<Vec<u8>> {
        // For now, use JSON as placeholder
        // TODO: Implement actual Cap'n Proto serialization
        let json = serde_json::to_vec(self)?;
        Ok(json)
    }

    /// Deserialize from Cap'n Proto bytes
    pub fn from_capnp(bytes: &[u8]) -> anyhow::Result<Self> {
        // For now, use JSON as placeholder
        // TODO: Implement actual Cap'n Proto deserialization
        let state = serde_json::from_slice(bytes)?;
        Ok(state)
    }
}

/// Transport state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    Stopped,
    Playing,
    Paused,
}

impl Transport {
    pub fn as_str(&self) -> &'static str {
        match self {
            Transport::Stopped => "stopped",
            Transport::Playing => "playing",
            Transport::Paused => "paused",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "playing" => Transport::Playing,
            "paused" => Transport::Paused,
            _ => Transport::Stopped,
        }
    }
}

/// Current beat information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatInfo {
    pub current: f64,
    pub tempo_bpm: f64,
}

/// Job tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub job_id: String,
    pub state: JobState,
    pub artifact_id: Option<String>,
}

/// Job execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
    Pending,
    Running,
    Complete,
    Failed,
}

impl JobState {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobState::Pending => "pending",
            JobState::Running => "running",
            JobState::Complete => "complete",
            JobState::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "pending" => JobState::Pending,
            "running" => JobState::Running,
            "complete" => JobState::Complete,
            "failed" => JobState::Failed,
            _ => JobState::Pending,
        }
    }
}

/// Artifact reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub id: String,
    pub content_hash: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_state_serialization() {
        let state = KernelState::new(SessionId::new(), "test".to_string(), 120.0);

        let bytes = state.to_capnp().unwrap();
        let restored = KernelState::from_capnp(&bytes).unwrap();

        assert_eq!(restored.session_name, "test");
        assert_eq!(restored.tempo_bpm, 120.0);
    }

    #[test]
    fn test_artifact_limit() {
        let mut state = KernelState::new(SessionId::new(), "test".to_string(), 120.0);

        for i in 0..150 {
            state.add_artifact(ArtifactInfo {
                id: format!("art_{}", i),
                content_hash: format!("hash_{}", i),
                tags: vec![],
                created_at: Utc::now(),
            });
        }

        assert_eq!(state.recent_artifacts.len(), 100);
        // Most recent should be first
        assert_eq!(state.recent_artifacts[0].id, "art_149");
    }
}
