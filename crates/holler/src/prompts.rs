//! MCP Prompts - Templates for common multi-step workflows
//!
//! Prompts solve the discoverability problem - agents get pre-built workflow templates
//! with clear argument descriptions instead of assembling tool chains from scratch.

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
            "render_pipeline" => Self::prompt_render_pipeline(args),
            _ => Err(McpError::invalid_params(
                format!("Unknown prompt: {}", name),
                None,
            )),
        }
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

Each step returns artifact IDs that track lineage through parent chains."#,
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
