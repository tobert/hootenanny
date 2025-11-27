#[cfg(test)]
mod common;

#[tokio::test]
async fn test_infra_check() {
    // Just verify that we can compile the client module
    // We won't connect to anything yet because we haven't implemented the server side refactor
    let _client: Option<common::mcp_client::McpClient> = None;
}
