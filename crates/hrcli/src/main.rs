use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;

mod mcp_client;

use mcp_client::McpClient;

/// HalfRemembered MCP CLI - Beautiful command-line interface for musical collaboration
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// MCP server URL
    #[arg(short, long, default_value = "http://127.0.0.1:8080")]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List available MCP tools
    ListTools,

    /// Call an MCP tool
    Call {
        /// Tool name (play, add_node, fork_branch, get_tree_status)
        tool: String,

        /// Arguments as JSON (or - for stdin)
        #[arg(default_value = "{}")]
        args: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::ListTools => list_tools(&cli.server).await,
        Commands::Call { tool, args } => call_tool(&cli.server, &tool, &args).await,
    }
}

async fn list_tools(server_url: &str) -> Result<()> {
    println!("{}", "📋 Available MCP Tools".bright_cyan().bold());
    println!("{}", "━".repeat(50).bright_black());

    let client = McpClient::connect(server_url)
        .await
        .context("Failed to connect to MCP server")?;

    let tools = client
        .list_tools()
        .await
        .context("Failed to list tools")?;

    for tool in tools {
        let icon = match tool.name.as_str() {
            "play" => "🎵",
            "add_node" => "🌳",
            "fork_branch" => "🔱",
            "get_tree_status" => "📊",
            _ => "🔧",
        };

        println!(
            "  {} {} - {}",
            icon,
            tool.name.bright_green(),
            tool.description.bright_white()
        );

        if !tool.parameters.is_empty() {
            println!("    {} {}", "Parameters:".dimmed(), tool.parameters.bright_yellow());
        }
    }

    println!("{}", "━".repeat(50).bright_black());
    Ok(())
}

async fn call_tool(server_url: &str, tool_name: &str, args: &str) -> Result<()> {
    // Handle stdin input
    let args_json = if args == "-" {
        let mut buffer = String::new();
        use std::io::Read;
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        buffer
    } else {
        args.to_string()
    };

    // Parse arguments
    let arguments: serde_json::Value = serde_json::from_str(&args_json)
        .context("Arguments must be valid JSON")?;

    println!("{} {}", "🎵 Calling tool:".bright_cyan(), tool_name.bright_green().bold());
    println!("{}", "━".repeat(50).bright_black());

    let client = McpClient::connect(server_url)
        .await
        .context("Failed to connect to MCP server")?;

    let result = client
        .call_tool(tool_name, arguments)
        .await
        .context("Tool call failed")?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    println!("{}", "━".repeat(50).bright_black());
    println!("{}", "✅ Success".bright_green());

    Ok(())
}
