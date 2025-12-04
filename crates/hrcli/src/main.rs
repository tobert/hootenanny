use anyhow::{Context, Result};
use owo_colors::OwoColorize;

mod mcp_client;
mod discovery;
mod execution;
mod dynamic_cli;
mod completion;

use discovery::{DiscoveryClient, DynamicToolSchema};
use dynamic_cli::{DynamicCli, ParsedCommand};
use execution::DynamicFormatter;

#[tokio::main]
async fn main() -> Result<()> {
    // Get server URL from env or use default (includes /mcp path)
    let server_url = std::env::var("HRCLI_SERVER")
        .unwrap_or_else(|_| "http://127.0.0.1:8080/mcp".to_string());

    // Always discover fresh from server - no caching
    let schemas = match discover_tools(&server_url).await {
        Ok(schemas) => schemas,
        Err(e) => {
            eprintln!("❌ Failed to discover tools from server");
            eprintln!("   {}", e.to_string().bright_red());
            eprintln!("\n💡 Hints:");
            eprintln!("   • Is the MCP server running at {}?", server_url.bright_yellow());
            eprintln!("   • Try: cargo run --bin hootenanny");
            std::process::exit(1);
        }
    };

    // Create dynamic CLI with discovered schemas
    let cli = DynamicCli::new(schemas.clone());

    // Parse command
    let command = match cli.parse() {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("Error: {}", e);
            cli.print_help(&schemas);
            std::process::exit(1);
        }
    };

    // Execute command
    match command {
        ParsedCommand::Help => {
            cli.print_help(&schemas);
        }
        ParsedCommand::Tool { name, args, global } => {
            let server = global.server.as_ref().unwrap_or(&server_url);
            execute_tool(server, &name, args, &schemas, global.no_color).await?;
        }
        ParsedCommand::Discover { json, .. } => {
            show_discovered_tools(&server_url, &schemas, json);
        }
        ParsedCommand::Completions { shell } => {
            generate_completions(&shell, &schemas)?;
        }
        ParsedCommand::Complete { tool_name, argument_name, partial } => {
            completion::complete_argument(&server_url, &tool_name, &argument_name, &partial).await?;
        }
        ParsedCommand::Interactive => {
            run_interactive_mode(&server_url, schemas).await?;
        }
    }

    Ok(())
}

async fn discover_tools(server_url: &str) -> Result<Vec<DynamicToolSchema>> {
    let client = DiscoveryClient::new(server_url.to_string());
    let (schemas, _capabilities) = client.discover_tools().await?;
    Ok(schemas)
}

async fn execute_tool(
    server_url: &str,
    tool_name: &str,
    args: std::collections::HashMap<String, serde_json::Value>,
    schemas: &[DynamicToolSchema],
    no_color: bool,
) -> Result<()> {
    use std::sync::{Arc, Mutex};
    use std::io::Write;

    // Find the schema for this tool
    let schema = schemas.iter()
        .find(|s| s.name == tool_name)
        .context("Tool schema not found")?;

    // Show icon if available
    if let Some(icon) = &schema.metadata.ui_hints.icon {
        print!("{} ", icon);
    }
    println!("{} {}", "Executing:".bright_cyan(), tool_name.bright_green().bold());

    // Track current progress message for display
    let current_progress = Arc::new(Mutex::new(Option::<String>::None));
    let is_tty = atty::is(atty::Stream::Stderr);

    // Create notification callback for progress and logs
    let progress_clone = current_progress.clone();
    let callback: mcp_client::NotificationCallback = Arc::new(move |notification| {
        use mcp_client::{Notification, LogLevel};

        match notification {
            Notification::Progress(progress_notif) => {
                if !is_tty {
                    return; // Don't show progress if not a terminal
                }

                // Format progress message
                let msg = if let Some(total) = progress_notif.total {
                    let percentage = (progress_notif.progress / total * 100.0) as u32;
                    if let Some(message) = &progress_notif.message {
                        format!("⏳ {}% - {}", percentage, message)
                    } else {
                        format!("⏳ {}%", percentage)
                    }
                } else if let Some(message) = &progress_notif.message {
                    format!("⏳ {}", message)
                } else {
                    format!("⏳ {:.0}", progress_notif.progress)
                };

                // Update progress (overwrite line with \r)
                eprint!("\r{:<80}\r{}", "", msg);
                let _ = std::io::stderr().flush();

                *progress_clone.lock().unwrap() = Some(msg);
            }
            Notification::Log(log_msg) => {
                // Clear progress line if showing
                if is_tty {
                    if let Some(_) = progress_clone.lock().unwrap().as_ref() {
                        eprint!("\r{:<80}\r", "");
                    }
                }

                // Format log message with colored icon
                let (icon, color_fn): (&str, fn(&str) -> String) = match log_msg.level {
                    LogLevel::Debug => ("🐛", |s| s.bright_black().to_string()),
                    LogLevel::Info => ("ℹ", |s| s.bright_blue().to_string()),
                    LogLevel::Notice => ("●", |s| s.bright_cyan().to_string()),
                    LogLevel::Warning => ("⚠", |s| s.bright_yellow().to_string()),
                    LogLevel::Error => ("✗", |s| s.bright_red().to_string()),
                    LogLevel::Critical | LogLevel::Alert | LogLevel::Emergency =>
                        ("🔥", |s| s.bright_red().bold().to_string()),
                };

                let message = match &log_msg.message {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };

                eprintln!("{} {}", color_fn(icon), message);

                // Restore progress if it was showing
                if is_tty {
                    if let Some(progress_msg) = progress_clone.lock().unwrap().as_ref() {
                        eprint!("{}", progress_msg);
                        let _ = std::io::stderr().flush();
                    }
                }
            }
        }
    });

    // Connect to MCP server with notification callback
    let client = mcp_client::McpClient::connect(server_url)
        .await
        .context("Failed to connect to MCP server")?
        .with_notification_callback(callback);

    // Call the tool
    let params = serde_json::Value::Object(args.into_iter().collect());
    let response = client
        .call_tool(tool_name, params)
        .await
        .context("Tool execution failed")?;

    // Clear progress line before showing results
    if is_tty {
        if current_progress.lock().unwrap().is_some() {
            eprint!("\r{:<80}\r", "");
        }
    }

    // Format the response
    let formatter = DynamicFormatter::new(schema.clone(), no_color);
    let output = formatter.format(response)?;

    println!("{}", output);

    Ok(())
}

fn show_discovered_tools(server_url: &str, schemas: &[DynamicToolSchema], json: bool) {
    if json {
        let output = serde_json::json!({
            "server": server_url,
            "tools": schemas.iter().map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "description": s.description,
                    "aliases": s.metadata.cli.aliases,
                    "category": s.metadata.cli.category,
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("{}", "🔍 Discovered Tools".bright_cyan().bold());
        println!("{}", "━".repeat(50).bright_black());
        println!("Server: {}", server_url.bright_yellow());
        println!("Total tools: {}", schemas.len().to_string().bright_green());

        // Group by category
        let mut by_category = std::collections::HashMap::new();
        for schema in schemas {
            let category = schema.metadata.cli.category.as_deref().unwrap_or("Tools");
            by_category.entry(category).or_insert_with(Vec::new).push(schema);
        }

        for (category, tools) in by_category {
            println!("\n{}", category.bright_white().bold());
            for tool in tools {
                let icon = tool.metadata.ui_hints.icon.as_deref().unwrap_or("•");
                println!(
                    "  {} {} - {}",
                    icon,
                    tool.name.bright_green(),
                    tool.description.dimmed()
                );

                if !tool.metadata.cli.aliases.is_empty() {
                    println!(
                        "    {} {}",
                        "aliases:".dimmed(),
                        tool.metadata.cli.aliases.join(", ").bright_black()
                    );
                }
            }
        }
    }
}

fn generate_completions(shell: &str, schemas: &[DynamicToolSchema]) -> Result<()> {
    // Generate simple completion script
    match shell {
        "bash" => generate_bash_completions(schemas),
        "zsh" => generate_zsh_completions(schemas),
        "fish" => generate_fish_completions(schemas),
        _ => {
            eprintln!("Unsupported shell: {}", shell);
            eprintln!("Supported shells: bash, zsh, fish");
        }
    }
    Ok(())
}

fn generate_bash_completions(schemas: &[DynamicToolSchema]) {
    println!("# Bash completion for hrcli");
    println!("_hrcli() {{");
    println!("    local cur=${{COMP_WORDS[COMP_CWORD]}}");
    println!("    local prev=${{COMP_WORDS[COMP_CWORD-1]}}");
    println!("    local commands=\"{}\"",
        schemas.iter()
            .map(|s| s.name.as_str())
            .chain(["discover", "completions", "interactive", "help"].iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!();
    println!("    # If completing a subcommand name");
    println!("    if [ $COMP_CWORD -eq 1 ]; then");
    println!("        COMPREPLY=( $(compgen -W \"$commands\" -- $cur) )");
    println!("        return 0");
    println!("    fi");
    println!();
    println!("    # Get the tool name from position 1");
    println!("    local tool_name=${{COMP_WORDS[1]}}");
    println!();
    println!("    # Try dynamic completion from server");
    println!("    # Extract argument name from previous word if it's a flag");
    println!("    if [[ \"$prev\" == --* ]]; then");
    println!("        local arg_name=${{prev#--}}");
    println!("        local completions=$(hrcli __complete \"$tool_name\" \"$arg_name\" \"$cur\" 2>/dev/null)");
    println!("        if [ -n \"$completions\" ]; then");
    println!("            COMPREPLY=( $(compgen -W \"$completions\" -- $cur) )");
    println!("            return 0");
    println!("        fi");
    println!("    fi");
    println!("}}");
    println!("complete -F _hrcli hrcli");
}

fn generate_zsh_completions(schemas: &[DynamicToolSchema]) {
    println!("#compdef hrcli");
    println!("_hrcli() {{");
    println!("    local -a commands");
    println!("    commands=(");
    for schema in schemas {
        println!("        '{}:{}'", schema.name, schema.description.replace('\'', ""));
    }
    println!("        'discover:Discover available tools'");
    println!("        'completions:Generate completions'");
    println!("        'interactive:Start REPL mode'");
    println!("    )");
    println!("    _describe 'command' commands");
    println!("}}");
}

fn generate_fish_completions(schemas: &[DynamicToolSchema]) {
    println!("# Fish completion for hrcli");
    for schema in schemas {
        println!("complete -c hrcli -n __fish_use_subcommand -a {} -d '{}'",
            schema.name, schema.description.replace('\'', ""));
    }
    println!("complete -c hrcli -n __fish_use_subcommand -a discover -d 'Discover available tools'");
    println!("complete -c hrcli -n __fish_use_subcommand -a completions -d 'Generate completions'");
    println!("complete -c hrcli -n __fish_use_subcommand -a interactive -d 'Start REPL mode'");
}

async fn run_interactive_mode(server_url: &str, schemas: Vec<DynamicToolSchema>) -> Result<()> {
    println!("{}", "🎵 HalfRemembered Interactive Mode".bright_cyan().bold());
    println!("{}", "━".repeat(50).bright_black());
    println!("Type 'help' for available commands, 'exit' to quit\n");

    use rustyline::DefaultEditor;
    let mut rl = DefaultEditor::new()?;

    loop {
        let readline = rl.readline("hrcli> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line == "exit" || line == "quit" {
                    break;
                }

                if line == "help" {
                    println!("Available tools:");
                    for schema in &schemas {
                        let icon = schema.metadata.ui_hints.icon.as_deref().unwrap_or("•");
                        println!("  {} {}", icon, schema.name);
                    }
                    continue;
                }

                // Parse line as arguments
                let args: Vec<String> = line.split_whitespace()
                    .map(|s| s.to_string())
                    .collect();

                if args.is_empty() {
                    continue;
                }

                // Create a new CLI instance with args
                let mut full_args = vec!["hrcli".to_string()];
                full_args.extend(args);

                let cli = DynamicCli::from_args(full_args, schemas.clone());
                match cli.parse() {
                    Ok(ParsedCommand::Tool { name, args, global }) => {
                        let default_server = server_url.to_string();
                        let server = global.server.as_ref().unwrap_or(&default_server);
                        if let Err(e) = execute_tool(server, &name, args, &schemas, global.no_color).await {
                            eprintln!("Error: {}", e);
                        }
                    }
                    Ok(ParsedCommand::Help) => {
                        cli.print_help(&schemas);
                    }
                    Ok(_) => {
                        println!("This command is not available in interactive mode");
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            Err(_) => break,
        }
    }

    println!("Goodbye! 👋");
    Ok(())
}