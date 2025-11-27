use crate::api::service::EventDualityServer;
use crate::api::schema::{GetJobStatusRequest, WaitForJobRequest, CancelJobRequest, PollRequest, SleepRequest};
use crate::job_system::{JobId, JobStatus};
use rmcp::{ErrorData as McpError, model::{CallToolResult, Content}};
use tracing;

impl EventDualityServer {
    #[tracing::instrument(
        name = "mcp.tool.get_job_status",
        skip(self, request),
        fields(
            job.id = %request.job_id,
            job.status = tracing::field::Empty,
        )
    )]
    pub async fn get_job_status(
        &self,
        request: GetJobStatusRequest,
    ) -> Result<CallToolResult, McpError> {
        let job_id = JobId::from(request.job_id);

        let job_info = self.job_store.get_job(&job_id)
            .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

        tracing::Span::current().record("job.status", format!("{:?}", job_info.status));

        let json = serde_json::to_string(&job_info)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize job info: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tracing::instrument(
        name = "mcp.tool.wait_for_job",
        skip(self, request),
        fields(
            job.id = %request.job_id,
            job.timeout_seconds = request.timeout_seconds.unwrap_or(86400),
            job.final_status = tracing::field::Empty,
        )
    )]
    pub async fn wait_for_job(
        &self,
        request: WaitForJobRequest,
    ) -> Result<CallToolResult, McpError> {
        let job_id = JobId::from(request.job_id);
        let timeout = std::time::Duration::from_secs(request.timeout_seconds.unwrap_or(86400)); // 24 hours

        let job_info = self.job_store.wait_for_job(&job_id, Some(timeout))
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        tracing::Span::current().record("job.final_status", format!("{:?}", job_info.status));

        let json = serde_json::to_string(&job_info)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize job info: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tracing::instrument(
        name = "mcp.tool.list_jobs",
        skip(self),
        fields(
            jobs.count = tracing::field::Empty,
        )
    )]
    pub async fn list_jobs(&self) -> Result<CallToolResult, McpError> {
        let jobs = self.job_store.list_jobs();

        tracing::Span::current().record("jobs.count", jobs.len());

        let json = serde_json::to_string(&jobs)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize jobs: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tracing::instrument(
        name = "mcp.tool.cancel_job",
        skip(self, request),
        fields(
            job.id = %request.job_id,
        )
    )]
    pub async fn cancel_job(
        &self,
        request: CancelJobRequest,
    ) -> Result<CallToolResult, McpError> {
        let job_id = JobId::from(request.job_id);

        self.job_store.cancel_job(&job_id)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let response = serde_json::json!({
            "status": "cancelled",
            "job_id": job_id.as_str(),
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }

    #[tracing::instrument(
        name = "mcp.tool.poll",
        skip(self, request),
        fields(
            poll.timeout_ms = request.timeout_ms,
            poll.job_count = request.job_ids.len(),
            poll.mode = ?request.mode,
            poll.elapsed_ms = tracing::field::Empty,
            poll.reason = tracing::field::Empty,
        )
    )]
    pub async fn poll(
        &self,
        request: PollRequest,
    ) -> Result<CallToolResult, McpError> {
        use std::time::{Duration, Instant};

        // Cap timeout at 10 seconds (less than 30s SSE keep-alive to prevent disconnects)
        // This ensures we return frequently enough to keep SSE connection alive
        let timeout_ms = request.timeout_ms.min(10000);
        let timeout = Duration::from_millis(timeout_ms);
        let mode = request.mode.as_deref().unwrap_or("any");

        // Validate mode
        if mode != "any" && mode != "all" {
            return Err(McpError::invalid_params(
                format!("mode must be 'any' or 'all', got '{}'", mode),
                None
            ));
        }

        // Convert job_ids to JobId
        let job_ids: Vec<JobId> = request.job_ids.into_iter()
            .map(JobId::from)
            .collect();

        let start = Instant::now();
        let poll_interval = Duration::from_millis(500);

        // SSE keepalive: Always return within 10s to prevent SSE timeout
        // Even if jobs aren't complete, we return with current status
        // Caller can poll() again to continue waiting

        loop {
            let mut completed = Vec::new();
            let mut pending = Vec::new();
            let mut failed = Vec::new();

            // Check status of all jobs
            for job_id in &job_ids {
                match self.job_store.get_job(job_id) {
                    Ok(job_info) => {
                        match job_info.status {
                            JobStatus::Complete => completed.push(job_id.as_str().to_string()),
                            JobStatus::Failed | JobStatus::Cancelled => failed.push(job_id.as_str().to_string()),
                            JobStatus::Pending | JobStatus::Running => pending.push(job_id.as_str().to_string()),
                        }
                    }
                    Err(_) => {
                        // Job not found - treat as failed
                        failed.push(job_id.as_str().to_string());
                    }
                }
            }

            let elapsed = start.elapsed();
            let elapsed_ms = elapsed.as_millis() as u64;

            // Check completion conditions
            let should_return = if job_ids.is_empty() {
                // No jobs - just timeout/sleep
                elapsed >= timeout
            } else if mode == "any" {
                // Return if ANY job completed or failed
                !completed.is_empty() || !failed.is_empty()
            } else {
                // mode == "all" - return if ALL jobs done
                pending.is_empty()
            };

            // ALWAYS return on timeout to prevent SSE disconnects
            // Caller should poll again if jobs still pending
            let reason = if should_return && (!completed.is_empty() || !failed.is_empty()) {
                "job_complete"
            } else if elapsed >= timeout {
                "timeout"
            } else {
                // Keep polling
                tokio::time::sleep(poll_interval).await;
                continue;
            };

            // Record and return
            tracing::Span::current().record("poll.elapsed_ms", elapsed_ms);
            tracing::Span::current().record("poll.reason", reason);

            let response = serde_json::json!({
                "completed": completed,
                "pending": pending,
                "failed": failed,
                "elapsed_ms": elapsed_ms,
                "reason": reason,
            });

            return Ok(CallToolResult::success(vec![Content::text(response.to_string())]));
        }
    }

    #[tracing::instrument(
        name = "mcp.tool.sleep",
        skip(self, request),
        fields(
            sleep.milliseconds = request.milliseconds,
        )
    )]
    pub async fn sleep(
        &self,
        request: SleepRequest,
    ) -> Result<CallToolResult, McpError> {
        // Cap at 30 seconds
        let ms = request.milliseconds.min(30000);

        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;

        let completed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let response = serde_json::json!({
            "slept_ms": ms,
            "completed_at": completed_at,
        });

        Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
    }
}
