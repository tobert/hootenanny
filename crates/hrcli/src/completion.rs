use anyhow::{Context, Result};
use crate::mcp_client::McpClient;

/// Complete an argument value by querying the MCP server
/// Prints completion values one per line (for shell consumption)
pub async fn complete_argument(
    server_url: &str,
    tool_name: &str,
    argument_name: &str,
    partial: &str,
) -> Result<()> {
    // Connect to MCP server
    let client = McpClient::connect(server_url)
        .await
        .context("Failed to connect to MCP server")?;

    // Request completions from server
    let result = client
        .complete_argument(tool_name, argument_name, partial)
        .await;

    // Fail silently on connection errors (completion should not block usage)
    match result {
        Ok(completion) => {
            // Print each value on its own line for shell consumption
            for value in completion.values {
                println!("{}", value);
            }
        }
        Err(_) => {
            // Fail silently - shell completions should not block normal usage
        }
    }

    Ok(())
}
