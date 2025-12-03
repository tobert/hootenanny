//! Progress Notification Types
//!
//! Per MCP 2025-06-18 specification, progress notifications allow servers to send
//! updates about long-running operations to clients without requiring polling.

use serde::{Deserialize, Serialize};

/// Progress notification parameters sent via `notifications/progress`.
///
/// This notification is sent by the server to update the client on the progress
/// of a long-running operation that was initiated with a `progressToken`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotification {
    /// Token linking this progress update to the original request.
    /// Must match the `progressToken` provided in the request metadata.
    pub progress_token: ProgressToken,

    /// Progress value.
    /// - If `total` is provided: absolute progress (e.g., 50 out of 100)
    /// - If `total` is None: normalized progress from 0.0 to 1.0
    pub progress: f64,

    /// Optional total for absolute progress reporting.
    /// When provided, `progress` is interpreted as an absolute value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,

    /// Human-readable status message describing current operation.
    /// Examples: "Tokenizing MIDI...", "Generating audio...", "Finalizing..."
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ProgressNotification {
    /// Create a progress notification with normalized progress (0.0 to 1.0).
    pub fn normalized(token: ProgressToken, progress: f64, message: impl Into<String>) -> Self {
        Self {
            progress_token: token,
            progress,
            total: None,
            message: Some(message.into()),
        }
    }

    /// Create a progress notification with absolute progress.
    pub fn absolute(
        token: ProgressToken,
        progress: f64,
        total: f64,
        message: impl Into<String>,
    ) -> Self {
        Self {
            progress_token: token,
            progress,
            total: Some(total),
            message: Some(message.into()),
        }
    }
}

/// Progress token identifying a request for progress tracking.
///
/// Per MCP spec, this can be either a string or an integer.
/// Clients include this in the `_meta.progressToken` field of requests
/// to indicate they want progress notifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum ProgressToken {
    String(String),
    Integer(i64),
}

impl std::fmt::Display for ProgressToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProgressToken::String(s) => write!(f, "{}", s),
            ProgressToken::Integer(i) => write!(f, "{}", i),
        }
    }
}

/// Request metadata that can be included in any MCP request.
///
/// The `_meta` field is a reserved namespace for request-level metadata.
/// Currently only `progressToken` is defined.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestMeta {
    /// Optional progress token for tracking long-running operations.
    /// When provided, the server should send `notifications/progress` updates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_token: Option<ProgressToken>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_token_string_serialization() {
        let token = ProgressToken::String("tok-123".to_string());
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, r#""tok-123""#);

        let deserialized: ProgressToken = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, token);
    }

    #[test]
    fn test_progress_token_integer_serialization() {
        let token = ProgressToken::Integer(42);
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "42");

        let deserialized: ProgressToken = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, token);
    }

    #[test]
    fn test_progress_notification_normalized() {
        let notif = ProgressNotification::normalized(
            ProgressToken::String("tok-123".to_string()),
            0.5,
            "Processing...",
        );

        assert_eq!(notif.progress, 0.5);
        assert!(notif.total.is_none());
        assert_eq!(notif.message.as_deref(), Some("Processing..."));

        let json = serde_json::to_value(&notif).unwrap();
        assert_eq!(json["progressToken"], "tok-123");
        assert_eq!(json["progress"], 0.5);
        assert!(json.get("total").is_none());
    }

    #[test]
    fn test_progress_notification_absolute() {
        let notif = ProgressNotification::absolute(
            ProgressToken::Integer(1),
            50.0,
            100.0,
            "Generating...",
        );

        assert_eq!(notif.progress, 50.0);
        assert_eq!(notif.total, Some(100.0));

        let json = serde_json::to_value(&notif).unwrap();
        assert_eq!(json["progressToken"], 1);
        assert_eq!(json["progress"], 50.0);
        assert_eq!(json["total"], 100.0);
    }

    #[test]
    fn test_request_meta_serialization() {
        let meta = RequestMeta {
            progress_token: Some(ProgressToken::String("tok-abc".to_string())),
        };

        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(json["progressToken"], "tok-abc");

        // Empty meta should serialize to empty object
        let empty = RequestMeta::default();
        let json = serde_json::to_value(&empty).unwrap();
        assert_eq!(json, serde_json::json!({}));
    }
}
