//! Async Job System
//!
//! Provides background job execution for long-running operations like model inference.
//! Tools return job IDs immediately, allowing agents to check status and retrieve results later.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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
    pub tool_name: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: u64,  // Unix timestamp in seconds
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
}

impl JobInfo {
    fn new(job_id: JobId, tool_name: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            job_id,
            status: JobStatus::Pending,
            tool_name,
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
                .as_secs()
        );
    }

    fn mark_complete(&mut self, result: serde_json::Value) {
        self.status = JobStatus::Complete;
        self.result = Some(result);
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }

    fn mark_failed(&mut self, error: String) {
        self.status = JobStatus::Failed;
        self.error = Some(error);
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
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
    pub fn create_job(&self, tool_name: String) -> JobId {
        let job_id = JobId::new();
        let job_info = JobInfo::new(job_id.clone(), tool_name.clone());

        let mut jobs = self.jobs.lock().unwrap();
        jobs.insert(job_id.0.clone(), job_info);

        tracing::info!(
            job.id = %job_id,
            job.tool = %tool_name,
            "Job created"
        );

        job_id
    }

    /// Mark a job as running
    pub fn mark_running(&self, job_id: &JobId) -> Result<()> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs.get_mut(job_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        let tool_name = job.tool_name.clone();
        job.mark_running();

        tracing::info!(
            job.id = %job_id,
            job.tool = %tool_name,
            "Job started"
        );

        Ok(())
    }

    /// Mark a job as complete with result
    pub fn mark_complete(&self, job_id: &JobId, result: serde_json::Value) -> Result<()> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs.get_mut(job_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        let tool_name = job.tool_name.clone();
        let duration = job.started_at.map(|started| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() - started
        });

        job.mark_complete(result);

        tracing::info!(
            job.id = %job_id,
            job.tool = %tool_name,
            job.duration_secs = ?duration,
            "Job completed successfully"
        );

        Ok(())
    }

    /// Mark a job as failed with error
    pub fn mark_failed(&self, job_id: &JobId, error: String) -> Result<()> {
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs.get_mut(job_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", job_id))?;

        let tool_name = job.tool_name.clone();
        let duration = job.started_at.map(|started| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() - started
        });

        job.mark_failed(error.clone());

        tracing::error!(
            job.id = %job_id,
            job.tool = %tool_name,
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
            let tool_name = job.tool_name.clone();
            job.status = JobStatus::Cancelled;
            job.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            );

            tracing::warn!(
                job.id = %job_id,
                job.tool = %tool_name,
                "Job cancelled"
            );
        }

        Ok(())
    }

    /// Wait for a job to complete with timeout
    pub async fn wait_for_job(&self, job_id: &JobId, timeout: Option<Duration>) -> Result<JobInfo> {
        let start = Instant::now();
        let timeout = timeout.unwrap_or(Duration::from_secs(300)); // 5 minute default

        loop {
            let job = self.get_job(job_id)?;

            match job.status {
                JobStatus::Complete | JobStatus::Failed | JobStatus::Cancelled => {
                    return Ok(job);
                }
                JobStatus::Pending | JobStatus::Running => {
                    if start.elapsed() > timeout {
                        anyhow::bail!("Job {} timed out after {:?}", job_id, timeout);
                    }
                    // Poll every 500ms
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    /// Clean up old completed jobs
    pub fn cleanup_old_jobs(&self, max_age: Duration) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut jobs = self.jobs.lock().unwrap();
        let mut handles = self.handles.lock().unwrap();

        let mut removed_count = 0;

        jobs.retain(|job_id, job| {
            let should_keep = if let Some(completed_at) = job.completed_at {
                now - completed_at < max_age.as_secs()
            } else {
                true // Keep running/pending jobs
            };

            if !should_keep {
                handles.remove(job_id);
                removed_count += 1;
            }

            should_keep
        });

        if removed_count > 0 {
            tracing::info!(
                jobs.removed = removed_count,
                jobs.remaining = jobs.len(),
                jobs.max_age_secs = max_age.as_secs(),
                "Cleaned up old jobs"
            );
        }
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
