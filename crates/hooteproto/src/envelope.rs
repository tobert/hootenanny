//! Response envelope and typed errors for the hooteproto protocol.
//!
//! The envelope wraps tool responses with timing/job semantics.
//! Errors are typed by category for better handling.

use crate::responses::ToolResponse;
use crate::timing::ToolTiming;
use serde::{Deserialize, Serialize};

/// Response envelope - wraps tool responses with protocol semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseEnvelope {
    /// Immediate success with typed response
    Success { response: ToolResponse },

    /// Async job started - poll for result
    JobStarted {
        job_id: String,
        tool: String,
        timing: ToolTiming,
    },

    /// Error occurred
    Error(ToolError),

    /// Simple acknowledgement (for fire-and-forget)
    Ack { message: String },

    // === Streaming (future) ===
    // StreamStart { stream_id: String, tool: String },
    // StreamChunk { stream_id: String, chunk: StreamChunk },
    // StreamEnd { stream_id: String, final_response: Option<ToolResponse> },
}

impl ResponseEnvelope {
    /// Create a success envelope
    pub fn success(response: ToolResponse) -> Self {
        Self::Success { response }
    }

    /// Create a job started envelope
    pub fn job_started(job_id: impl Into<String>, tool: impl Into<String>, timing: ToolTiming) -> Self {
        Self::JobStarted {
            job_id: job_id.into(),
            tool: tool.into(),
            timing,
        }
    }

    /// Create an error envelope
    pub fn error(err: ToolError) -> Self {
        Self::Error(err)
    }

    /// Create an ack envelope
    pub fn ack(message: impl Into<String>) -> Self {
        Self::Ack {
            message: message.into(),
        }
    }

    /// Convert to JSON for gateway edge
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|e| {
            serde_json::json!({
                "kind": "error",
                "error": {
                    "category": "internal",
                    "code": "serialization_failed",
                    "message": e.to_string()
                }
            })
        })
    }
}

/// Typed errors by category
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum ToolError {
    /// Invalid request parameters
    Validation(ValidationError),

    /// Resource not found (CAS hash, artifact, job, etc.)
    NotFound(NotFoundError),

    /// External service failure (Orpheus, MusicGen, etc.)
    Service(ServiceError),

    /// Internal error (should not happen)
    Internal(InternalError),

    /// Operation cancelled
    Cancelled(CancelledError),

    /// Timeout waiting for result
    Timeout(TimeoutError),

    /// Permission denied
    Permission(PermissionError),
}

impl ToolError {
    /// Create a validation error
    pub fn validation(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation(ValidationError {
            code: code.into(),
            message: message.into(),
            field: None,
        })
    }

    /// Create a validation error with field
    pub fn validation_field(
        code: impl Into<String>,
        message: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        Self::Validation(ValidationError {
            code: code.into(),
            message: message.into(),
            field: Some(field.into()),
        })
    }

    /// Create a not found error
    pub fn not_found(resource_type: impl Into<String>, resource_id: impl Into<String>) -> Self {
        Self::NotFound(NotFoundError {
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
        })
    }

    /// Create a service error
    pub fn service(
        service: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Service(ServiceError {
            service: service.into(),
            code: code.into(),
            message: message.into(),
            retryable: false,
        })
    }

    /// Create a retryable service error
    pub fn service_retryable(
        service: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Service(ServiceError {
            service: service.into(),
            code: code.into(),
            message: message.into(),
            retryable: true,
        })
    }

    /// Create an internal error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(InternalError {
            message: message.into(),
            details: None,
        })
    }

    /// Create an internal error with details
    pub fn internal_with_details(message: impl Into<String>, details: impl Into<String>) -> Self {
        Self::Internal(InternalError {
            message: message.into(),
            details: Some(details.into()),
        })
    }

    /// Create a cancelled error
    pub fn cancelled(reason: impl Into<String>) -> Self {
        Self::Cancelled(CancelledError {
            reason: reason.into(),
        })
    }

    /// Create a timeout error
    pub fn timeout(operation: impl Into<String>, timeout_ms: u64) -> Self {
        Self::Timeout(TimeoutError {
            operation: operation.into(),
            timeout_ms,
        })
    }

    /// Create a permission error
    pub fn permission(action: impl Into<String>, resource: impl Into<String>) -> Self {
        Self::Permission(PermissionError {
            action: action.into(),
            resource: resource.into(),
        })
    }

    /// Get a human-readable message
    pub fn message(&self) -> String {
        match self {
            Self::Validation(e) => e.message.clone(),
            Self::NotFound(e) => format!("{} not found: {}", e.resource_type, e.resource_id),
            Self::Service(e) => format!("{}: {}", e.service, e.message),
            Self::Internal(e) => e.message.clone(),
            Self::Cancelled(e) => format!("Cancelled: {}", e.reason),
            Self::Timeout(e) => format!("Timeout after {}ms: {}", e.timeout_ms, e.operation),
            Self::Permission(e) => format!("Permission denied: {} on {}", e.action, e.resource),
        }
    }

    /// Get a code for programmatic handling
    pub fn code(&self) -> &str {
        match self {
            Self::Validation(e) => &e.code,
            Self::NotFound(_) => "not_found",
            Self::Service(e) => &e.code,
            Self::Internal(_) => "internal_error",
            Self::Cancelled(_) => "cancelled",
            Self::Timeout(_) => "timeout",
            Self::Permission(_) => "permission_denied",
        }
    }
}

/// Validation error details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
}

/// Not found error details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotFoundError {
    pub resource_type: String,
    pub resource_id: String,
}

/// External service error details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceError {
    pub service: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

/// Internal error details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InternalError {
    pub message: String,
    pub details: Option<String>,
}

/// Cancelled error details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CancelledError {
    pub reason: String,
}

/// Timeout error details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeoutError {
    pub operation: String,
    pub timeout_ms: u64,
}

/// Permission error details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionError {
    pub action: String,
    pub resource: String,
}

// Conversion from anyhow::Error for convenience
impl From<anyhow::Error> for ToolError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(e.to_string())
    }
}

// Conversion from std::io::Error
impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(format!("IO error: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_serialization() {
        let err = ToolError::not_found("artifact", "artifact_abc123");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("not_found"));
        assert!(json.contains("artifact_abc123"));

        let err2: ToolError = serde_json::from_str(&json).unwrap();
        assert_eq!(err, err2);
    }

    #[test]
    fn envelope_serialization() {
        let env = ResponseEnvelope::job_started("job_123", "orpheus_generate", ToolTiming::AsyncMedium);
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("job_started"));
        assert!(json.contains("job_123"));

        let env2: ResponseEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env, env2);
    }
}
