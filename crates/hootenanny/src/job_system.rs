//! Async Job System
//!
//! Provides background job execution for long-running operations like model inference.
//! Tools return job IDs immediately, allowing agents to check status and retrieve results later.
//!
//! Uses canonical job types from hooteproto for interoperability with luanette.

use anyhow::Result;
use hooteproto::{JobId, JobInfo, JobStatus, JobStoreStats};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

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
    pub fn create_job(&self, source: String) -> JobId {
        let job_id = JobId::new();
        let job_info = JobInfo::new(job_id.clone(), source.clone());

        let mut jobs = self.jobs.lock().unwrap();
        jobs.insert(job_id.as_str().to_string(), job_info);

        tracing::info!(
            job.id = %job_id,
            job.source = %source,
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

        let source = job.source.clone();
        job.mark_running();

        tracing::info!(
            job.id = %job_id,
            job.source = %source,
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

        let source = job.source.clone();
        let duration = job.duration_secs();

        job.mark_complete(result);

        tracing::info!(
            job.id = %job_id,
            job.source = %source,
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

        let source = job.source.clone();
        let duration = job.duration_secs();

        job.mark_failed(error.clone());

        tracing::error!(
            job.id = %job_id,
            job.source = %source,
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
        handles.insert(job_id.as_str().to_string(), handle);
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
            let source = job.source.clone();
            job.mark_cancelled();

            tracing::warn!(
                job.id = %job_id,
                job.source = %source,
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

    /// Remove completed/failed/cancelled jobs older than the given age.
    ///
    /// Returns the number of jobs removed.
    pub fn cleanup_completed_older_than(&self, max_age_secs: u64) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cutoff = now.saturating_sub(max_age_secs);

        let mut jobs = self.jobs.lock().unwrap();
        let mut handles = self.handles.lock().unwrap();

        let to_remove: Vec<String> = jobs
            .iter()
            .filter(|(_, job)| {
                // Only remove terminal states
                let is_terminal = matches!(
                    job.status,
                    JobStatus::Complete | JobStatus::Failed | JobStatus::Cancelled
                );

                // Check if completed before cutoff
                let is_old = job.completed_at.is_some_and(|t| t < cutoff);

                is_terminal && is_old
            })
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();

        for id in to_remove {
            jobs.remove(&id);
            handles.remove(&id);
        }

        if count > 0 {
            tracing::debug!(removed = count, max_age_secs, "Cleaned up expired jobs");
        }

        count
    }

    /// Remove jobs matching a specific source that are older than the given age.
    ///
    /// Useful for cleaning up fire-and-forget jobs (e.g., garden_play) more aggressively.
    pub fn cleanup_by_source(&self, source_prefix: &str, max_age_secs: u64) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cutoff = now.saturating_sub(max_age_secs);

        let mut jobs = self.jobs.lock().unwrap();
        let mut handles = self.handles.lock().unwrap();

        let to_remove: Vec<String> = jobs
            .iter()
            .filter(|(_, job)| {
                let matches_source = job.source.starts_with(source_prefix);
                let is_terminal = matches!(
                    job.status,
                    JobStatus::Complete | JobStatus::Failed | JobStatus::Cancelled
                );
                let is_old = job.completed_at.is_some_and(|t| t < cutoff);

                matches_source && is_terminal && is_old
            })
            .map(|(id, _)| id.clone())
            .collect();

        let count = to_remove.len();

        for id in to_remove {
            jobs.remove(&id);
            handles.remove(&id);
        }

        if count > 0 {
            tracing::debug!(
                removed = count,
                source_prefix,
                max_age_secs,
                "Cleaned up expired jobs by source"
            );
        }

        count
    }
}

impl Default for JobStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Default TTL for completed jobs (5 minutes)
pub const DEFAULT_JOB_TTL_SECS: u64 = 300;

/// TTL for fire-and-forget jobs like garden commands (60 seconds)
pub const FIRE_AND_FORGET_TTL_SECS: u64 = 60;

/// Spawn a background task that periodically cleans up expired jobs.
///
/// Runs every `interval_secs` and removes:
/// - Fire-and-forget jobs (garden_*) older than 60 seconds
/// - Other completed jobs older than 5 minutes
pub fn spawn_cleanup_task(job_store: JobStore, interval_secs: u64) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));

        loop {
            interval.tick().await;

            // Clean up garden jobs more aggressively (60s TTL)
            job_store.cleanup_by_source("garden_", FIRE_AND_FORGET_TTL_SECS);

            // Clean up all other completed jobs (5 minute TTL)
            job_store.cleanup_completed_older_than(DEFAULT_JOB_TTL_SECS);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_preserves_running_jobs() {
        let store = JobStore::new();

        // Create a running job
        let job_id = store.create_job("test_tool".to_string());
        store.mark_running(&job_id).unwrap();

        // Cleanup should not remove running jobs even with huge TTL
        let removed = store.cleanup_completed_older_than(0);
        assert_eq!(removed, 0);

        // Job should still exist
        assert!(store.get_job(&job_id).is_ok());
    }

    #[test]
    fn test_cleanup_preserves_pending_jobs() {
        let store = JobStore::new();

        // Create a pending job (never started)
        let job_id = store.create_job("test_tool".to_string());

        // Cleanup should not remove pending jobs
        let removed = store.cleanup_completed_older_than(0);
        assert_eq!(removed, 0);

        // Job should still exist
        assert!(store.get_job(&job_id).is_ok());
    }

    #[test]
    fn test_cleanup_filters_by_source() {
        let store = JobStore::new();

        // Create a running garden job and a running other job
        let garden_job = store.create_job("garden_play".to_string());
        let other_job = store.create_job("orpheus_generate".to_string());

        store.mark_running(&garden_job).unwrap();
        store.mark_running(&other_job).unwrap();

        // Neither should be cleaned (both running)
        let removed = store.cleanup_by_source("garden_", 0);
        assert_eq!(removed, 0);

        // Both should exist
        assert!(store.get_job(&garden_job).is_ok());
        assert!(store.get_job(&other_job).is_ok());
    }

    #[test]
    fn test_cleanup_with_backdated_completion() {
        let store = JobStore::new();

        // Create and complete a job
        let job_id = store.create_job("test_tool".to_string());
        store.mark_running(&job_id).unwrap();
        store
            .mark_complete(&job_id, serde_json::json!({"result": "ok"}))
            .unwrap();

        // Manually backdate the completion time
        {
            let mut jobs = store.jobs.lock().unwrap();
            if let Some(job) = jobs.get_mut(job_id.as_str()) {
                // Set completed_at to 100 seconds ago
                job.completed_at = Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                        .saturating_sub(100),
                );
            }
        }

        // Cleanup with 50 second TTL should remove it (100 > 50)
        let removed = store.cleanup_completed_older_than(50);
        assert_eq!(removed, 1);

        // Job should be gone
        assert!(store.get_job(&job_id).is_err());
    }

    #[test]
    fn test_cleanup_by_source_with_backdated() {
        let store = JobStore::new();

        // Create jobs with different sources
        let garden_job = store.create_job("garden_play".to_string());
        let other_job = store.create_job("orpheus_generate".to_string());

        // Complete both
        store.mark_running(&garden_job).unwrap();
        store
            .mark_complete(&garden_job, serde_json::json!({}))
            .unwrap();
        store.mark_running(&other_job).unwrap();
        store
            .mark_complete(&other_job, serde_json::json!({}))
            .unwrap();

        // Backdate both jobs
        {
            let mut jobs = store.jobs.lock().unwrap();
            let old_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(100);

            for job in jobs.values_mut() {
                job.completed_at = Some(old_time);
            }
        }

        // Cleanup only garden jobs
        let removed = store.cleanup_by_source("garden_", 50);
        assert_eq!(removed, 1);

        // Garden job gone, other job remains
        assert!(store.get_job(&garden_job).is_err());
        assert!(store.get_job(&other_job).is_ok());
    }
}
