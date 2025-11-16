use anyhow::{Context, Result};
use std::time::Duration;
use serde_json::{json, Value};
use super::schema::*;
use crate::mcp_client::McpClient;

pub struct DiscoveryClient {
    pub server_url: String,
    pub timeout: Duration,
    pub capabilities: Vec<String>,
}

impl DiscoveryClient {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            timeout: Duration::from_secs(5),
            capabilities: vec![
                "extended_schemas".to_string(),
                "parameter_handlers".to_string(),
                "output_formats".to_string(),
                "ui_hints".to_string(),
                "interactive_prompts".to_string(),
                "batch_operations".to_string(),
                "streaming".to_string(),
            ],
        }
    }

    /// Discover tools with extended metadata if available
    pub async fn discover_tools(&self) -> Result<(Vec<DynamicToolSchema>, ServerCapabilities)> {
        // Connect to MCP server
        let client = McpClient::connect(&self.server_url)
            .await
            .context("Failed to connect to MCP server")?;

        // Try to get extended schemas first
        match self.try_extended_discovery(&client).await {
            Ok((schemas, caps)) => Ok((schemas, caps)),
            Err(_) => {
                // Fallback to standard MCP discovery
                self.standard_discovery(&client).await
            }
        }
    }

    /// Try to discover using extended protocol
    async fn try_extended_discovery(&self, client: &McpClient) -> Result<(Vec<DynamicToolSchema>, ServerCapabilities)> {
        // Check if server supports extended schemas
        let probe_request = json!({
            "method": "hrcli/probe",
            "params": {
                "version": env!("CARGO_PKG_VERSION"),
                "capabilities": self.capabilities
            }
        });

        // Try custom method to check for extended support
        let probe_response = client.call_custom("hrcli/probe", probe_request).await?;

        if probe_response.get("supported").and_then(|s| s.as_bool()).unwrap_or(false) {
            // Server supports extended protocol!
            self.fetch_extended_schemas(client).await
        } else {
            // Server doesn't support extended protocol
            self.standard_discovery(client).await
        }
    }

    /// Fetch extended schemas from server
    async fn fetch_extended_schemas(&self, client: &McpClient) -> Result<(Vec<DynamicToolSchema>, ServerCapabilities)> {
        let request = json!({
            "version": env!("CARGO_PKG_VERSION"),
            "capabilities": self.capabilities,
            "include": {
                "metadata": true,
                "parameter_handlers": true,
                "output_formats": true,
                "ui_hints": true,
                "examples": true
            }
        });

        let response = client.call_custom("hrcli/list_tools_extended", request).await?;

        // Parse extended response
        let extended: ExtendedSchemaResponse = serde_json::from_value(response)
            .context("Failed to parse extended schema response")?;

        Ok((extended.schemas, extended.server_capabilities))
    }

    /// Standard MCP discovery with inference
    async fn standard_discovery(&self, client: &McpClient) -> Result<(Vec<DynamicToolSchema>, ServerCapabilities)> {
        // Get basic tool list
        let tools = client.list_tools().await?;

        // Convert to dynamic schemas with inference
        let mut schemas = Vec::new();
        for tool in tools {
            // Try to get tool-specific metadata
            let metadata = self.try_fetch_tool_metadata(client, &tool.name).await
                .unwrap_or_default();

            let schema = DynamicToolSchema {
                name: tool.name.clone(),
                description: tool.description.clone(),
                metadata,
                input_schema: self.parse_input_schema(&tool),
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

    /// Try to fetch tool-specific metadata
    async fn try_fetch_tool_metadata(&self, client: &McpClient, tool_name: &str) -> Result<ToolMetadata> {
        // Try custom method for tool metadata
        let request = json!({
            "tool": tool_name,
            "include_all": true
        });

        match client.call_custom("hrcli/get_tool_metadata", request).await {
            Ok(response) => {
                serde_json::from_value(response)
                    .context("Failed to parse tool metadata")
            }
            Err(_) => {
                // No metadata available, use defaults
                Ok(self.infer_metadata(tool_name))
            }
        }
    }

    /// Infer metadata from tool name and description
    fn infer_metadata(&self, tool_name: &str) -> ToolMetadata {
        let help = match tool_name {
            "play" => HelpTexts {
                brief: Some("Play a musical event".to_string()),
                detailed: Some("Express musical ideas in the conversation tree".to_string()),
                usage: Some("Use to add musical utterances".to_string()),
                human_context: Some("Creates a musical moment you can hear".to_string()),
                ai_context: Some("Maps abstract concepts to concrete sounds".to_string()),
                see_also: vec!["add_node".to_string(), "fork_branch".to_string()],
            },
            "fork_branch" => HelpTexts {
                brief: Some("Create alternate timeline".to_string()),
                detailed: Some("Branch the conversation to explore alternatives".to_string()),
                usage: Some("Use to try different musical directions".to_string()),
                human_context: Some("Like git branching for music".to_string()),
                ai_context: Some("Enables parallel exploration of possibilities".to_string()),
                see_also: vec!["play".to_string(), "get_tree_status".to_string()],
            },
            _ => HelpTexts::default(),
        };

        let ui_hints = UiHints {
            icon: match tool_name {
                "play" => Some("🎵".to_string()),
                "fork_branch" => Some("🔱".to_string()),
                "add_node" => Some("🌳".to_string()),
                "get_tree_status" => Some("📊".to_string()),
                _ => Some("🔧".to_string()),
            },
            color: match tool_name {
                "play" => Some("cyan".to_string()),
                "fork_branch" => Some("yellow".to_string()),
                _ => None,
            },
            ..Default::default()
        };

        ToolMetadata {
            help,
            ui_hints,
            ..Default::default()
        }
    }

    fn parse_input_schema(&self, tool: &crate::mcp_client::ToolInfo) -> Option<Value> {
        // Try to reconstruct schema from parameter string
        // This is a fallback for basic MCP servers
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

// Extension trait for McpClient to support custom methods
impl McpClient {
    /// Call a custom MCP method (for extended protocol)
    pub async fn call_custom(&self, method: &str, params: Value) -> Result<Value> {
        // This would need to be implemented in mcp_client.rs
        // For now, return an error to trigger fallback
        Err(anyhow::anyhow!("Custom methods not yet implemented"))
    }
}