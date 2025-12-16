use crate::api::responses::{JobStatusResponse, JobListResponse, JobSummary, JobCancelResponse, JobPollResponse, JobSleepResponse, GpuInfo, GpuSparklines};
use crate::api::service::EventDualityServer;
use crate::api::schema::{GetJobStatusRequest, CancelJobRequest, PollRequest, SleepRequest};
use hooteproto::{JobId, JobStatus, ToolOutput, ToolResult, ToolError};
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
    ) -> ToolResult {
        let job_id = JobId::from(request.job_id);

        let job_info = self.job_store.get_job(&job_id)
            .map_err(|e| ToolError::not_found("job", e.to_string()))?;

        tracing::Span::current().record("job.status", format!("{:?}", job_info.status));

        let status = match job_info.status {
            JobStatus::Pending => crate::api::responses::JobStatus::Pending,
            JobStatus::Running => crate::api::responses::JobStatus::Running,
            JobStatus::Complete => crate::api::responses::JobStatus::Completed,
            JobStatus::Failed => crate::api::responses::JobStatus::Failed,
            JobStatus::Cancelled => crate::api::responses::JobStatus::Cancelled,
        };

        let response = JobStatusResponse {
            job_id: job_info.job_id.as_str().to_string(),
            status,
            tool_name: job_info.source.clone(),
            result: job_info.result.clone(),
            error: job_info.error.clone(),
            created_at: Some(job_info.created_at as i64),
            started_at: job_info.started_at.map(|t| t as i64),
            completed_at: job_info.completed_at.map(|t| t as i64),
        };

        Ok(ToolOutput::new(
            format!("Job {}: {:?}", job_info.job_id.as_str(), job_info.status),
            &response,
        ))
    }

    #[tracing::instrument(
        name = "mcp.tool.list_jobs",
        skip(self),
        fields(
            jobs.count = tracing::field::Empty,
        )
    )]
    pub async fn list_jobs(&self) -> ToolResult {
        let jobs = self.job_store.list_jobs();

        tracing::Span::current().record("jobs.count", jobs.len());

        let job_summaries: Vec<JobSummary> = jobs.iter().map(|j| {
            let status = match j.status {
                JobStatus::Pending => crate::api::responses::JobStatus::Pending,
                JobStatus::Running => crate::api::responses::JobStatus::Running,
                JobStatus::Complete => crate::api::responses::JobStatus::Completed,
                JobStatus::Failed => crate::api::responses::JobStatus::Failed,
                JobStatus::Cancelled => crate::api::responses::JobStatus::Cancelled,
            };
            JobSummary {
                job_id: j.job_id.as_str().to_string(),
                tool_name: j.source.clone(),
                status,
                created_at: j.created_at as i64,
            }
        }).collect();

        let response = JobListResponse {
            total: job_summaries.len(),
            jobs: job_summaries,
        };

        Ok(ToolOutput::new(
            format!("{} jobs", response.total),
            &response,
        ))
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
    ) -> ToolResult {
        let job_id = JobId::from(request.job_id);

        self.job_store.cancel_job(&job_id)
            .map_err(|e| ToolError::internal(e.to_string()))?;

        let response = JobCancelResponse {
            job_id: job_id.as_str().to_string(),
            cancelled: true,
            message: "Job cancelled successfully".to_string(),
        };

        Ok(ToolOutput::new(
            format!("Job {}: {}", job_id.as_str(), response.message),
            &response,
        ))
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
    ) -> ToolResult {
        use std::time::{Duration, Instant};

        let timeout_ms = request.timeout_ms.min(10000);
        let timeout = Duration::from_millis(timeout_ms);
        let mode = request.mode.as_deref().unwrap_or("any");

        if mode != "any" && mode != "all" {
            return Err(ToolError::validation(
                "invalid_params",
                format!("mode must be 'any' or 'all', got '{}'", mode)
            ));
        }

        let job_ids: Vec<JobId> = request.job_ids.into_iter()
            .map(JobId::from)
            .collect();

        let start = Instant::now();
        let poll_interval = Duration::from_millis(500);

        loop {
            let mut completed = Vec::new();
            let mut pending = Vec::new();
            let mut failed = Vec::new();

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
                        failed.push(job_id.as_str().to_string());
                    }
                }
            }

            let elapsed = start.elapsed();
            let elapsed_ms = elapsed.as_millis() as u64;

            let should_return = if job_ids.is_empty() {
                elapsed >= timeout
            } else if mode == "any" {
                !completed.is_empty() || !failed.is_empty()
            } else {
                pending.is_empty()
            };

            let reason = if should_return && (!completed.is_empty() || !failed.is_empty()) {
                "job_complete"
            } else if elapsed >= timeout {
                "timeout"
            } else {
                tokio::time::sleep(poll_interval).await;
                continue;
            };

            tracing::Span::current().record("poll.elapsed_ms", elapsed_ms);
            tracing::Span::current().record("poll.reason", reason);

            let gpu = match self.gpu_monitor.fetch_status().await {
                Ok(status) => Some(GpuInfo {
                    summary: status.summary,
                    health: status.health,
                    utilization: status.gpu.util_pct as u8,
                    status: status.gpu.status,
                    vram_used_gb: status.gpu.vram_used_gb,
                    vram_total_gb: status.gpu.vram_total_gb,
                    temp_c: status.gpu.temp_c as u8,
                    power_w: status.gpu.power_w as u16,
                    oom_risk: status.gpu.oom_risk,
                    sparklines: if status.sparklines.util.peak > 10.0 {
                        Some(GpuSparklines {
                            util: status.sparklines.util.spark,
                            temp: status.sparklines.temp.spark,
                            power: status.sparklines.power.spark,
                            vram: status.sparklines.vram.spark,
                            util_avg: status.sparklines.util.avg,
                            util_peak: status.sparklines.util.peak,
                        })
                    } else {
                        None
                    },
                    services: None,
                }),
                Err(e) => {
                    tracing::warn!("Failed to fetch GPU status: {:#}", e);
                    None
                }
            };

            let response = JobPollResponse {
                completed: completed.iter().map(|id| id.as_str().to_string()).collect(),
                failed: failed.iter().map(|id| id.as_str().to_string()).collect(),
                pending: pending.iter().map(|id| id.as_str().to_string()).collect(),
                reason: reason.to_string(),
                elapsed_ms,
                gpu,
            };

            return Ok(ToolOutput::new(
                format!("Poll: {} completed, {} failed, {} pending ({})",
                    response.completed.len(), response.failed.len(), response.pending.len(), reason),
                &response,
            ));
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
    ) -> ToolResult {
        let ms = request.milliseconds.min(30000);

        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;

        let completed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let response = JobSleepResponse {
            slept_ms: ms,
            completed_at,
        };

        Ok(ToolOutput::new(format!("Slept for {}ms", ms), &response))
    }
}
