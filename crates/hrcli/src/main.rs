use anyhow::{Context, Result};
use owo_colors::OwoColorize;

mod mcp_client;
mod discovery;
mod execution;
mod dynamic_cli;

use discovery::{DiscoveryClient, DynamicToolSchema};
use dynamic_cli::{DynamicCli, ParsedCommand};
use execution::DynamicFormatter;

#[tokio::main]
async fn main() -> Result<()> {
    // Get server URL from env or use default
    let server_url = std::env::var("HRCLI_SERVER")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

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
    // Find the schema for this tool
    let schema = schemas.iter()
        .find(|s| s.name == tool_name)
        .context("Tool schema not found")?;

    // Show icon if available
    if let Some(icon) = &schema.metadata.ui_hints.icon {
        print!("{} ", icon);
    }
    println!("{} {}", "Executing:".bright_cyan(), tool_name.bright_green().bold());

    // Connect to MCP server
    let client = mcp_client::McpClient::connect(server_url)
        .await
        .context("Failed to connect to MCP server")?;

    // Call the tool
    let params = serde_json::Value::Object(args.into_iter().collect());
    let response = client
        .call_tool(tool_name, params)
        .await
        .context("Tool execution failed")?;

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
    println!("    local commands=\"{}\"",
        schemas.iter()
            .map(|s| s.name.as_str())
            .chain(["discover", "completions", "interactive", "help"].iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("    COMPREPLY=( $(compgen -W \"$commands\" -- $cur) )");
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