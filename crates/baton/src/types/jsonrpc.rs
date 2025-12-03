//! JSON-RPC 2.0 Types
//!
//! Base types for JSON-RPC 2.0 protocol as used by MCP.
//! Per MCP 2025-06-18 schema lines 947-1050.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::error::ErrorData;

/// JSON-RPC version constant - always "2.0".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonRpcVersion;

impl Serialize for JsonRpcVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "2.0" {
            Ok(JsonRpcVersion)
        } else {
            Err(serde::de::Error::custom(format!(
                "expected JSON-RPC version '2.0', got '{}'",
                s
            )))
        }
    }
}

impl Default for JsonRpcVersion {
    fn default() -> Self {
        JsonRpcVersion
    }
}

/// Request ID - can be a string or integer.
/// Per MCP 2025-06-18 schema lines 1752-1758.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::Number(n) => write!(f, "{}", n),
            RequestId::String(s) => write!(f, "{}", s),
        }
    }
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        RequestId::Number(n)
    }
}

impl From<String> for RequestId {
    fn from(s: String) -> Self {
        RequestId::String(s)
    }
}

impl From<&str> for RequestId {
    fn from(s: &str) -> Self {
        RequestId::String(s.to_string())
    }
}

/// A JSON-RPC 2.0 request.
/// Per MCP 2025-06-18 schema lines 992-1029.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    /// Create a new request with the given method and optional params.
    pub fn new(id: impl Into<RequestId>, method: impl Into<String>) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id: id.into(),
            method: method.into(),
            params: None,
        }
    }

    /// Create a new request with params.
    pub fn with_params(id: impl Into<RequestId>, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id: id.into(),
            method: method.into(),
            params: Some(params),
        }
    }
}

/// A successful JSON-RPC 2.0 response.
/// Per MCP 2025-06-18 schema lines 1030-1050.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse<T = Value> {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub result: T,
}

impl<T> JsonRpcResponse<T> {
    /// Create a new successful response.
    pub fn success(id: impl Into<RequestId>, result: T) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id: id.into(),
            result,
        }
    }
}

/// A JSON-RPC 2.0 error response.
/// Per MCP 2025-06-18 schema lines 909-946.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub error: ErrorData,
}

impl JsonRpcErrorResponse {
    /// Create a new error response.
    pub fn new(id: impl Into<RequestId>, error: ErrorData) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id: id.into(),
            error,
        }
    }
}

/// A JSON-RPC 2.0 notification (no response expected).
/// Per MCP 2025-06-18 schema lines 964-991.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: JsonRpcVersion,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A JSON-RPC message that could be either a request or notification.
/// Used when receiving messages that may or may not have an id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: JsonRpcVersion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcMessage {
    /// Create a new notification (no id).
    pub fn notification(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            id: None,
            method: method.into(),
            params: Some(params),
        }
    }

    /// Returns true if this is a notification (no id).
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// Returns true if this is a request (has id).
    pub fn is_request(&self) -> bool {
        self.id.is_some()
    }
}

impl JsonRpcNotification {
    /// Create a new notification.
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            method: method.into(),
            params: None,
        }
    }

    /// Create a new notification with params.
    pub fn with_params(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcVersion,
            method: method.into(),
            params: Some(params),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_id_number() {
        let id = RequestId::Number(42);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "42");

        let parsed: RequestId = serde_json::from_str("42").unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn test_request_id_string() {
        let id = RequestId::String("abc-123".to_string());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"abc-123\"");

        let parsed: RequestId = serde_json::from_str("\"abc-123\"").unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn test_request_serialization() {
        let request = JsonRpcRequest::with_params(
            1,
            "tools/call",
            json!({ "name": "hello" }),
        );

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "tools/call");
        assert_eq!(json["params"]["name"], "hello");
    }

    #[test]
    fn test_request_roundtrip() {
        let original = JsonRpcRequest::with_params(
            "req-1",
            "initialize",
            json!({ "protocolVersion": "2025-06-18" }),
        );

        let json = serde_json::to_string(&original).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, RequestId::String("req-1".to_string()));
        assert_eq!(parsed.method, "initialize");
    }

    #[test]
    fn test_response_success() {
        let response = JsonRpcResponse::success(1, json!({ "tools": [] }));

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert!(json["result"]["tools"].is_array());
    }

    #[test]
    fn test_notification() {
        let notif = JsonRpcNotification::new("notifications/initialized");

        let json = serde_json::to_value(&notif).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "notifications/initialized");
        assert!(json.get("id").is_none());
    }
}
