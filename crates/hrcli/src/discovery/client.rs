use anyhow::{Context, Result};
use serde_json::{json, Value};
use super::schema::*;
use crate::mcp_client::McpClient;

pub struct DiscoveryClient {
    pub server_url: String,
}

impl DiscoveryClient {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
        }
    }

    /// Discover tools from MCP server
    pub async fn discover_tools(&self) -> Result<(Vec<DynamicToolSchema>, ServerCapabilities)> {
        // Connect to MCP server
        let client = McpClient::connect(&self.server_url)
            .await
            .context("Failed to connect to MCP server")?;

        // Use standard MCP discovery with metadata from annotations
        self.standard_discovery(&client).await
    }

    /// Standard MCP discovery with metadata from annotations
    async fn standard_discovery(&self, client: &McpClient) -> Result<(Vec<DynamicToolSchema>, ServerCapabilities)> {
        // Get basic tool list
        let tools = client.list_tools().await?;

        // Convert to dynamic schemas
        let mut schemas = Vec::new();
        for tool in tools {
            // Extract metadata from MCP annotations or infer from tool name
            let metadata = self.extract_metadata_from_tool(&tool);

            let schema = DynamicToolSchema {
                name: tool.name.clone(),
                description: tool.description.clone(),
                metadata,
                input_schema: self.parse_input_schema(&tool),
                output_schema: None,  // Not available in standard MCP discovery
                parameter_handlers: self.infer_handlers(&tool),
                output_format: self.infer_output_format(&tool.name),
            };

            schemas.push(schema);
        }

        // Basic server capabilities for standard MCP
        let capabilities = ServerCapabilities {
            version: "standard".to_string(),
            parameter_handlers: vec!["simple".to_string()],
            output_formats: vec!["json".to_string()],
            extensions: HashMap::new(),
            supports_streaming: false,
            supports_batch: false,
            custom: HashMap::new(),
        };

        Ok((schemas, capabilities))
    }

    /// Extract metadata from MCP tool annotations or infer from name
    fn extract_metadata_from_tool(&self, tool: &crate::mcp_client::ToolInfo) -> ToolMetadata {
        // Start with defaults
        let mut metadata = ToolMetadata::default();

        // Extract from annotations if present
        if let Some(annotations) = &tool.annotations {
            metadata.ui_hints.icon = annotations.icon.clone();
            metadata.cli.category = annotations.category.clone();
            metadata.cli.aliases = annotations.aliases.clone();
        }

        // If no icon in annotations, try to infer from tool name prefix
        if metadata.ui_hints.icon.is_none() {
            metadata.ui_hints.icon = Some(self.infer_icon_from_name(&tool.name));
        }

        // If no category in annotations, use default "Tools"
        if metadata.cli.category.is_none() {
            metadata.cli.category = Some("Tools".to_string());
        }

        metadata
    }

    /// Infer icon from tool name prefix
    fn infer_icon_from_name(&self, tool_name: &str) -> String {
        // Infer from prefix patterns
        if tool_name.starts_with("orpheus_") {
            "🎼".to_string()
        } else if tool_name.starts_with("job_") {
            "⚙️".to_string()
        } else if tool_name.starts_with("cas_") {
            "💾".to_string()
        } else if tool_name.starts_with("graph_") {
            "🔗".to_string()
        } else if tool_name.starts_with("abc_") {
            "📝".to_string()
        } else if tool_name.starts_with("soundfont_") {
            "🎹".to_string()
        } else if tool_name.starts_with("artifact_") {
            "🎨".to_string()
        } else if tool_name.starts_with("convert_") {
            "🔄".to_string()
        } else if tool_name.starts_with("beatthis_") {
            "🎯".to_string()
        } else {
            "🔧".to_string()
        }
    }

    fn parse_input_schema(&self, tool: &crate::mcp_client::ToolInfo) -> Option<Value> {
        // Use the real inputSchema from MCP if available
        if let Some(schema) = &tool.input_schema {
            return Some(schema.clone());
        }

        // Fallback: Try to reconstruct schema from parameter string
        // This is for basic MCP servers without proper schemas
        if !tool.parameters.is_empty() {
            let params: Vec<&str> = tool.parameters.split(", ").collect();
            let mut properties = serde_json::Map::new();

            for param in params {
                properties.insert(
                    param.to_string(),
                    json!({
                        "type": "string",
                        "description": format!("Parameter: {}", param)
                    })
                );
            }

            Some(json!({
                "type": "object",
                "properties": properties
            }))
        } else {
            None
        }
    }

    fn infer_handlers(&self, tool: &crate::mcp_client::ToolInfo) -> HashMap<String, ParameterHandler> {
        let mut handlers = HashMap::new();

        // Special handling for known tools
        match tool.name.as_str() {
            "play" => {
                // Emotion could be a composite field
                handlers.insert(
                    "emotion".to_string(),
                    ParameterHandler::Composite {
                        fields: vec![
                            CompositeField {
                                name: "valence".to_string(),
                                description: "Joy-sorrow axis".to_string(),
                                cli_arg: "--valence".to_string(),
                                default: Some(json!(0.0)),
                            },
                            CompositeField {
                                name: "arousal".to_string(),
                                description: "Energy level".to_string(),
                                cli_arg: "--arousal".to_string(),
                                default: Some(json!(0.5)),
                            },
                            CompositeField {
                                name: "agency".to_string(),
                                description: "Leading-following".to_string(),
                                cli_arg: "--agency".to_string(),
                                default: Some(json!(0.0)),
                            },
                        ],
                        combiner: "emotion_vector".to_string(),
                    }
                );
            }
            _ => {}
        }

        handlers
    }

    fn infer_output_format(&self, tool_name: &str) -> OutputFormat {
        match tool_name {
            "get_tree_status" => OutputFormat::Template {
                template: r#"
🌳 Musical Conversation Tree
━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Current Branch: {{branch}}
Total Nodes: {{node_count}}
Active Threads: {{active_threads}}

Recent Events:
{{#each recent_events}}
  {{this.timestamp}} - {{this.description}}
{{/each}}
"#.to_string(),
                colors: {
                    let mut colors = HashMap::new();
                    colors.insert("branch".to_string(), "green".to_string());
                    colors.insert("node_count".to_string(), "yellow".to_string());
                    colors
                },
            },
            "play" => OutputFormat::Template {
                template: r#"
🎵 Musical Event Created
━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Node: #{{node_id}} on branch '{{branch}}'
Content: {{what}} ({{how}})
Emotion: valence={{emotion.valence}}, arousal={{emotion.arousal}}, agency={{emotion.agency}}

Musical Interpretation:
  {{interpretation}}

Suggested Responses:
{{#each suggestions}}
  • {{this}}
{{/each}}
"#.to_string(),
                colors: HashMap::new(),
            },
            _ => OutputFormat::default()
        }
    }
}

use std::collections::HashMap;