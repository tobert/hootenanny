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
}

impl Default for JobStore {
    fn default() -> Self {
        Self::new()
    }
}
