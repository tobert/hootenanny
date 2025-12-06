//! MCP client re-exports and OpenAI function conversion helpers.

use serde::Serialize;
use serde_json::Value;

// Re-export baton client types
pub use baton::client::{ClientOptions, McpClient as McpToolClient, ToolInfo};

/// OpenAI function definition format.
#[derive(Debug, Clone, Serialize)]
pub struct OpenAiFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

/// Convert MCP tools to OpenAI function format.
pub fn to_openai_functions(tools: &[ToolInfo]) -> Vec<OpenAiFunction> {
    tools
        .iter()
        .map(|tool| OpenAiFunction {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.input_schema.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_openai_functions() {
        let tools = vec![ToolInfo {
            name: "orpheus_generate".to_string(),
            description: Some("Generate MIDI with Orpheus".to_string()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "model": { "type": "string" }
                }
            }),
            annotations: None,
        }];

        let functions = to_openai_functions(&tools);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "orpheus_generate");
    }
}
