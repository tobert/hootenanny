//! Domain type wrappers for Cap'n Proto generated types
//!
//! This module provides ergonomic Rust wrappers around Cap'n Proto types,
//! enabling idiomatic Rust usage while maintaining cross-language compatibility.
//!
//! ## Design Philosophy
//!
//! - Cap'n Proto schemas are the **single source of truth** for wire types
//! - Rust newtypes provide **type safety** (JobId vs String)
//! - Helper methods provide **ergonomic APIs** for common operations
//! - Python/Lua clients use generated capnp types directly (no wrappers needed)

use crate::{common_capnp, jobs_capnp};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// JobId - Newtype for type safety
// ============================================================================

/// Unique identifier for a background job
///
/// This is a thin Rust wrapper for ergonomics. On the wire, it's just Text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(String);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to Cap'n Proto wire format (just the string value)
    pub fn as_text(&self) -> &str {
        &self.0
    }

    /// Create from Cap'n Proto wire format
    pub fn from_text(s: impl Into<String>) -> Self {
        Self(s.into())
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

impl From<&str> for JobId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ============================================================================
// JobStatus - Type alias to capnp enum
// ============================================================================

/// Job status is defined in common.capnp and used directly
///
/// Rust code uses the generated enum: `common_capnp::JobStatus`
/// Python code uses: `hooteproto.common.JobStatus`
pub type JobStatus = common_capnp::JobStatus;

// Helper functions for JobStatus
impl JobStatus {
    /// Convert to lowercase string for JSON/serde compatibility
    pub fn to_string_lower(&self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Running => "running",
            JobStatus::Complete => "complete",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    /// Parse from string (case-insensitive)
    pub fn from_str_lower(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(JobStatus::Pending),
            "running" => Some(JobStatus::Running),
            "complete" => Some(JobStatus::Complete),
            "failed" => Some(JobStatus::Failed),
            "cancelled" => Some(JobStatus::Cancelled),
            _ => None,
        }
    }
}

// ============================================================================
// JobInfo - Ergonomic wrapper for jobs_capnp::JobInfo
// ============================================================================

/// Rich domain type for job information
///
/// Wraps the Cap'n Proto JobInfo with convenient methods
#[derive(Clone)]
pub struct JobInfo {
    pub job_id: JobId,
    pub status: JobStatus,
    pub source: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
}

impl JobInfo {
    pub fn new(job_id: JobId, source: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            job_id,
            status: JobStatus::Pending,
            source,
            result: None,
            error: None,
            created_at: now,
            started_at: None,
            completed_at: None,
        }
    }

    pub fn mark_running(&mut self) {
        self.status = JobStatus::Running;
        self.started_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    pub fn mark_complete(&mut self, result: serde_json::Value) {
        self.status = JobStatus::Complete;
        self.result = Some(result);
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    pub fn mark_failed(&mut self, error: String) {
        self.status = JobStatus::Failed;
        self.error = Some(error);
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    pub fn mark_cancelled(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    /// Duration in seconds if job has started
    pub fn duration_secs(&self) -> Option<u64> {
        self.started_at.map(|started| {
            let end = self.completed_at.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            });
            end.saturating_sub(started)
        })
    }

    /// Write to Cap'n Proto builder for wire transmission
    pub fn to_capnp<'a>(&self, builder: &mut jobs_capnp::job_info::Builder<'a>) {
        builder.set_job_id(self.job_id.as_text());
        builder.set_status(self.status);
        builder.set_source(&self.source);

        // Result as JSON string (or empty)
        if let Some(ref result) = self.result {
            builder.set_result(&serde_json::to_string(result).unwrap_or_default());
        } else {
            builder.set_result("");
        }

        builder.set_error(self.error.as_deref().unwrap_or(""));
        builder.set_created_at(self.created_at);
        builder.set_started_at(self.started_at.unwrap_or(0));
        builder.set_completed_at(self.completed_at.unwrap_or(0));
    }

    /// Read from Cap'n Proto reader after receiving from wire
    pub fn from_capnp(reader: jobs_capnp::job_info::Reader) -> capnp::Result<Self> {
        // Get text fields (capnp text::Reader needs to_str()? conversion)
        let job_id_str = reader.get_job_id()?.to_str()?;
        let source_str = reader.get_source()?.to_str()?;
        let result_str = reader.get_result()?.to_str()?;
        let error_str = reader.get_error()?.to_str()?;

        let result = if result_str.is_empty() {
            None
        } else {
            serde_json::from_str(result_str).ok()
        };

        let error = if error_str.is_empty() {
            None
        } else {
            Some(error_str.to_string())
        };

        let started = reader.get_started_at();
        let completed = reader.get_completed_at();

        Ok(Self {
            job_id: JobId::from_text(job_id_str),
            status: reader.get_status()?,
            source: source_str.to_string(),
            result,
            error,
            created_at: reader.get_created_at(),
            started_at: if started == 0 { None } else { Some(started) },
            completed_at: if completed == 0 { None } else { Some(completed) },
        })
    }
}

// ============================================================================
// JobStoreStats - Ergonomic wrapper
// ============================================================================

/// Statistics about job store state
#[derive(Debug, Clone, Default)]
pub struct JobStoreStats {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
}

impl JobStoreStats {
    /// Write to Cap'n Proto builder
    pub fn to_capnp<'a>(&self, builder: &mut jobs_capnp::job_store_stats::Builder<'a>) {
        builder.set_total(self.total as u32);
        builder.set_pending(self.pending as u32);
        builder.set_running(self.running as u32);
        builder.set_completed(self.completed as u32);
        builder.set_failed(self.failed as u32);
        builder.set_cancelled(self.cancelled as u32);
    }

    /// Read from Cap'n Proto reader
    pub fn from_capnp(reader: jobs_capnp::job_store_stats::Reader) -> capnp::Result<Self> {
        Ok(Self {
            total: reader.get_total() as usize,
            pending: reader.get_pending() as usize,
            running: reader.get_running() as usize,
            completed: reader.get_completed() as usize,
            failed: reader.get_failed() as usize,
            cancelled: reader.get_cancelled() as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_id_roundtrip() {
        let id = JobId::new();
        let text = id.as_text();
        let recovered = JobId::from_text(text);
        assert_eq!(id, recovered);
    }

    #[test]
    fn job_status_strings() {
        assert_eq!(JobStatus::Pending.to_string_lower(), "pending");
        assert_eq!(JobStatus::Running.to_string_lower(), "running");
        assert_eq!(JobStatus::Complete.to_string_lower(), "complete");

        assert_eq!(JobStatus::from_str_lower("pending"), Some(JobStatus::Pending));
        assert_eq!(JobStatus::from_str_lower("RUNNING"), Some(JobStatus::Running));
        assert_eq!(JobStatus::from_str_lower("invalid"), None);
    }

    #[test]
    fn job_info_capnp_roundtrip() {
        let job_id = JobId::new();
        let mut info = JobInfo::new(job_id.clone(), "test_tool".to_string());
        info.mark_running();
        info.mark_complete(serde_json::json!({"answer": 42}));

        // Write to capnp
        let mut message = capnp::message::Builder::new_default();
        {
            let mut builder = message.init_root::<jobs_capnp::job_info::Builder>();
            info.to_capnp(&mut builder);
        }

        // Read back
        let reader = message.get_root_as_reader::<jobs_capnp::job_info::Reader>().unwrap();
        let recovered = JobInfo::from_capnp(reader).unwrap();

        assert_eq!(recovered.job_id, info.job_id);
        assert_eq!(recovered.status, JobStatus::Complete);
        assert_eq!(recovered.source, "test_tool");
        assert_eq!(recovered.result, Some(serde_json::json!({"answer": 42})));
    }
}
