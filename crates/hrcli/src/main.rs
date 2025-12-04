use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};

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
    use std::sync::Arc;

    // Find the schema for this tool
    let schema = schemas.iter()
        .find(|s| s.name == tool_name)
        .context("Tool schema not found")?;

    // Show icon if available
    if let Some(icon) = &schema.metadata.ui_hints.icon {
        print!("{} ", icon);
    }
    println!("{} {}", "Executing:".bright_cyan(), tool_name.bright_green().bold());

    // Set up indicatif for clean progress display
    let multi = MultiProgress::new();
    let pb = multi.add(ProgressBar::new(100));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} [{bar:40.green/dim}] {pos}% {msg:.dim}")
            .unwrap()
            .progress_chars("█▓▒░ ")
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
    );
    pb.set_message("Initializing...");

    // Enable steady tick so spinner animates even when progress doesn't change
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    // Create notification callback for progress and logs
    let pb_clone = pb.clone();
    let callback: mcp_client::NotificationCallback = Arc::new(move |notification| {
        use mcp_client::{Notification, LogLevel};

        match notification {
            Notification::Progress(progress_notif) => {
                // Calculate position (0-100)
                let (progress_val, total_val) = match progress_notif.total {
                    Some(t) => (progress_notif.progress, t),
                    None => (progress_notif.progress, 1.0),
                };
                let position = ((progress_val / total_val) * 100.0) as u64;

                pb_clone.set_position(position);

                if let Some(message) = &progress_notif.message {
                    pb_clone.set_message(message.clone());
                } else {
                    pb_clone.set_message("");
                }
            }
            Notification::Log(log_msg) => {
                // Format log message with colored icon and level badge
                let (icon, badge) = match log_msg.level {
                    LogLevel::Debug => ("🔍", "DEBUG"),
                    LogLevel::Info => ("ℹ", "INFO"),
                    LogLevel::Notice => ("●", "NOTE"),
                    LogLevel::Warning => ("⚠", "WARN"),
                    LogLevel::Error => ("✗", "ERROR"),
                    LogLevel::Critical => ("🔥", "CRIT"),
                    LogLevel::Alert => ("🚨", "ALERT"),
                    LogLevel::Emergency => ("💥", "EMERG"),
                };

                let message = match &log_msg.message {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };

                // Print log above progress bar (indicatif handles the positioning)
                let log_line = if let Some(logger) = &log_msg.logger {
                    format!("{} [{}] [{}] {}", icon, badge, logger, message)
                } else {
                    format!("{} [{}] {}", icon, badge, message)
                };

                pb_clone.println(log_line);
            }
        }
    });

    // Connect to MCP server with notification callback
    let client = mcp_client::McpClient::connect_with_callback(server_url, Some(callback))
        .await
        .context("Failed to connect to MCP server")?;

    // Call the tool
    let params = serde_json::Value::Object(args.into_iter().collect());
    let response = client
        .call_tool(tool_name, params)
        .await
        .context("Tool execution failed")?;

    // Check if response contains a job_id - if so, auto-poll it
    let final_response = if let Some(job_id) = response.get("job_id").and_then(|v| v.as_str()) {
        pb.set_message(format!("Job started: {}", job_id));

        // Poll the job until completion
        poll_job_until_complete(&client, job_id, &pb).await?
    } else {
        // Immediate response - give async notifications time to arrive
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        response
    };

    // Finish progress bar
    pb.finish_and_clear();

    // Format the final response
    let formatter = DynamicFormatter::new(schema.clone(), no_color);
    let output = formatter.format(final_response)?;

    println!("{}", output);

    Ok(())
}

/// Poll a job until it completes, updating progress bar
async fn poll_job_until_complete(
    client: &mcp_client::McpClient,
    job_id: &str,
    pb: &ProgressBar,
) -> Result<serde_json::Value> {
    use std::time::Duration;

    loop {
        // Poll with 1 second timeout
        let poll_params = serde_json::json!({
            "job_ids": [job_id],
            "timeout_ms": 1000,
            "mode": "any"
        });

        let poll_response = client
            .call_tool("job_poll", poll_params)
            .await
            .context("Failed to poll job")?;

        // Check if job completed
        if let Some(completed) = poll_response.get("completed").and_then(|v| v.as_array()) {
            if !completed.is_empty() {
                // Job completed! Get the full status with result
                pb.set_message("Job completed! Fetching result...");

                let status_params = serde_json::json!({
                    "job_id": job_id
                });

                let status_response = client
                    .call_tool("job_status", status_params)
                    .await
                    .context("Failed to get job status")?;

                // Extract the result field from status
                if let Some(result) = status_response.get("result") {
                    return Ok(result.clone());
                } else {
                    return Ok(status_response);
                }
            }
        }

        // Not complete yet, wait a bit and continue polling
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
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