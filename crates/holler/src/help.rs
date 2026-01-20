//! Help tool implementation for MCP tool discovery.
//!
//! Provides tool index, category browsing, and detailed tool help.

use crate::tools_registry::list_tools;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool categories for organization
const CATEGORIES: &[(&str, &[&str])] = &[
    ("generation", &["orpheus_generate", "orpheus_continue", "orpheus_bridge", "musicgen_generate", "yue_generate"]),
    ("abc", &["abc_validate", "abc_to_midi"]),
    ("analysis", &["beats_detect", "audio_analyze", "midi_classify", "midi_info"]),
    ("rendering", &["soundfont_inspect", "midi_render"]),
    ("playback", &["play", "pause", "stop", "seek", "tempo", "garden_query"]),
    ("timeline", &["timeline_region_create", "timeline_region_move", "timeline_region_delete", "timeline_region_list", "timeline_clear"]),
    ("audio", &["audio_output_attach", "audio_output_detach", "audio_output_status", "audio_input_attach", "audio_input_detach", "audio_input_status", "audio_monitor"]),
    ("artifacts", &["artifact_list", "artifact_get", "artifact_upload"]),
    ("jobs", &["job_poll", "job_cancel", "job_list"]),
    ("system", &["status", "config", "storage_stats", "event_poll"]),
    ("kernel", &["kernel_eval", "kernel_session", "kernel_reset"]),
    ("graph", &["graph_bind", "graph_tag", "graph_connect", "graph_find", "graph_query", "graph_context"]),
    ("help", &["help"]),
];

/// Help request arguments
#[derive(Debug, Default, Deserialize)]
pub struct HelpArgs {
    /// Specific tool name to get help for
    pub tool: Option<String>,
    /// Category to list tools for
    pub category: Option<String>,
}

/// Help response
#[derive(Debug, Serialize)]
pub struct HelpResponse {
    /// Error message if tool/category not found
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Suggestion for similar tool name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    /// Tool details (when requesting specific tool)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolHelp>,
    /// Category details (when requesting category)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<CategoryHelp>,
    /// Full category index (when no specific request)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<HashMap<String, Vec<String>>>,
    /// Total tool count
    pub tool_count: usize,
}

/// Detailed help for a single tool
#[derive(Debug, Serialize)]
pub struct ToolHelp {
    pub name: String,
    pub description: String,
    pub category: String,
    pub parameters: serde_json::Value,
}

/// Help for a category
#[derive(Debug, Serialize)]
pub struct CategoryHelp {
    pub name: String,
    pub tools: Vec<ToolSummary>,
}

/// Brief tool summary for category listings
#[derive(Debug, Serialize)]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
}

/// Process help request and return response
pub fn help(args: HelpArgs) -> HelpResponse {
    let tools = list_tools();
    let tool_count = tools.len();

    match (args.tool, args.category) {
        // Specific tool requested
        (Some(tool_name), _) => {
            if let Some(tool_info) = tools.iter().find(|t| t.name == tool_name) {
                let category = find_category(&tool_name).unwrap_or("unknown").to_string();
                HelpResponse {
                    error: None,
                    suggestion: None,
                    tool: Some(ToolHelp {
                        name: tool_info.name.clone(),
                        description: tool_info.description.clone(),
                        category,
                        parameters: tool_info.input_schema.clone(),
                    }),
                    category: None,
                    categories: None,
                    tool_count,
                }
            } else {
                // Tool not found - suggest similar and return index
                let suggestion = find_similar_tool(&tool_name, &tools);
                HelpResponse {
                    error: Some(format!("Tool '{}' not found", tool_name)),
                    suggestion,
                    tool: None,
                    category: None,
                    categories: Some(build_category_index()),
                    tool_count,
                }
            }
        }

        // Category requested
        (None, Some(cat_name)) => {
            if let Some((_, tool_names)) = CATEGORIES.iter().find(|(name, _)| *name == cat_name) {
                let tool_summaries: Vec<ToolSummary> = tool_names
                    .iter()
                    .filter_map(|name| {
                        tools.iter().find(|t| &t.name == name).map(|t| ToolSummary {
                            name: t.name.clone(),
                            description: t.description.clone(),
                        })
                    })
                    .collect();

                HelpResponse {
                    error: None,
                    suggestion: None,
                    tool: None,
                    category: Some(CategoryHelp {
                        name: cat_name,
                        tools: tool_summaries,
                    }),
                    categories: None,
                    tool_count,
                }
            } else {
                // Category not found - return index
                let available: Vec<&str> = CATEGORIES.iter().map(|(name, _)| *name).collect();
                HelpResponse {
                    error: Some(format!(
                        "Category '{}' not found. Available: {}",
                        cat_name,
                        available.join(", ")
                    )),
                    suggestion: None,
                    tool: None,
                    category: None,
                    categories: Some(build_category_index()),
                    tool_count,
                }
            }
        }

        // No specific request - return full index
        (None, None) => HelpResponse {
            error: None,
            suggestion: None,
            tool: None,
            category: None,
            categories: Some(build_category_index()),
            tool_count,
        },
    }
}

/// Build category index mapping category name to tool names
fn build_category_index() -> HashMap<String, Vec<String>> {
    CATEGORIES
        .iter()
        .map(|(name, tools)| {
            (
                name.to_string(),
                tools.iter().map(|s| s.to_string()).collect(),
            )
        })
        .collect()
}

/// Find which category a tool belongs to
fn find_category(tool_name: &str) -> Option<&'static str> {
    CATEGORIES
        .iter()
        .find(|(_, tools)| tools.contains(&tool_name))
        .map(|(name, _)| *name)
}

/// Find a similar tool name (simple substring match)
fn find_similar_tool(name: &str, tools: &[hooteproto::ToolInfo]) -> Option<String> {
    let name_lower = name.to_lowercase();

    // Try prefix match first
    if let Some(tool) = tools.iter().find(|t| t.name.to_lowercase().starts_with(&name_lower)) {
        return Some(tool.name.clone());
    }

    // Try substring match
    if let Some(tool) = tools.iter().find(|t| t.name.to_lowercase().contains(&name_lower)) {
        return Some(tool.name.clone());
    }

    // Try matching parts separated by underscore
    let parts: Vec<&str> = name_lower.split('_').collect();
    for part in parts {
        if part.len() >= 3 {
            if let Some(tool) = tools.iter().find(|t| t.name.to_lowercase().contains(part)) {
                return Some(tool.name.clone());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_index() {
        let response = help(HelpArgs::default());
        assert!(response.categories.is_some());
        // At least 50 tools (grows as we add more)
        assert!(response.tool_count >= 50, "Expected at least 50 tools, got {}", response.tool_count);
        assert!(response.error.is_none());
    }

    #[test]
    fn test_help_tool() {
        let response = help(HelpArgs {
            tool: Some("play".to_string()),
            category: None,
        });
        assert!(response.tool.is_some());
        let tool = response.tool.unwrap();
        assert_eq!(tool.name, "play");
        assert_eq!(tool.category, "playback");
    }

    #[test]
    fn test_help_category() {
        let response = help(HelpArgs {
            tool: None,
            category: Some("playback".to_string()),
        });
        assert!(response.category.is_some());
        let cat = response.category.unwrap();
        assert_eq!(cat.name, "playback");
        assert!(cat.tools.iter().any(|t| t.name == "play"));
    }

    #[test]
    fn test_help_unknown_tool() {
        let response = help(HelpArgs {
            tool: Some("unknown_tool".to_string()),
            category: None,
        });
        assert!(response.error.is_some());
        assert!(response.categories.is_some()); // Returns index on error
    }

    #[test]
    fn test_help_similar_suggestion() {
        let response = help(HelpArgs {
            tool: Some("pla".to_string()), // partial match for "play"
            category: None,
        });
        assert!(response.error.is_some());
        assert_eq!(response.suggestion, Some("play".to_string()));
    }
}
