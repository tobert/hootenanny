//! Request and response types for Luanette MCP tools.

use hooteproto::{JobInfo, JobStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request to execute a Lua script stored in CAS.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaExecuteRequest {
    /// CAS hash of the Lua script to execute.
    pub script_hash: String,

    /// Parameters to pass to the script's main() function.
    #[serde(default)]
    pub params: Option<Value>,

    /// Creator identifier for artifact tracking.
    #[serde(default)]
    pub creator: Option<String>,

    /// Tags for the execution result artifact.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to evaluate Lua code directly (for debugging/quick scripts).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaEvalRequest {
    /// Lua code to evaluate.
    pub code: String,

    /// Parameters to pass to the script's main() function.
    #[serde(default)]
    pub params: Option<Value>,
}

/// Request to describe a Lua script's interface.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaDescribeRequest {
    /// CAS hash of the Lua script to describe.
    pub script_hash: String,
}

/// Response from lua_execute tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaExecuteResponse {
    /// Job ID for tracking the execution.
    pub job_id: String,
}

/// Response from lua_eval tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaEvalResponse {
    /// Result returned by the script (JSON-serialized).
    pub result: Value,

    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

/// Response from lua_describe tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LuaDescribeResponse {
    /// Script name from describe() function.
    pub name: Option<String>,

    /// Script description from describe() function.
    pub description: Option<String>,

    /// Parameter schema from describe() function.
    pub params: Option<Value>,

    /// Return type description from describe() function.
    pub returns: Option<String>,
}

/// Script parameter definition (from describe() function).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ParamDefinition {
    /// Parameter type (string, number, boolean, table).
    #[serde(rename = "type")]
    pub param_type: String,

    /// Whether the parameter is required.
    #[serde(default)]
    pub required: bool,

    /// Default value if not provided.
    #[serde(default)]
    pub default: Option<Value>,

    /// Description of the parameter.
    #[serde(default)]
    pub description: Option<String>,
}

// ============================================================================
// Job Management Tools
// ============================================================================

/// Request to get the status of a job.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobStatusRequest {
    /// Job ID to check.
    pub job_id: String,
}

/// Response from job_status tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobStatusResponse {
    /// Job ID.
    pub job_id: String,
    /// Current job status.
    pub status: String,
    /// Script hash being executed.
    pub script_hash: String,
    /// Result if job completed successfully.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error message if job failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Job creation timestamp (Unix seconds).
    pub created_at: u64,
    /// Job start timestamp (Unix seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    /// Job completion timestamp (Unix seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
}

impl From<JobInfo> for JobStatusResponse {
    fn from(info: JobInfo) -> Self {
        let status = match info.status {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Complete => "complete",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        };
        Self {
            job_id: info.job_id.to_string(),
            status: status.to_string(),
            script_hash: info.source,
            result: info.result,
            error: info.error,
            created_at: info.created_at,
            started_at: info.started_at,
            completed_at: info.completed_at,
        }
    }
}

/// Request to poll for job completion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobPollRequest {
    /// Job IDs to poll (empty = just timeout/sleep).
    #[serde(default)]
    pub job_ids: Vec<String>,

    /// Timeout in milliseconds (max 30000).
    pub timeout_ms: u64,

    /// Mode: "any" (return on first complete) or "all" (wait for all).
    #[serde(default = "default_poll_mode")]
    pub mode: String,
}

fn default_poll_mode() -> String {
    "any".to_string()
}

/// Response from job_poll tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobPollResponse {
    /// Jobs that completed during the poll.
    pub completed: Vec<JobStatusResponse>,
    /// Jobs that are still running.
    pub pending: Vec<String>,
    /// Whether the poll timed out.
    pub timed_out: bool,
}

/// Request to cancel a job.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobCancelRequest {
    /// Job ID to cancel.
    pub job_id: String,
}

/// Response from job_cancel tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobCancelResponse {
    /// Job ID that was cancelled.
    pub job_id: String,
    /// Whether cancellation was successful.
    pub success: bool,
}

/// Request to list all jobs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobListRequest {
    /// Filter by status (optional).
    #[serde(default)]
    pub status: Option<String>,

    /// Maximum number of jobs to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

/// Response from job_list tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobListResponse {
    /// List of jobs.
    pub jobs: Vec<JobStatusResponse>,
    /// Total number of jobs (before limit).
    pub total: usize,
}

// ============================================================================
// Script Discovery Tools
// ============================================================================

/// Request to store a Lua script in CAS.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScriptStoreRequest {
    /// Lua script content.
    pub content: String,

    /// Tags for the script artifact (e.g., ["workflow:midi", "author:claude"]).
    #[serde(default)]
    pub tags: Vec<String>,

    /// Creator identifier.
    #[serde(default)]
    pub creator: Option<String>,
}

/// Response from script_store tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScriptStoreResponse {
    /// CAS hash of the stored script.
    pub hash: String,
    /// Artifact ID if created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
}

/// Request to search for Lua scripts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScriptSearchRequest {
    /// Filter by tag (e.g., "workflow:midi").
    #[serde(default)]
    pub tag: Option<String>,

    /// Filter by creator.
    #[serde(default)]
    pub creator: Option<String>,

    /// Search in annotations/vibes.
    #[serde(default)]
    pub vibe: Option<String>,

    /// Maximum results to return.
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    20
}

/// A script found by search.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScriptInfo {
    /// CAS hash of the script.
    pub hash: String,
    /// Artifact ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// Tags on the artifact.
    pub tags: Vec<String>,
    /// Creator if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    /// Script name from describe() if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Script description from describe() if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Response from script_search tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScriptSearchResponse {
    /// Scripts matching the search criteria.
    pub scripts: Vec<ScriptInfo>,
    /// Total count before limit.
    pub total: usize,
}

// ============================================================================
// Job Statistics
// ============================================================================

/// Request to get job store statistics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobStatsRequest {}

/// Response from job_stats tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobStatsResponse {
    /// Total number of jobs.
    pub total: usize,
    /// Number of pending jobs.
    pub pending: usize,
    /// Number of running jobs.
    pub running: usize,
    /// Number of completed jobs.
    pub completed: usize,
    /// Number of failed jobs.
    pub failed: usize,
    /// Number of cancelled jobs.
    pub cancelled: usize,
}

// ============================================================================
// Upstream Management
// ============================================================================

/// Request to refresh tools from an upstream namespace.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolsRefreshRequest {
    /// Namespace to refresh tools for.
    pub namespace: String,
}

/// Response from tools_refresh tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolsRefreshResponse {
    /// Namespace that was refreshed.
    pub namespace: String,
    /// Number of tools discovered.
    pub tool_count: usize,
    /// Names of discovered tools.
    pub tools: Vec<String>,
}

/// Request to remove an upstream namespace.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpstreamRemoveRequest {
    /// Namespace to remove.
    pub namespace: String,
}

/// Response from upstream_remove tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpstreamRemoveResponse {
    /// Namespace that was removed.
    pub namespace: String,
    /// Whether the removal was successful.
    pub removed: bool,
}
