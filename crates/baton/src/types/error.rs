//! MCP Error Types
//!
//! Error data structures and standard error codes per JSON-RPC 2.0 and MCP spec.
//! Per MCP 2025-06-18 schema lines 909-946.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC error data.
/// Per MCP 2025-06-18 schema lines 912-931.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    /// The error code.
    pub code: i32,

    /// A short description of the error.
    pub message: String,

    /// Additional error data (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ErrorData {
    // JSON-RPC 2.0 standard error codes
    // https://www.jsonrpc.org/specification#error_object

    /// Parse error - Invalid JSON was received.
    pub const PARSE_ERROR: i32 = -32700;

    /// Invalid Request - The JSON sent is not a valid Request object.
    pub const INVALID_REQUEST: i32 = -32600;

    /// Method not found - The method does not exist / is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;

    /// Invalid params - Invalid method parameter(s).
    pub const INVALID_PARAMS: i32 = -32602;

    /// Internal error - Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;

    /// Create a new error with code and message.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Create a new error with additional data.
    pub fn with_data(code: i32, message: impl Into<String>, data: Value) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(data),
        }
    }

    /// Create a parse error.
    pub fn parse_error(message: impl Into<String>) -> Self {
        Self::new(Self::PARSE_ERROR, message)
    }

    /// Create an invalid request error.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(Self::INVALID_REQUEST, message)
    }

    /// Create a method not found error.
    pub fn method_not_found(method: &str) -> Self {
        Self::new(
            Self::METHOD_NOT_FOUND,
            format!("Method not found: {}", method),
        )
    }

    /// Create an invalid params error.
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(Self::INVALID_PARAMS, message)
    }

    /// Create an internal error.
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(Self::INTERNAL_ERROR, message)
    }

    /// Create a tool not found error.
    pub fn tool_not_found(name: &str) -> Self {
        Self::new(
            Self::METHOD_NOT_FOUND,
            format!("Tool not found: {}", name),
        )
    }

    /// Create a resource not found error.
    pub fn resource_not_found(uri: &str) -> Self {
        Self::new(
            Self::INVALID_PARAMS,
            format!("Resource not found: {}", uri),
        )
    }

    /// Create a prompt not found error.
    pub fn prompt_not_found(name: &str) -> Self {
        Self::new(
            Self::INVALID_PARAMS,
            format!("Prompt not found: {}", name),
        )
    }
}

impl std::fmt::Display for ErrorData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for ErrorData {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_error_codes() {
        assert_eq!(ErrorData::PARSE_ERROR, -32700);
        assert_eq!(ErrorData::INVALID_REQUEST, -32600);
        assert_eq!(ErrorData::METHOD_NOT_FOUND, -32601);
        assert_eq!(ErrorData::INVALID_PARAMS, -32602);
        assert_eq!(ErrorData::INTERNAL_ERROR, -32603);
    }

    #[test]
    fn test_error_serialization() {
        let error = ErrorData::method_not_found("foo/bar");
        let json = serde_json::to_value(&error).unwrap();

        assert_eq!(json["code"], -32601);
        assert_eq!(json["message"], "Method not found: foo/bar");
        assert!(json.get("data").is_none());
    }

    #[test]
    fn test_error_with_data() {
        let error = ErrorData::with_data(
            ErrorData::INVALID_PARAMS,
            "Missing required field",
            json!({ "field": "name" }),
        );

        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], -32602);
        assert_eq!(json["data"]["field"], "name");
    }

    #[test]
    fn test_error_roundtrip() {
        let original = ErrorData::internal_error("Something went wrong");
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ErrorData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.code, original.code);
        assert_eq!(parsed.message, original.message);
    }
}
