//! Tool Types
//!
//! Types for MCP tool definitions and call results.
//! Per MCP 2025-06-18 schema lines 2353-2487.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::content::Content;

/// A tool definition.
/// Per MCP 2025-06-18 schema lines 2353-2437.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// Programmatic name of the tool.
    pub name: String,

    /// Human-readable title (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Description for the LLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// JSON Schema for input parameters.
    pub input_schema: ToolSchema,

    /// JSON Schema for structured output (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<ToolSchema>,

    /// Additional tool annotations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

impl Tool {
    /// Create a new tool with name and description.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: Some(description.into()),
            input_schema: ToolSchema::empty(),
            output_schema: None,
            annotations: None,
        }
    }

    /// Set the human-readable title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the input schema.
    pub fn with_input_schema(mut self, schema: ToolSchema) -> Self {
        self.input_schema = schema;
        self
    }

    /// Set the input schema from a JSON value.
    pub fn with_input_schema_value(mut self, schema: Value) -> Self {
        self.input_schema = ToolSchema::from_value(schema);
        self
    }

    /// Set the output schema.
    pub fn with_output_schema(mut self, schema: ToolSchema) -> Self {
        self.output_schema = Some(schema);
        self
    }

    /// Set tool annotations.
    pub fn with_annotations(mut self, annotations: ToolAnnotations) -> Self {
        self.annotations = Some(annotations);
        self
    }

    /// Mark this tool as read-only (doesn't modify state).
    pub fn read_only(mut self) -> Self {
        self.annotations = Some(
            self.annotations
                .unwrap_or_default()
                .with_read_only(true),
        );
        self
    }

    /// Mark this tool as idempotent.
    pub fn idempotent(mut self) -> Self {
        self.annotations = Some(
            self.annotations
                .unwrap_or_default()
                .with_idempotent(true),
        );
        self
    }

    /// Set CLI icon (emoji or unicode symbol).
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.annotations = Some(
            self.annotations
                .unwrap_or_default()
                .with_icon(icon),
        );
        self
    }

    /// Set CLI category for grouping.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.annotations = Some(
            self.annotations
                .unwrap_or_default()
                .with_category(category),
        );
        self
    }

    /// Add CLI aliases.
    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        self.annotations = Some(
            self.annotations
                .unwrap_or_default()
                .with_aliases(aliases),
        );
        self
    }
}

/// JSON Schema for tool input/output.
/// Per MCP 2025-06-18 schema lines 2369-2395.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Always "object" for tool schemas.
    #[serde(rename = "type")]
    pub schema_type: String,

    /// Property definitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Map<String, Value>>,

    /// Required property names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

impl ToolSchema {
    /// Create an empty schema (no parameters).
    pub fn empty() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: None,
            required: None,
        }
    }

    /// Create a schema from properties.
    pub fn with_properties(properties: Map<String, Value>) -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: Some(properties),
            required: None,
        }
    }

    /// Create a schema from a JSON value.
    pub fn from_value(value: Value) -> Self {
        if let Value::Object(map) = value {
            Self {
                schema_type: map
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("object")
                    .to_string(),
                properties: map.get("properties").and_then(|v| {
                    if let Value::Object(props) = v {
                        Some(props.clone())
                    } else {
                        None
                    }
                }),
                required: map.get("required").and_then(|v| {
                    if let Value::Array(arr) = v {
                        Some(
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect(),
                        )
                    } else {
                        None
                    }
                }),
            }
        } else {
            Self::empty()
        }
    }

    /// Add a required field.
    pub fn with_required(mut self, fields: Vec<String>) -> Self {
        self.required = Some(fields);
        self
    }
}

impl Default for ToolSchema {
    fn default() -> Self {
        Self::empty()
    }
}

/// Tool behavior annotations.
/// Per MCP 2025-06-18 schema lines 2438-2463.
/// Extended with optional CLI metadata (icon, category, aliases).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// Human-readable title for the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// If true, the tool doesn't modify state. Default: false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,

    /// If true, the tool may perform destructive updates. Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,

    /// If true, repeated calls have no additional effect. Default: false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,

    /// If true, the tool interacts with external entities. Default: true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,

    /// CLI metadata: icon (emoji or unicode symbol).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// CLI metadata: category for grouping in help output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// CLI metadata: command aliases.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

impl ToolAnnotations {
    /// Set read-only hint.
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only_hint = Some(read_only);
        self
    }

    /// Set destructive hint.
    pub fn with_destructive(mut self, destructive: bool) -> Self {
        self.destructive_hint = Some(destructive);
        self
    }

    /// Set idempotent hint.
    pub fn with_idempotent(mut self, idempotent: bool) -> Self {
        self.idempotent_hint = Some(idempotent);
        self
    }

    /// Set open-world hint.
    pub fn with_open_world(mut self, open_world: bool) -> Self {
        self.open_world_hint = Some(open_world);
        self
    }

    /// Set CLI icon (emoji or unicode symbol).
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set CLI category for grouping.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Add CLI aliases.
    pub fn with_aliases(mut self, aliases: Vec<String>) -> Self {
        self.aliases = aliases;
        self
    }
}

/// Parameters for tools/call request.
/// Per MCP 2025-06-18 schema lines 126-154.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    /// Name of the tool to call.
    pub name: String,

    /// Arguments to pass to the tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Map<String, Value>>,
}

/// Result of a tool call.
/// Per MCP 2025-06-18 schema lines 155-184.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    /// Content blocks representing the result.
    pub content: Vec<Content>,

    /// Whether the tool call resulted in an error.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,

    /// Structured content (if tool defines outputSchema).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
}

impl CallToolResult {
    /// Create a successful result with content.
    pub fn success(content: Vec<Content>) -> Self {
        Self {
            content,
            is_error: false,
            structured_content: None,
        }
    }

    /// Create a successful result with a single text content.
    pub fn text(text: impl Into<String>) -> Self {
        Self::success(vec![Content::text(text)])
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(message)],
            is_error: true,
            structured_content: None,
        }
    }

    /// Add structured content.
    pub fn with_structured(mut self, value: Value) -> Self {
        self.structured_content = Some(value);
        self
    }
}

/// Result of tools/list request.
/// Per MCP 2025-06-18 schema lines 1261-1284.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    /// Available tools.
    pub tools: Vec<Tool>,

    /// Pagination cursor for next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl ListToolsResult {
    /// Create a result with all tools (no pagination).
    pub fn all(tools: Vec<Tool>) -> Self {
        Self {
            tools,
            next_cursor: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_creation() {
        let tool = Tool::new("hello", "Say hello to someone")
            .with_title("Hello Tool")
            .read_only();

        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "hello");
        assert_eq!(json["title"], "Hello Tool");
        assert_eq!(json["description"], "Say hello to someone");
        assert_eq!(json["inputSchema"]["type"], "object");
        assert_eq!(json["annotations"]["readOnlyHint"], true);
    }

    #[test]
    fn test_tool_schema() {
        let schema = ToolSchema::from_value(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        }));

        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.is_some());
        assert_eq!(schema.required, Some(vec!["name".to_string()]));
    }

    #[test]
    fn test_call_tool_result_success() {
        let result = CallToolResult::text("Hello, World!");

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "Hello, World!");
        assert!(json.get("isError").is_none()); // false is skipped
    }

    #[test]
    fn test_call_tool_result_error() {
        let result = CallToolResult::error("Something went wrong");

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["isError"], true);
        assert_eq!(json["content"][0]["text"], "Something went wrong");
    }

    #[test]
    fn test_list_tools_result() {
        let result = ListToolsResult::all(vec![
            Tool::new("foo", "Foo tool"),
            Tool::new("bar", "Bar tool"),
        ]);

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["tools"].as_array().unwrap().len(), 2);
        assert!(json.get("nextCursor").is_none());
    }
}
