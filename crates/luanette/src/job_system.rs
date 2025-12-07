//! Async Job System for Luanette
//!
//! Provides background job execution for long-running Lua script operations.
//! Scripts return job IDs immediately, allowing agents to check status and retrieve results later.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Unique identifier for a background job
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(String);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for JobId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Current status of a background job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// Job is queued but not yet started
    Pending,
    /// Job is currently executing
    Running,
    /// Job completed successfully
    Complete,
    /// Job failed with an error
    Failed,
    /// Job was cancelled
    Cancelled,
}

/// Information about a job and its result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub job_id: JobId,
    pub status: JobStatus,
    pub script_hash: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
}

impl JobInfo {
    fn new(job_id: JobId, script_hash: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            job_id,
            status: JobStatus::Pending,
            script_hash,
            result: None,
            error: None,
            created_at: now,
            started_at: None,
            completed_at: None,
        }
    }

    fn mark_running(&mut self) {
        self.status = JobStatus::Running;
        self.started_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    fn mark_complete(&mut self, result: serde_json::Value) {
        self.status = JobStatus::Complete;
        self.result = Some(result);
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    fn mark_failed(&mut self, error: String) {
        self.status = JobStatus::Failed;
        self.error = Some(error);
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }
}

/// Statistics about job store state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobStoreStats {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
}

/// Storage for background jobs
#[derive(Clone)]
pub struct JobStore {
    jobs: Arc<Mutex<HashMap<String, JobInfo>>>,
    handles: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl JobStore {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new job and return its ID
    pub fn create_job(&self, script_hash: String) -> JobId {
        let job_id = JobId::new();
        let job_info = JobInfo::new(job_id.clone(), script_hash.clone());

        let mut jobs = self.jobs.lock().unwrap();
        jobs.insert(job_id.0.clone(), job_info);

        tracing::info!(
            job.id = %job_id,
            job.script_hash = %script_hash,
            "Job created"
        );

        job_id
    }

    /// Mark a job as running
    pub fn mark_running(&self, job_id: &JobId) -> Result<()> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get_mut(job_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        let script_hash = job.script_hash.clone();
        job.mark_running();

        tracing::info!(
            job.id = %job_id,
            job.script_hash = %script_hash,
            "Job started"
        );

        Ok(())
    }

    /// Mark a job as complete with result
    pub fn mark_complete(&self, job_id: &JobId, result: serde_json::Value) -> Result<()> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get_mut(job_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        let script_hash = job.script_hash.clone();
        let duration = job.started_at.map(|started| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - started
        });

        job.mark_complete(result);

        tracing::info!(
            job.id = %job_id,
            job.script_hash = %script_hash,
            job.duration_secs = ?duration,
            "Job completed successfully"
        );

        Ok(())
    }

    /// Mark a job as failed with error
    pub fn mark_failed(&self, job_id: &JobId, error: String) -> Result<()> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs
            .get_mut(job_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        let script_hash = job.script_hash.clone();
        let duration = job.started_at.map(|started| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - started
        });

        job.mark_failed(error.clone());

        tracing::error!(
            job.id = %job_id,
            job.script_hash = %script_hash,
            job.duration_secs = ?duration,
            job.error = %error,
            "Job failed"
        );

        Ok(())
    }

    /// Get job information
    pub fn get_job(&self, job_id: &JobId) -> Result<JobInfo> {
        let jobs = self.jobs.lock().unwrap();
        jobs.get(job_id.as_str())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))
    }

    /// List all jobs
    pub fn list_jobs(&self) -> Vec<JobInfo> {
        let jobs = self.jobs.lock().unwrap();
        jobs.values().cloned().collect()
    }

    /// Store a task handle for potential cancellation
    pub fn store_handle(&self, job_id: &JobId, handle: JoinHandle<()>) {
        let mut handles = self.handles.lock().unwrap();
        handles.insert(job_id.0.clone(), handle);
    }

    /// Cancel a job
    pub fn cancel_job(&self, job_id: &JobId) -> Result<()> {
        // Abort the task if it exists
        let mut handles = self.handles.lock().unwrap();
        if let Some(handle) = handles.remove(job_id.as_str()) {
            handle.abort();
        }

        // Mark as cancelled
        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.get_mut(job_id.as_str()) {
            let script_hash = job.script_hash.clone();
            job.status = JobStatus::Cancelled;
            job.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            );

            tracing::warn!(
                job.id = %job_id,
                job.script_hash = %script_hash,
                "Job cancelled"
            );
        }

        Ok(())
    }

    /// Get job store statistics for monitoring
    pub fn stats(&self) -> JobStoreStats {
        let jobs = self.jobs.lock().unwrap();
        let mut stats = JobStoreStats::default();

        for job in jobs.values() {
            stats.total += 1;
            match job.status {
                JobStatus::Pending => stats.pending += 1,
                JobStatus::Running => stats.running += 1,
                JobStatus::Complete => stats.completed += 1,
                JobStatus::Failed => stats.failed += 1,
                JobStatus::Cancelled => stats.cancelled += 1,
            }
        }

        stats
    }
}

impl Default for JobStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_lifecycle() {
        let store = JobStore::new();

        // Create job
        let job_id = store.create_job("abc123".to_string());

        // Check pending
        let info = store.get_job(&job_id).unwrap();
        assert_eq!(info.status, JobStatus::Pending);
        assert_eq!(info.script_hash, "abc123");

        // Mark running
        store.mark_running(&job_id).unwrap();
        let info = store.get_job(&job_id).unwrap();
        assert_eq!(info.status, JobStatus::Running);
        assert!(info.started_at.is_some());

        // Mark complete
        store
            .mark_complete(&job_id, serde_json::json!({"result": 42}))
            .unwrap();
        let info = store.get_job(&job_id).unwrap();
        assert_eq!(info.status, JobStatus::Complete);
        assert!(info.completed_at.is_some());
        assert_eq!(info.result, Some(serde_json::json!({"result": 42})));
    }

    #[test]
    fn test_job_failure() {
        let store = JobStore::new();

        let job_id = store.create_job("script123".to_string());
        store.mark_running(&job_id).unwrap();
        store
            .mark_failed(&job_id, "Lua syntax error".to_string())
            .unwrap();

        let info = store.get_job(&job_id).unwrap();
        assert_eq!(info.status, JobStatus::Failed);
        assert_eq!(info.error, Some("Lua syntax error".to_string()));
    }

    #[test]
    fn test_job_cancellation() {
        let store = JobStore::new();

        let job_id = store.create_job("long_script".to_string());
        store.mark_running(&job_id).unwrap();
        store.cancel_job(&job_id).unwrap();

        let info = store.get_job(&job_id).unwrap();
        assert_eq!(info.status, JobStatus::Cancelled);
    }

    #[test]
    fn test_job_stats() {
        let store = JobStore::new();

        store.create_job("a".to_string());
        store.create_job("b".to_string());
        let job_c = store.create_job("c".to_string());
        store.mark_running(&job_c).unwrap();

        let stats = store.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.pending, 2);
        assert_eq!(stats.running, 1);
    }

    #[test]
    fn test_list_jobs() {
        let store = JobStore::new();

        store.create_job("script1".to_string());
        store.create_job("script2".to_string());

        let jobs = store.list_jobs();
        assert_eq!(jobs.len(), 2);
    }
}
