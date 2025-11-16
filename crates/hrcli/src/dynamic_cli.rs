// A truly dynamic CLI system that doesn't fight the framework
use anyhow::{Result, Context};
use std::collections::HashMap;
use crate::discovery::schema::{DynamicToolSchema, ParameterHandler};

/// Dynamic command-line parser that works with runtime-discovered schemas
pub struct DynamicCli {
    args: Vec<String>,
    schemas: Vec<DynamicToolSchema>,
}

impl DynamicCli {
    pub fn new(schemas: Vec<DynamicToolSchema>) -> Self {
        Self {
            args: std::env::args().collect(),
            schemas,
        }
    }

    pub fn from_args(args: Vec<String>, schemas: Vec<DynamicToolSchema>) -> Self {
        Self { args, schemas }
    }

    /// Parse and execute - returns (command_name, parsed_args)
    pub fn parse(&self) -> Result<ParsedCommand> {
        // Skip program name
        let args = &self.args[1..];

        if args.is_empty() {
            return Ok(ParsedCommand::Help);
        }

        // Check for global flags first
        let (global_args, remaining) = self.parse_global_args(args)?;

        if remaining.is_empty() {
            return Ok(ParsedCommand::Help);
        }

        // Get the command
        let cmd = &remaining[0];

        // Check for meta commands
        match cmd.as_str() {
            "help" | "--help" | "-h" => return Ok(ParsedCommand::Help),
            "discover" => return Ok(ParsedCommand::Discover {
                json: remaining.contains(&"--json".to_string()),
            }),
            "completions" => return self.parse_completions_command(&remaining[1..]),
            "interactive" | "repl" => return Ok(ParsedCommand::Interactive),
            _ => {}
        }

        // Look for matching tool schema
        if let Some(schema) = self.schemas.iter().find(|s| {
            s.name == *cmd || s.metadata.cli.aliases.contains(&cmd.to_string())
        }) {
            let tool_args = self.parse_tool_args(schema, &remaining[1..])?;
            return Ok(ParsedCommand::Tool {
                name: schema.name.clone(),
                args: tool_args,
                global: global_args,
            });
        }

        Err(anyhow::anyhow!("Unknown command: {}", cmd))
    }

    fn parse_global_args(&self, args: &[String]) -> Result<(GlobalArgs, Vec<String>)> {
        let mut global = GlobalArgs::default();
        let mut remaining = Vec::new();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--server" | "-s" => {
                    if i + 1 < args.len() {
                        global.server = Some(args[i + 1].clone());
                        i += 2;
                    } else {
                        return Err(anyhow::anyhow!("--server requires a value"));
                    }
                }
                "--format" | "-f" => {
                    if i + 1 < args.len() {
                        global.format = Some(args[i + 1].clone());
                        i += 2;
                    } else {
                        return Err(anyhow::anyhow!("--format requires a value"));
                    }
                }
                "--no-color" => {
                    global.no_color = true;
                    i += 1;
                }
                "-v" | "--verbose" => {
                    global.verbose += 1;
                    i += 1;
                }
                arg if arg.starts_with('-') && !arg.starts_with("--") => {
                    // Count -vvv style verbosity
                    let v_count = arg.chars().filter(|&c| c == 'v').count();
                    if v_count > 0 && arg.chars().all(|c| c == '-' || c == 'v') {
                        global.verbose += v_count;
                        i += 1;
                    } else {
                        remaining.push(args[i].clone());
                        i += 1;
                    }
                }
                _ => {
                    // Not a global arg, add to remaining
                    remaining.extend_from_slice(&args[i..]);
                    break;
                }
            }
        }

        Ok((global, remaining))
    }

    fn parse_tool_args(&self, schema: &DynamicToolSchema, args: &[String]) -> Result<HashMap<String, serde_json::Value>> {
        let mut parsed = HashMap::new();
        let mut i = 0;

        // Handle stdin flag
        if args.contains(&"--stdin".to_string()) {
            use std::io::Read;
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            return serde_json::from_str(&buffer).context("stdin must contain valid JSON");
        }

        while i < args.len() {
            let arg = &args[i];

            if !arg.starts_with('-') {
                // Positional argument - handle based on schema
                // For now, skip
                i += 1;
                continue;
            }

            // Remove leading dashes
            let arg_name = arg.trim_start_matches('-');

            // Find matching parameter in schema
            let param = schema.extract_parameters().into_iter()
                .find(|p| to_kebab_case(&p.name) == arg_name || p.name == arg_name);

            if let Some(param_info) = param {
                let value = self.parse_parameter_value(&param_info.handler, args, &mut i)?;
                parsed.insert(param_info.name, value);
            } else {
                // Unknown argument - could be a flag or typo
                if arg.starts_with("--") {
                    return Err(anyhow::anyhow!("Unknown argument: {}", arg));
                }
                i += 1;
            }
        }

        // Check for required parameters
        for param in schema.extract_parameters() {
            if param.required && !parsed.contains_key(&param.name) {
                // Check environment variables for Environment handlers
                if let ParameterHandler::Environment { var_name, .. } = &param.handler {
                    if let Ok(value) = std::env::var(var_name) {
                        parsed.insert(param.name.clone(), serde_json::json!(value));
                        continue;
                    }
                }
                return Err(anyhow::anyhow!("Missing required parameter: {}", param.name));
            }
        }

        Ok(parsed)
    }

    fn parse_parameter_value(
        &self,
        handler: &ParameterHandler,
        args: &[String],
        index: &mut usize,
    ) -> Result<serde_json::Value> {
        match handler {
            ParameterHandler::Simple { .. } => {
                // Simple value - next arg is the value
                *index += 1;
                if *index < args.len() {
                    let value = args[*index].clone();
                    *index += 1;
                    Ok(serde_json::json!(value))
                } else {
                    Err(anyhow::anyhow!("Missing value for parameter"))
                }
            }
            ParameterHandler::Composite { fields, combiner } => {
                // Look for multiple related args
                let mut composite = serde_json::json!({});
                *index += 1;

                // Parse each field if present
                for field in fields {
                    let field_arg = format!("--{}", field.cli_arg.trim_start_matches("--"));
                    if let Some(pos) = args.iter().position(|a| a == &field_arg) {
                        if pos + 1 < args.len() {
                            composite[&field.name] = serde_json::json!(args[pos + 1]);
                        }
                    } else if let Some(default) = &field.default {
                        composite[&field.name] = default.clone();
                    }
                }

                // Apply combiner
                self.apply_combiner(composite, combiner)
            }
            ParameterHandler::Interactive { prompt, choices, multi_select } => {
                // Check if value provided, otherwise prompt
                *index += 1;
                if *index < args.len() && !args[*index].starts_with('-') {
                    let value = args[*index].clone();
                    *index += 1;
                    Ok(serde_json::json!(value))
                } else {
                    // Would prompt here
                    self.prompt_interactive(prompt, choices, *multi_select)
                }
            }
            _ => {
                // For other handlers, just get the next value
                *index += 1;
                if *index < args.len() {
                    let value = args[*index].clone();
                    *index += 1;
                    Ok(serde_json::json!(value))
                } else {
                    Err(anyhow::anyhow!("Missing value for parameter"))
                }
            }
        }
    }

    fn apply_combiner(&self, composite: serde_json::Value, combiner: &str) -> Result<serde_json::Value> {
        match combiner {
            "emotion_vector" => {
                Ok(serde_json::json!({
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
            _ => Ok(composite)
        }
    }

    fn prompt_interactive(
        &self,
        prompt: &str,
        choices: &[crate::discovery::schema::Choice],
        multi_select: bool,
    ) -> Result<serde_json::Value> {
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

                let values: Vec<serde_json::Value> = selections.iter()
                    .map(|&i| choices[i].value.clone())
                    .collect();

                Ok(serde_json::json!(values))
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

            Ok(serde_json::json!(input))
        }
    }

    fn parse_completions_command(&self, args: &[String]) -> Result<ParsedCommand> {
        if args.is_empty() {
            return Err(anyhow::anyhow!("Shell type required for completions"));
        }

        Ok(ParsedCommand::Completions {
            shell: args[0].clone(),
        })
    }

    pub fn print_help(&self, schemas: &[DynamicToolSchema]) {
        use owo_colors::OwoColorize;

        println!("{}", "hrcli - Dynamic Musical CLI".bright_cyan().bold());
        println!("{}", "━".repeat(50).bright_black());
        println!("\n{}", "USAGE:".bright_white());
        println!("  hrcli [OPTIONS] <COMMAND> [ARGS]");

        println!("\n{}", "OPTIONS:".bright_white());
        println!("  {} {:20} {}", "-s".bright_green(), "--server URL".bright_green(), "MCP server URL");
        println!("  {} {:20} {}", "-f".bright_green(), "--format FORMAT".bright_green(), "Output format (auto, json, table, plain)");
        println!("  {} {:20} {}", "".bright_green(), "--no-color".bright_green(), "Disable colored output");
        println!("  {} {:20} {}", "-v".bright_green(), "--verbose".bright_green(), "Increase verbosity");

        println!("\n{}", "COMMANDS:".bright_white());

        // Group by category
        let mut by_category: HashMap<String, Vec<&DynamicToolSchema>> = HashMap::new();
        for schema in schemas {
            let category = schema.metadata.cli.category.as_deref().unwrap_or("Tools");
            by_category.entry(category.to_string()).or_default().push(schema);
        }

        for (category, tools) in by_category {
            println!("\n  {}:", category.bright_yellow());
            for tool in tools {
                let icon = tool.metadata.ui_hints.icon.as_deref().unwrap_or("•");
                println!("    {} {:15} {}",
                    icon,
                    tool.name.bright_green(),
                    tool.description.dimmed()
                );
            }
        }

        println!("\n  {}:", "Meta Commands".bright_yellow());
        println!("    • {:15} {}", "discover".bright_green(), "Discover available tools");
        println!("    • {:15} {}", "cache".bright_green(), "Manage schema cache");
        println!("    • {:15} {}", "completions".bright_green(), "Generate shell completions");
        println!("    • {:15} {}", "interactive".bright_green(), "Start REPL mode");
    }
}

pub enum ParsedCommand {
    Help,
    Tool {
        name: String,
        args: HashMap<String, serde_json::Value>,
        global: GlobalArgs,
    },
    Discover {
        json: bool,
    },
    Completions {
        shell: String,
    },
    Interactive,
}

#[derive(Debug, Default)]
pub struct GlobalArgs {
    pub server: Option<String>,
    pub format: Option<String>,
    pub no_color: bool,
    pub verbose: usize,
}

fn to_kebab_case(s: &str) -> String {
    s.chars()
        .enumerate()
        .flat_map(|(i, c)| {
            if c.is_uppercase() && i > 0 {
                vec!['-', c.to_ascii_lowercase()]
            } else if c == '_' {
                vec!['-']
            } else {
                vec![c.to_ascii_lowercase()]
            }
        })
        .collect()
}