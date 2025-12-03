//! Sampling Types
//!
//! Types for server-initiated LLM sampling requests per MCP 2025-06-18 spec.
//! This enables the server to request inference from the connected client's LLM.

use serde::{Deserialize, Serialize};
use super::content::Content;
use super::Role;

/// A message in a sampling request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingMessage {
    pub role: Role,
    pub content: Content,
}

impl SamplingMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: Content::text(text),
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: Content::text(text),
        }
    }
}

/// Model preferences for sampling
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreferences {
    /// Hints for model selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<ModelHint>>,

    /// Priority for model intelligence (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f64>,

    /// Priority for response speed (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f64>,

    /// Priority for cost efficiency (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f64>,
}

/// Hint for model selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHint {
    /// Model name pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ModelHint {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
        }
    }
}

/// Sampling request parameters
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingRequest {
    /// Messages to send
    pub messages: Vec<SamplingMessage>,

    /// Model preferences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,

    /// System prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Include context from MCP servers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_context: Option<IncludeContext>,

    /// Temperature (0.0-2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,

    /// Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// What context to include
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IncludeContext {
    None,
    ThisServer,
    AllServers,
}

/// Sampling response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingResponse {
    /// Role (always assistant)
    pub role: Role,

    /// Response content
    pub content: Content,

    /// Model that generated the response
    pub model: String,

    /// Why the model stopped
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
}

/// Reason the model stopped generating
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    StopSequence,
    MaxTokens,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_message_user() {
        let msg = SamplingMessage::user("Hello");
        assert_eq!(msg.role, Role::User);
    }

    #[test]
    fn test_sampling_request_serialization() {
        let request = SamplingRequest {
            messages: vec![SamplingMessage::user("Hello")],
            max_tokens: Some(100),
            ..Default::default()
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["maxTokens"], 100);
    }

    #[test]
    fn test_model_hint() {
        let hint = ModelHint::new("claude-sonnet");
        assert!(hint.name.is_some());
    }
}
