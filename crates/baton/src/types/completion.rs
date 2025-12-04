//! Completion Types
//!
//! Types for argument autocompletion.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};

/// Reference to what we're completing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CompletionRef {
    /// Completing a prompt argument
    #[serde(rename = "ref/prompt")]
    Prompt {
        /// Prompt name
        name: String,
    },

    /// Completing a resource URI
    #[serde(rename = "ref/resource")]
    Resource {
        /// Resource URI (may be partial)
        uri: String,
    },

    /// Completing a tool argument
    #[serde(rename = "ref/argument")]
    Argument {
        /// Tool name
        name: String,
        /// Argument being completed
        #[serde(rename = "argumentName")]
        argument_name: String,
    },
}

/// The argument being completed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionArgument {
    /// Argument name
    pub name: String,
    /// Current partial value
    pub value: String,
}

/// Completion request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteParams {
    /// What we're completing
    #[serde(rename = "ref")]
    pub reference: CompletionRef,
    /// The argument (for argument completions)
    pub argument: CompletionArgument,
}

/// Completion result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResult {
    /// Suggested completions
    pub values: Vec<String>,
    /// Total number of matches (may be > values.len())
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    /// Whether there are more completions available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

impl CompletionResult {
    pub fn new(values: Vec<String>) -> Self {
        Self {
            total: Some(values.len()),
            has_more: Some(false),
            values,
        }
    }

    pub fn empty() -> Self {
        Self {
            values: vec![],
            total: Some(0),
            has_more: Some(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_ref_argument_serialization() {
        let ref_arg = CompletionRef::Argument {
            name: "orpheus_generate".to_string(),
            argument_name: "model".to_string(),
        };
        let json = serde_json::to_value(&ref_arg).unwrap();
        assert_eq!(json["type"], "ref/argument");
        assert_eq!(json["name"], "orpheus_generate");
        assert_eq!(json["argumentName"], "model");
    }

    #[test]
    fn test_completion_ref_prompt_serialization() {
        let ref_prompt = CompletionRef::Prompt {
            name: "ensemble-jam".to_string(),
        };
        let json = serde_json::to_value(&ref_prompt).unwrap();
        assert_eq!(json["type"], "ref/prompt");
        assert_eq!(json["name"], "ensemble-jam");
    }

    #[test]
    fn test_completion_ref_resource_serialization() {
        let ref_resource = CompletionRef::Resource {
            uri: "artifacts://".to_string(),
        };
        let json = serde_json::to_value(&ref_resource).unwrap();
        assert_eq!(json["type"], "ref/resource");
        assert_eq!(json["uri"], "artifacts://");
    }

    #[test]
    fn test_completion_result_new() {
        let result = CompletionResult::new(vec!["base".to_string(), "bridge".to_string()]);
        assert_eq!(result.values.len(), 2);
        assert_eq!(result.total, Some(2));
        assert_eq!(result.has_more, Some(false));
    }

    #[test]
    fn test_completion_result_empty() {
        let result = CompletionResult::empty();
        assert!(result.values.is_empty());
        assert_eq!(result.total, Some(0));
        assert_eq!(result.has_more, Some(false));
    }
}
