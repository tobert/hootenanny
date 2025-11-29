//! Prompt Types
//!
//! Types for MCP prompt templates and messages.
//! Per MCP 2025-06-18 schema lines 1550-1669.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::content::Content;
use super::Role;

/// A prompt template that the server offers.
/// Per MCP 2025-06-18 schema lines 1550-1582.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    /// Programmatic name of the prompt.
    pub name: String,

    /// Human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description of what this prompt provides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Arguments that can be used to template the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

impl Prompt {
    /// Create a new prompt.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: None,
            arguments: None,
        }
    }

    /// Set the human-readable title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an argument to the prompt.
    pub fn with_argument(mut self, arg: PromptArgument) -> Self {
        self.arguments
            .get_or_insert_with(Vec::new)
            .push(arg);
        self
    }

    /// Add a required argument.
    pub fn argument(
        self,
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        self.with_argument(PromptArgument::new(name, description).required(required))
    }
}

/// An argument that a prompt can accept.
/// Per MCP 2025-06-18 schema lines 1583-1607.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    /// Programmatic name of the argument.
    pub name: String,

    /// Human-readable title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description of the argument.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this argument is required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

impl PromptArgument {
    /// Create a new prompt argument.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: Some(description.into()),
            required: None,
        }
    }

    /// Set the human-readable title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Mark this argument as required or optional.
    pub fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }
}

/// A message in a prompt.
/// Per MCP 2025-06-18 schema lines 1632-1647.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMessage {
    /// Role of the message sender.
    pub role: Role,

    /// Content of the message.
    pub content: Content,
}

impl PromptMessage {
    /// Create a user message.
    pub fn user(content: Content) -> Self {
        Self {
            role: Role::User,
            content,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: Content) -> Self {
        Self {
            role: Role::Assistant,
            content,
        }
    }

    /// Create a user message with text content.
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::user(Content::text(text))
    }

    /// Create an assistant message with text content.
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::assistant(Content::text(text))
    }
}

/// Parameters for prompts/get request.
/// Per MCP 2025-06-18 schema lines 710-742.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptParams {
    /// Name of the prompt to get.
    pub name: String,

    /// Arguments to use for templating.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<HashMap<String, String>>,
}

/// Result of prompts/get request.
/// Per MCP 2025-06-18 schema lines 743-766.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPromptResult {
    /// Optional description of the prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Messages that make up the prompt.
    pub messages: Vec<PromptMessage>,
}

impl GetPromptResult {
    /// Create a result with messages.
    pub fn new(messages: Vec<PromptMessage>) -> Self {
        Self {
            description: None,
            messages,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Result of prompts/list request.
/// Per MCP 2025-06-18 schema lines 1073-1096.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsResult {
    /// Available prompts.
    pub prompts: Vec<Prompt>,

    /// Pagination cursor for next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl ListPromptsResult {
    /// Create a result with all prompts (no pagination).
    pub fn all(prompts: Vec<Prompt>) -> Self {
        Self {
            prompts,
            next_cursor: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_creation() {
        let prompt = Prompt::new("compose")
            .with_title("Compose Music")
            .with_description("Start a new musical composition")
            .argument("style", "Musical style (ambient, jazz, classical)", true)
            .argument("mood", "Emotional mood", false);

        let json = serde_json::to_value(&prompt).unwrap();
        assert_eq!(json["name"], "compose");
        assert_eq!(json["title"], "Compose Music");
        assert_eq!(json["arguments"].as_array().unwrap().len(), 2);
        assert_eq!(json["arguments"][0]["name"], "style");
        assert_eq!(json["arguments"][0]["required"], true);
    }

    #[test]
    fn test_prompt_message() {
        let message = PromptMessage::user_text("What should I play?");

        let json = serde_json::to_value(&message).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"]["type"], "text");
        assert_eq!(json["content"]["text"], "What should I play?");
    }

    #[test]
    fn test_get_prompt_result() {
        let result = GetPromptResult::new(vec![
            PromptMessage::user_text("Compose an ambient piece"),
            PromptMessage::assistant_text("I'll create a peaceful ambient composition..."),
        ])
        .with_description("Ambient composition prompt");

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["messages"].as_array().unwrap().len(), 2);
        assert_eq!(json["description"], "Ambient composition prompt");
    }

    #[test]
    fn test_list_prompts_result() {
        let result = ListPromptsResult::all(vec![
            Prompt::new("compose").with_description("Compose music"),
            Prompt::new("analyze").with_description("Analyze audio"),
        ]);

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["prompts"].as_array().unwrap().len(), 2);
        assert!(json.get("nextCursor").is_none());
    }

    #[test]
    fn test_get_prompt_params() {
        let params = GetPromptParams {
            name: "compose".to_string(),
            arguments: Some(HashMap::from([
                ("style".to_string(), "ambient".to_string()),
            ])),
        };

        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["name"], "compose");
        assert_eq!(json["arguments"]["style"], "ambient");
    }
}
