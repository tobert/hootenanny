use clap::ArgMatches;
use serde_json::{json, Value};
use anyhow::{Context, Result};
use crate::discovery::schema::{DynamicToolSchema, ParameterHandler, CompositeField};

pub struct ArgumentTransformer {
    schema: DynamicToolSchema,
}

impl ArgumentTransformer {
    pub fn new(schema: DynamicToolSchema) -> Self {
        Self { schema }
    }

    /// Transform CLI arguments to MCP request parameters
    pub fn transform(&self, matches: &ArgMatches) -> Result<Value> {
        let mut params = json!({});

        // Handle stdin input if present
        if matches.get_flag("stdin") {
            use std::io::Read;
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .context("Failed to read from stdin")?;

            // Try to parse as JSON
            return serde_json::from_str(&buffer)
                .context("stdin must contain valid JSON");
        }

        // Process each parameter
        for param_info in self.schema.extract_parameters() {
            let value = self.transform_parameter(matches, &param_info.name, &param_info.handler)?;
            if let Some(v) = value {
                params[param_info.name] = v;
            }
        }

        Ok(params)
    }

    fn transform_parameter(
        &self,
        matches: &ArgMatches,
        name: &str,
        handler: &ParameterHandler,
    ) -> Result<Option<Value>> {
        match handler {
            ParameterHandler::Simple { transform, .. } => {
                if let Some(value) = matches.get_one::<String>(name) {
                    let mut result = json!(value);

                    // Apply transformation if specified
                    if let Some(transform_expr) = transform {
                        result = self.apply_transform(result, transform_expr)?;
                    }

                    Ok(Some(result))
                } else {
                    Ok(None)
                }
            }

            ParameterHandler::Composite { fields, combiner } => {
                let mut composite = json!({});
                let mut has_value = false;

                for field in fields {
                    let field_name = field.name.trim();
                    if let Some(value) = matches.get_one::<String>(field_name) {
                        composite[field_name] = json!(value);
                        has_value = true;
                    } else if let Some(default) = &field.default {
                        composite[field_name] = default.clone();
                        has_value = true;
                    }
                }

                if has_value {
                    // Apply combiner logic
                    let combined = self.apply_combiner(composite, combiner)?;
                    Ok(Some(combined))
                } else {
                    Ok(None)
                }
            }

            ParameterHandler::Interactive { prompt, choices, multi_select } => {
                if let Some(value) = matches.get_one::<String>(name) {
                    // User provided value directly
                    Ok(Some(json!(value)))
                } else {
                    // Need to prompt interactively
                    let selected = self.prompt_interactive(prompt, choices, *multi_select)?;
                    Ok(Some(selected))
                }
            }

            ParameterHandler::FilePath { base_dir, .. } => {
                if let Some(path) = matches.get_one::<String>(name) {
                    let full_path = if let Some(base) = base_dir {
                        std::path::Path::new(base).join(path)
                    } else {
                        std::path::PathBuf::from(path)
                    };

                    // Convert to absolute path
                    let absolute = std::fs::canonicalize(&full_path)
                        .unwrap_or(full_path);

                    Ok(Some(json!(absolute.to_string_lossy())))
                } else {
                    Ok(None)
                }
            }

            ParameterHandler::Environment { var_name, required } => {
                if let Some(value) = matches.get_one::<String>(name) {
                    Ok(Some(json!(value)))
                } else if let Ok(value) = std::env::var(var_name) {
                    Ok(Some(json!(value)))
                } else if *required {
                    anyhow::bail!("Required environment variable {} not set", var_name)
                } else {
                    Ok(None)
                }
            }

            ParameterHandler::Custom { handler_type, config } => {
                // Delegate to custom handler registry
                self.handle_custom(matches, name, handler_type, config)
            }
        }
    }

    fn apply_transform(&self, value: Value, transform_expr: &str) -> Result<Value> {
        // Simple transform expressions
        match transform_expr {
            "uppercase" => {
                if let Some(s) = value.as_str() {
                    Ok(json!(s.to_uppercase()))
                } else {
                    Ok(value)
                }
            }
            "lowercase" => {
                if let Some(s) = value.as_str() {
                    Ok(json!(s.to_lowercase()))
                } else {
                    Ok(value)
                }
            }
            "trim" => {
                if let Some(s) = value.as_str() {
                    Ok(json!(s.trim()))
                } else {
                    Ok(value)
                }
            }
            expr if expr.starts_with("parse:") => {
                let type_name = &expr[6..];
                if let Some(s) = value.as_str() {
                    match type_name {
                        "int" => Ok(json!(s.parse::<i64>()?)),
                        "float" => Ok(json!(s.parse::<f64>()?)),
                        "bool" => Ok(json!(s.parse::<bool>()?)),
                        "json" => Ok(serde_json::from_str(s)?),
                        _ => Ok(value)
                    }
                } else {
                    Ok(value)
                }
            }
            _ => Ok(value)
        }
    }

    fn apply_combiner(&self, composite: Value, combiner: &str) -> Result<Value> {
        match combiner {
            "emotion_vector" => {
                // Special case for emotion vectors
                Ok(json!({
                    "valence": composite["valence"].as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0),
                    "arousal": composite["arousal"].as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.5),
                    "agency": composite["agency"].as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0),
                }))
            }
            "concat" => {
                // Concatenate all fields
                let parts: Vec<String> = composite.as_object()
                    .map(|obj| {
                        obj.values()
                            .filter_map(|v| v.as_str())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(json!(parts.join(" ")))
            }
            "array" => {
                // Convert to array
                let values: Vec<Value> = composite.as_object()
                    .map(|obj| obj.values().cloned().collect())
                    .unwrap_or_default();
                Ok(json!(values))
            }
            _ => {
                // Return as-is for unknown combiners
                Ok(composite)
            }
        }
    }

    fn prompt_interactive(
        &self,
        prompt: &str,
        choices: &[crate::discovery::schema::Choice],
        multi_select: bool,
    ) -> Result<Value> {
        use dialoguer::{theme::ColorfulTheme, Select, MultiSelect};

        if !choices.is_empty() {
            let labels: Vec<String> = choices.iter()
                .map(|c| {
                    if let Some(desc) = &c.description {
                        format!("{} - {}", c.label, desc)
                    } else {
                        c.label.clone()
                    }
                })
                .collect();

            if multi_select {
                let selections = MultiSelect::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .items(&labels)
                    .interact()?;

                let values: Vec<Value> = selections.iter()
                    .map(|&i| choices[i].value.clone())
                    .collect();

                Ok(json!(values))
            } else {
                let selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .items(&labels)
                    .default(0)
                    .interact()?;

                Ok(choices[selection].value.clone())
            }
        } else {
            // Free text input
            use dialoguer::Input;
            let input: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(prompt)
                .interact_text()?;

            Ok(json!(input))
        }
    }

    fn handle_custom(
        &self,
        matches: &ArgMatches,
        name: &str,
        handler_type: &str,
        config: &Value,
    ) -> Result<Option<Value>> {
        // This would integrate with a plugin system
        // For now, handle some common custom types
        match handler_type {
            "musical_note" => {
                if let Some(value) = matches.get_one::<String>(name) {
                    // Validate and normalize musical note
                    let normalized = self.normalize_musical_note(value)?;
                    Ok(Some(json!(normalized)))
                } else {
                    Ok(None)
                }
            }
            "color" => {
                if let Some(value) = matches.get_one::<String>(name) {
                    // Parse color (hex, rgb, name)
                    let color = self.parse_color(value)?;
                    Ok(Some(json!(color)))
                } else {
                    Ok(None)
                }
            }
            _ => {
                // Unknown custom handler, pass through as string
                if let Some(value) = matches.get_one::<String>(name) {
                    Ok(Some(json!(value)))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn normalize_musical_note(&self, note: &str) -> Result<String> {
        // Simple normalization
        let normalized = note.to_uppercase()
            .replace("♯", "#")
            .replace("♭", "b")
            .replace("SHARP", "#")
            .replace("FLAT", "b");

        // Validate it's a valid note
        let valid_notes = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                          "Cb", "Db", "Eb", "Fb", "Gb", "Ab", "Bb"];

        let base_note = normalized.trim_end_matches(char::is_numeric);
        if !valid_notes.contains(&base_note) {
            anyhow::bail!("Invalid musical note: {}", note);
        }

        Ok(normalized)
    }

    fn parse_color(&self, color: &str) -> Result<String> {
        // Simple color parsing
        if color.starts_with('#') {
            // Hex color
            if color.len() == 7 || color.len() == 4 {
                Ok(color.to_string())
            } else {
                anyhow::bail!("Invalid hex color: {}", color)
            }
        } else if color.starts_with("rgb(") || color.starts_with("rgba(") {
            // RGB color
            Ok(color.to_string())
        } else {
            // Named color - could validate against a list
            Ok(color.to_lowercase())
        }
    }
}