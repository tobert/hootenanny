use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Dynamic tool schema - everything comes from the server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicToolSchema {
    /// Core MCP tool info
    pub name: String,
    pub description: String,

    /// Extended metadata from server (if available)
    #[serde(default)]
    pub metadata: ToolMetadata,

    /// Raw parameter schema from MCP
    #[serde(rename = "inputSchema")]
    pub input_schema: Option<Value>,

    /// Server-provided parameter handlers
    #[serde(default)]
    pub parameter_handlers: HashMap<String, ParameterHandler>,

    /// Server-provided output formatter
    #[serde(default)]
    pub output_format: OutputFormat,
}

/// Extended tool metadata provided by server
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// Help text for different audiences
    #[serde(default)]
    pub help: HelpTexts,

    /// CLI-specific configuration
    #[serde(default)]
    pub cli: CliConfig,

    /// Examples from the server
    #[serde(default)]
    pub examples: Vec<ServerExample>,

    /// Custom behaviors
    #[serde(default)]
    pub behaviors: HashMap<String, Value>,

    /// UI hints for the CLI
    #[serde(default)]
    pub ui_hints: UiHints,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HelpTexts {
    pub brief: Option<String>,
    pub detailed: Option<String>,
    pub usage: Option<String>,
    pub human_context: Option<String>,
    pub ai_context: Option<String>,
    pub see_also: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CliConfig {
    /// Custom command aliases
    pub aliases: Vec<String>,

    /// Whether this tool should be hidden from help
    pub hidden: bool,

    /// Category for grouping in help
    pub category: Option<String>,

    /// Priority for sorting (higher = earlier)
    pub priority: i32,

    /// Whether to allow stdin input
    pub allow_stdin: bool,

    /// Custom argument style
    pub arg_style: Option<ArgStyle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgStyle {
    /// Standard --key value
    LongForm,
    /// Short -k value
    ShortForm,
    /// Positional arguments
    Positional,
    /// Subcommand style
    Subcommand,
    /// Custom format defined by server
    Custom(String),
}

/// How a parameter should be handled by the CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParameterHandler {
    /// Simple value passthrough
    Simple {
        #[serde(default)]
        validator: Option<String>, // Regex or validation rule
        #[serde(default)]
        transform: Option<String>, // Transform expression
    },

    /// Multiple CLI args map to one parameter
    Composite {
        fields: Vec<CompositeField>,
        #[serde(default)]
        combiner: String, // How to combine fields
    },

    /// Interactive prompt for value
    Interactive {
        prompt: String,
        #[serde(default)]
        choices: Vec<Choice>,
        #[serde(default)]
        multi_select: bool,
    },

    /// File path with validation
    FilePath {
        must_exist: bool,
        #[serde(default)]
        extensions: Vec<String>,
        #[serde(default)]
        base_dir: Option<String>,
    },

    /// Environment variable fallback
    Environment {
        var_name: String,
        #[serde(default)]
        required: bool,
    },

    /// Custom handler defined by server
    Custom {
        handler_type: String,
        config: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeField {
    pub name: String,
    pub description: String,
    pub cli_arg: String, // How it appears in CLI
    #[serde(default)]
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub value: Value,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// How to format the output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OutputFormat {
    /// Plain text
    #[serde(rename = "plain")]
    Plain,

    /// JSON output
    #[serde(rename = "json")]
    Json {
        #[serde(default)]
        pretty: bool,
    },

    /// Table format
    #[serde(rename = "table")]
    Table {
        columns: Vec<ColumnDef>,
    },

    /// Custom template
    #[serde(rename = "template")]
    Template {
        template: String,
        #[serde(default)]
        colors: HashMap<String, String>,
    },

    /// Server-provided formatter
    #[serde(rename = "custom")]
    Custom {
        formatter: String,
        config: Value,
    },
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Json { pretty: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub key: String,
    pub header: String,
    #[serde(default)]
    pub width: Option<usize>,
    #[serde(default)]
    pub align: Alignment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    Left,
    Right,
    Center,
}

impl Default for Alignment {
    fn default() -> Self {
        Alignment::Left
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiHints {
    /// Icon to display (emoji or unicode)
    pub icon: Option<String>,

    /// Color theme
    pub color: Option<String>,

    /// Whether to show progress bar
    pub show_progress: bool,

    /// Whether to confirm before execution
    pub confirm: bool,

    /// Spinner style for long operations
    pub spinner: Option<String>,

    /// Custom UI elements
    pub custom: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerExample {
    pub command: String,
    pub description: String,
    #[serde(default)]
    pub output: Option<String>,
}

/// MCP Extension: Response with extended schemas
#[derive(Debug, Serialize, Deserialize)]
pub struct ExtendedSchemaResponse {
    pub schemas: Vec<DynamicToolSchema>,
    pub server_capabilities: ServerCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Server version
    pub version: String,

    /// Supported parameter handler types
    pub parameter_handlers: Vec<String>,

    /// Supported output formats
    pub output_formats: Vec<String>,

    /// Custom extensions
    pub extensions: HashMap<String, Value>,

    /// Whether server supports streaming
    pub supports_streaming: bool,

    /// Whether server supports batch operations
    pub supports_batch: bool,

    /// Custom capabilities
    pub custom: HashMap<String, Value>,
}

impl DynamicToolSchema {
    /// Extract parameter info for CLI building
    pub fn extract_parameters(&self) -> Vec<ParameterInfo> {
        let mut params = Vec::new();

        if let Some(schema) = &self.input_schema {
            if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                let required = schema.get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                for (name, spec) in props {
                    // Check if server provided a custom handler
                    let handler = self.parameter_handlers.get(name).cloned()
                        .unwrap_or_else(|| Self::infer_handler(name, spec));

                    params.push(ParameterInfo {
                        name: name.clone(),
                        spec: spec.clone(),
                        required: required.contains(&name.as_str()),
                        handler,
                    });
                }
            }
        }

        params
    }

    fn infer_handler(_name: &str, spec: &Value) -> ParameterHandler {
        // Basic inference if server doesn't provide handler
        if let Some(type_str) = spec.get("type").and_then(|t| t.as_str()) {
            match type_str {
                "boolean" => ParameterHandler::Simple {
                    validator: Some("true|false".to_string()),
                    transform: None,
                },
                "integer" | "number" => ParameterHandler::Simple {
                    validator: spec.get("pattern")
                        .and_then(|p| p.as_str())
                        .map(String::from),
                    transform: None,
                },
                _ => ParameterHandler::Simple {
                    validator: None,
                    transform: None,
                }
            }
        } else {
            ParameterHandler::Simple {
                validator: None,
                transform: None,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub spec: Value,
    pub required: bool,
    pub handler: ParameterHandler,
}