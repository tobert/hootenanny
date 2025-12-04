//! Elicitation Types
//!
//! Types for server-initiated user input requests.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Elicitation request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationRequest {
    /// Message to display to user
    pub message: String,

    /// JSON Schema for requested input
    pub requested_schema: ElicitationSchema,
}

/// Schema for elicitation (subset of JSON Schema)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationSchema {
    #[serde(rename = "type")]
    pub schema_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<serde_json::Map<String, Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

impl ElicitationSchema {
    /// Create a simple string input
    pub fn string_input(name: &str, description: &str) -> Self {
        let mut props = serde_json::Map::new();
        props.insert(
            name.to_string(),
            serde_json::json!({
                "type": "string",
                "description": description
            }),
        );

        Self {
            schema_type: "object".to_string(),
            properties: Some(props),
            required: Some(vec![name.to_string()]),
        }
    }

    /// Create a choice from enum
    pub fn choice(name: &str, options: &[(&str, &str)]) -> Self {
        let values: Vec<&str> = options.iter().map(|(v, _)| *v).collect();
        let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();

        let mut props = serde_json::Map::new();
        props.insert(
            name.to_string(),
            serde_json::json!({
                "type": "string",
                "enum": values,
                "enumLabels": labels
            }),
        );

        Self {
            schema_type: "object".to_string(),
            properties: Some(props),
            required: Some(vec![name.to_string()]),
        }
    }

    /// Create a boolean confirmation
    pub fn confirm(name: &str) -> Self {
        let mut props = serde_json::Map::new();
        props.insert(
            name.to_string(),
            serde_json::json!({ "type": "boolean" }),
        );

        Self {
            schema_type: "object".to_string(),
            properties: Some(props),
            required: Some(vec![name.to_string()]),
        }
    }
}

/// User's response to elicitation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationResponse {
    /// What the user did
    pub action: ElicitationAction,

    /// The input content (if accepted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
}

/// User action in response to elicitation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElicitationAction {
    /// User provided valid input
    Accept,
    /// User declined to provide input
    Decline,
    /// User cancelled the operation
    Cancel,
}

/// Error when elicitation fails
#[derive(Debug, Clone)]
pub enum ElicitationError {
    /// Client doesn't support elicitation
    NotSupported,
    /// User declined
    Declined,
    /// User cancelled
    Cancelled,
    /// Request timed out
    Timeout,
    /// Channel closed
    ChannelClosed,
}

impl std::fmt::Display for ElicitationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElicitationError::NotSupported => write!(f, "client does not support elicitation"),
            ElicitationError::Declined => write!(f, "user declined"),
            ElicitationError::Cancelled => write!(f, "user cancelled"),
            ElicitationError::Timeout => write!(f, "request timed out"),
            ElicitationError::ChannelClosed => write!(f, "channel closed"),
        }
    }
}

impl std::error::Error for ElicitationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elicitation_schema_choice() {
        let schema = ElicitationSchema::choice("key", &[
            ("C", "C Major"),
            ("Am", "A Minor"),
        ]);

        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["type"], "object");
        let props = &json["properties"]["key"];
        assert_eq!(props["enum"].as_array().unwrap().len(), 2);
        assert_eq!(props["enumLabels"][0], "C Major");
    }

    #[test]
    fn test_elicitation_response_serialization() {
        let response = ElicitationResponse {
            action: ElicitationAction::Accept,
            content: Some(serde_json::json!({"choice": "Am"})),
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["action"], "accept");
        assert_eq!(json["content"]["choice"], "Am");
    }
}
