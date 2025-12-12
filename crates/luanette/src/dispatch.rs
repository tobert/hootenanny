//! Dispatch hooteproto Payloads to handler logic
//!
//! This module bridges hooteproto messages to the existing Luanette
//! implementation, handling conversion between protocol types and internal types.

use hooteproto::{JobId, JobStatus, Payload, PollMode, ToolInfo};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, instrument};

use crate::error::format_lua_error;
use crate::job_system::JobStore;
use crate::runtime::LuaRuntime;

/// Dispatcher handles hooteproto Payload messages
pub struct Dispatcher {
    runtime: Arc<LuaRuntime>,
    jobs: Arc<JobStore>,
}

impl Dispatcher {
    pub fn new(runtime: Arc<LuaRuntime>, jobs: Arc<JobStore>) -> Self {
        Self { runtime, jobs }
    }

    /// Evaluate Lua code directly
    #[instrument(skip(self, code, params), fields(code_len = code.len()))]
    pub async fn lua_eval(&self, code: &str, params: Option<Value>) -> Payload {
        let result = if let Some(p) = params {
            self.runtime.execute(code, p).await
        } else {
            self.runtime.eval(code).await
        };

        match result {
            Ok(exec_result) => Payload::Success {
                result: serde_json::json!({
                    "result": exec_result.result,
                    "duration_ms": exec_result.duration.as_millis() as u64,
                }),
            },
            Err(e) => Payload::Error {
                code: "lua_error".to_string(),
                message: format_lua_error(&e),
                details: None,
            },
        }
    }

    /// Get status of a job
    #[instrument(skip(self))]
    pub async fn job_status(&self, job_id: &str) -> Payload {
        let job_id = JobId::from(job_id.to_string());
        match self.jobs.get_job(&job_id) {
            Ok(info) => Payload::Success {
                result: serde_json::to_value(&info).unwrap_or_default(),
            },
            Err(e) => Payload::Error {
                code: "job_not_found".to_string(),
                message: e.to_string(),
                details: None,
            },
        }
    }

    /// List all jobs
    #[instrument(skip(self))]
    pub async fn job_list(&self, status_filter: Option<&str>) -> Payload {
        let all_jobs = self.jobs.list_jobs();

        // Filter by status if requested
        let jobs: Vec<_> = if let Some(filter) = status_filter {
            all_jobs
                .into_iter()
                .filter(|j| {
                    let status_str = match j.status {
                        JobStatus::Pending => "pending",
                        JobStatus::Running => "running",
                        JobStatus::Complete => "complete",
                        JobStatus::Failed => "failed",
                        JobStatus::Cancelled => "cancelled",
                    };
                    status_str == filter
                })
                .collect()
        } else {
            all_jobs
        };

        Payload::Success {
            result: serde_json::to_value(&jobs).unwrap_or_default(),
        }
    }

    /// Cancel a job
    #[instrument(skip(self))]
    pub async fn job_cancel(&self, job_id: &str) -> Payload {
        let job_id = JobId::from(job_id.to_string());
        match self.jobs.cancel_job(&job_id) {
            Ok(()) => Payload::Success {
                result: serde_json::json!({"cancelled": true, "job_id": job_id.to_string()}),
            },
            Err(e) => Payload::Error {
                code: "cancel_failed".to_string(),
                message: e.to_string(),
                details: None,
            },
        }
    }

    /// Execute a script from CAS asynchronously
    #[instrument(skip(self, _params, _tags))]
    pub async fn job_execute(
        &self,
        script_hash: &str,
        _params: Value,
        _tags: Option<Vec<String>>,
    ) -> Payload {
        // TODO: Fetch script from CAS and execute
        // For now, create a job but don't actually run it
        debug!("job_execute not fully implemented yet - needs CAS integration");

        let job_id = self.jobs.create_job(script_hash.to_string());

        Payload::Success {
            result: serde_json::json!({
                "job_id": job_id.to_string(),
                "script_hash": script_hash,
                "status": "pending",
                "note": "CAS integration not yet implemented"
            }),
        }
    }

    /// Poll for job completion
    #[instrument(skip(self))]
    pub async fn job_poll(
        &self,
        job_ids: Vec<String>,
        timeout_ms: u64,
        mode: PollMode,
    ) -> Payload {
        let timeout_ms = timeout_ms.min(30000);
        let poll_interval = Duration::from_millis(100);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

        let job_ids: Vec<JobId> = job_ids.into_iter().map(JobId::from).collect();
        let mode_all = matches!(mode, PollMode::All);

        loop {
            let mut completed = Vec::new();
            let mut pending = Vec::new();

            for job_id in &job_ids {
                if let Ok(info) = self.jobs.get_job(job_id) {
                    match info.status {
                        JobStatus::Complete | JobStatus::Failed | JobStatus::Cancelled => {
                            completed.push(serde_json::to_value(&info).unwrap_or_default());
                        }
                        _ => {
                            pending.push(job_id.to_string());
                        }
                    }
                } else {
                    pending.push(job_id.to_string());
                }
            }

            let should_return = if mode_all {
                pending.is_empty()
            } else {
                !completed.is_empty() || pending.is_empty()
            };

            if should_return || tokio::time::Instant::now() >= deadline {
                let timed_out = !pending.is_empty() && tokio::time::Instant::now() >= deadline;
                return Payload::Success {
                    result: serde_json::json!({
                        "completed": completed,
                        "pending": pending,
                        "timed_out": timed_out,
                    }),
                };
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Store a script (stub - needs CAS)
    #[instrument(skip(self, content))]
    pub async fn script_store(
        &self,
        content: &str,
        tags: Option<Vec<String>>,
        creator: Option<String>,
    ) -> Payload {
        debug!("script_store not fully implemented yet - needs CAS");

        // Compute a simple hash for now
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let hash = format!("{:016x}", hasher.finish());

        Payload::Success {
            result: serde_json::json!({
                "hash": hash,
                "size": content.len(),
                "tags": tags,
                "creator": creator,
                "note": "CAS integration not yet implemented"
            }),
        }
    }

    /// Search for scripts (stub - needs CAS)
    #[instrument(skip(self))]
    pub async fn script_search(
        &self,
        tag: Option<String>,
        creator: Option<String>,
        vibe: Option<String>,
    ) -> Payload {
        debug!("script_search not fully implemented yet - needs CAS");

        Payload::Success {
            result: serde_json::json!({
                "scripts": [],
                "query": {
                    "tag": tag,
                    "creator": creator,
                    "vibe": vibe,
                },
                "note": "CAS integration not yet implemented"
            }),
        }
    }

    /// Describe a script's interface (stub - needs CAS)
    #[instrument(skip(self))]
    pub async fn lua_describe(&self, script_hash: &str) -> Payload {
        debug!("lua_describe not fully implemented yet - needs CAS");

        Payload::Error {
            code: "not_implemented".to_string(),
            message: "lua_describe requires CAS integration".to_string(),
            details: Some(serde_json::json!({"script_hash": script_hash})),
        }
    }

    /// List available tools
    pub async fn list_tools(&self) -> Payload {
        let tools = vec![
            ToolInfo {
                name: "lua_eval".to_string(),
                description: "Evaluate Lua code directly".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "code": {"type": "string"},
                        "params": {"type": "object"}
                    },
                    "required": ["code"]
                }),
            },
            ToolInfo {
                name: "job_execute".to_string(),
                description: "Execute a Lua script from CAS asynchronously".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "script_hash": {"type": "string"},
                        "params": {"type": "object"},
                        "tags": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["script_hash"]
                }),
            },
            ToolInfo {
                name: "job_status".to_string(),
                description: "Get the status of a job".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "job_id": {"type": "string"}
                    },
                    "required": ["job_id"]
                }),
            },
            ToolInfo {
                name: "job_list".to_string(),
                description: "List all jobs".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "status": {"type": "string"}
                    }
                }),
            },
            ToolInfo {
                name: "job_poll".to_string(),
                description: "Poll for job completion".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "job_ids": {"type": "array", "items": {"type": "string"}},
                        "timeout_ms": {"type": "integer"},
                        "mode": {"type": "string", "enum": ["any", "all"]}
                    },
                    "required": ["job_ids"]
                }),
            },
            ToolInfo {
                name: "job_cancel".to_string(),
                description: "Cancel a running job".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "job_id": {"type": "string"}
                    },
                    "required": ["job_id"]
                }),
            },
        ];

        Payload::ToolList { tools }
    }
}
