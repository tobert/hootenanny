//! MCP Prompts - Query templates for common graph operations
//!
//! Prompts solve the discoverability problem - agents don't need to guess Trustfall syntax.
//! Each prompt provides a pre-built query template with clear argument descriptions.

use rmcp::{
    model::{GetPromptResult, Prompt, PromptArgument, PromptMessage, PromptMessageRole},
    ErrorData as McpError,
};
use std::collections::HashMap;
use tracing::debug;

/// Registry of available MCP prompts.
pub struct PromptRegistry;

impl PromptRegistry {
    /// List all available prompts.
    pub fn list() -> Vec<Prompt> {
        vec![
            Prompt::new(
                "find_lineage",
                Some("Trace an artifact's full ancestry chain using Trustfall"),
                Some(vec![PromptArgument {
                    name: "artifact_id".into(),
                    title: Some("Artifact ID".into()),
                    description: Some("The artifact to trace (e.g., artifact_abc123)".into()),
                    required: Some(true),
                }]),
            ),
            Prompt::new(
                "find_variations",
                Some("Find all variations of an artifact (same variation_set)"),
                Some(vec![PromptArgument {
                    name: "artifact_id".into(),
                    title: Some("Artifact ID".into()),
                    description: Some("Any artifact in the variation set".into()),
                    required: Some(true),
                }]),
            ),
            Prompt::new(
                "search_vibes",
                Some("Search artifacts by vibe/mood annotations"),
                Some(vec![PromptArgument {
                    name: "vibe".into(),
                    title: Some("Vibe".into()),
                    description: Some("Mood to search for (e.g., 'jazzy', 'dark', 'uplifting')".into()),
                    required: Some(true),
                }]),
            ),
            Prompt::new(
                "device_routing",
                Some("Show PipeWire audio device connections and routing"),
                Some(vec![PromptArgument {
                    name: "device_name".into(),
                    title: Some("Device Name".into()),
                    description: Some("Optional device name filter (partial match)".into()),
                    required: Some(false),
                }]),
            ),
            Prompt::new(
                "render_pipeline",
                Some("Generate MIDI, render to audio, and schedule for playback"),
                Some(vec![
                    PromptArgument {
                        name: "style".into(),
                        title: Some("Style".into()),
                        description: Some("Musical style hint for generation".into()),
                        required: Some(false),
                    },
                    PromptArgument {
                        name: "soundfont".into(),
                        title: Some("SoundFont".into()),
                        description: Some("SoundFont hash to use for rendering".into()),
                        required: Some(false),
                    },
                ]),
            ),
        ]
    }

    /// Get a specific prompt by name with filled arguments.
    pub fn get(name: &str, args: &HashMap<String, String>) -> Result<GetPromptResult, McpError> {
        debug!(name = %name, ?args, "Getting prompt");

        match name {
            "find_lineage" => Self::prompt_find_lineage(args),
            "find_variations" => Self::prompt_find_variations(args),
            "search_vibes" => Self::prompt_search_vibes(args),
            "device_routing" => Self::prompt_device_routing(args),
            "render_pipeline" => Self::prompt_render_pipeline(args),
            _ => Err(McpError::invalid_params(
                format!("Unknown prompt: {}", name),
                None,
            )),
        }
    }

    fn prompt_find_lineage(args: &HashMap<String, String>) -> Result<GetPromptResult, McpError> {
        let id = args.get("artifact_id").ok_or_else(|| {
            McpError::invalid_params("artifact_id argument is required", None)
        })?;

        Ok(GetPromptResult {
            description: Some(format!("Trace lineage of artifact {}", id)),
            messages: vec![PromptMessage::new_text(
                PromptMessageRole::User,
                format!(
                    r#"Use the graph_query tool to trace the ancestry of artifact {id}.

Query:
```graphql
{{
  Artifact(id: "{id}") {{
    id @output
    creator @output
    tags @output
    parent {{
      id @output
      creator @output
      tags @output
      parent {{
        id @output
        creator @output
        tags @output
      }}
    }}
  }}
}}
```

This will show the artifact and up to 2 generations of ancestors. Adjust the nesting depth if you need more history."#,
                    id = id
                ),
            )],
        })
    }

    fn prompt_find_variations(args: &HashMap<String, String>) -> Result<GetPromptResult, McpError> {
        let id = args.get("artifact_id").ok_or_else(|| {
            McpError::invalid_params("artifact_id argument is required", None)
        })?;

        Ok(GetPromptResult {
            description: Some(format!("Find variations of artifact {}", id)),
            messages: vec![PromptMessage::new_text(
                PromptMessageRole::User,
                format!(
                    r#"Use graph_query to find all variations of artifact {id}.

First, get the artifact to find its variation_set:
```graphql
{{
  Artifact(id: "{id}") {{
    id @output
    variation_set_id @output
  }}
}}
```

Then query for all artifacts in that variation set:
```graphql
{{
  Artifact {{
    id @output
    creator @output
    tags @output
    variation_index @output
  }}
}}
```

Note: Filter by variation_set_id once you have it from the first query."#,
                    id = id
                ),
            )],
        })
    }

    fn prompt_search_vibes(args: &HashMap<String, String>) -> Result<GetPromptResult, McpError> {
        let vibe = args.get("vibe").ok_or_else(|| {
            McpError::invalid_params("vibe argument is required", None)
        })?;

        Ok(GetPromptResult {
            description: Some(format!("Search for '{}' vibes", vibe)),
            messages: vec![PromptMessage::new_text(
                PromptMessageRole::User,
                format!(
                    r#"Search for artifacts with '{vibe}' vibe using graph_query.

Query artifacts that have vibe tags:
```graphql
{{
  Artifact {{
    id @output
    creator @output
    tags @output @filter(op: "has_substring", value: ["{vibe}"])
  }}
}}
```

This searches for artifacts with tags containing "{vibe}". Vibe tags typically have the format "vibe:jazzy", "vibe:dark", etc.

You can also use graph_context with vibe_search parameter:
```
graph_context(vibe_search: "{vibe}")
```"#,
                    vibe = vibe
                ),
            )],
        })
    }

    fn prompt_device_routing(args: &HashMap<String, String>) -> Result<GetPromptResult, McpError> {
        let device_filter = args.get("device_name").map(|s| s.as_str()).unwrap_or("");

        let description = if device_filter.is_empty() {
            "Show all PipeWire device connections".to_string()
        } else {
            format!("Show connections for devices matching '{}'", device_filter)
        };

        let query = if device_filter.is_empty() {
            r#"Use garden_query to show PipeWire device routing:

```graphql
{
  Identity {
    id @output
    name @output
    hints {
      kind @output
      value @output
    }
  }
}
```

This shows all identities (devices) with their discovery hints."#
                .to_string()
        } else {
            format!(
                r#"Use garden_query to show PipeWire device routing for devices matching "{filter}":

```graphql
{{
  Identity {{
    id @output
    name @output @filter(op: "has_substring", value: ["{filter}"])
    hints {{
      kind @output
      value @output
    }}
  }}
}}
```

This filters identities by name containing "{filter}"."#,
                filter = device_filter
            )
        };

        Ok(GetPromptResult {
            description: Some(description),
            messages: vec![PromptMessage::new_text(PromptMessageRole::User, query)],
        })
    }

    fn prompt_render_pipeline(args: &HashMap<String, String>) -> Result<GetPromptResult, McpError> {
        let style = args.get("style").map(|s| s.as_str()).unwrap_or("ambient");
        let soundfont_note = args
            .get("soundfont")
            .map(|sf| format!("Use soundfont hash: {}", sf))
            .unwrap_or_else(|| "Use a soundfont from holler://soundfonts".to_string());

        Ok(GetPromptResult {
            description: Some("Generate, render, and play audio pipeline".to_string()),
            messages: vec![PromptMessage::new_text(
                PromptMessageRole::User,
                format!(
                    r#"Create a complete audio generation pipeline with style hint "{style}":

1. **Generate MIDI** using orpheus_generate:
```json
{{
  "temperature": 1.0,
  "max_tokens": 512,
  "tags": ["style:{style}"],
  "creator": "claude"
}}
```

2. **Wait for generation** using job_poll with the returned job_id

3. **Render to audio** using midi_render:
```json
{{
  "input_hash": "<midi_hash_from_step_2>",
  "soundfont_hash": "<soundfont_hash>"
}}
```
{soundfont_note}

4. **Schedule playback** using timeline_region_create:
```json
{{
  "position": 0,
  "duration": 16,
  "behavior_type": "play_audio",
  "content_id": "<artifact_id_from_step_3>"
}}
```

5. **Start playback** with the play tool

Each step returns artifact IDs you can use with graph_context to track lineage."#,
                    style = style,
                    soundfont_note = soundfont_note
                ),
            )],
        })
    }
}

/// Convert JsonObject arguments to HashMap<String, String>.
pub fn args_to_hashmap(
    args: Option<&serde_json::Map<String, serde_json::Value>>,
) -> HashMap<String, String> {
    args.map(|obj| {
        obj.iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect()
    })
    .unwrap_or_default()
}
