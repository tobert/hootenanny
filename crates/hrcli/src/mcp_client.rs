use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{timeout, Duration};

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: String,
}

pub struct McpClient {
    base_url: String,
    client: reqwest::Client,
    session_id: Option<String>,
}

impl McpClient {
    /// Connect to the MCP server via SSE
    pub async fn connect(base_url: &str) -> Result<Self> {
        let client = reqwest::Client::new();
        let mut mcp_client = Self {
            base_url: base_url.to_string(),
            client,
            session_id: None,
        };

        // Connect to SSE endpoint to get session ID
        mcp_client.establish_session().await?;

        Ok(mcp_client)
    }

    /// Establish SSE connection and extract session ID
    async fn establish_session(&mut self) -> Result<()> {
        let sse_url = format!("{}/sse", self.base_url);

        let response = timeout(
            Duration::from_secs(5),
            self.client.get(&sse_url).send()
        )
        .await
        .context("Timeout connecting to SSE endpoint")?
        .context("Failed to connect to SSE endpoint")?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "SSE connection failed with status: {}",
                response.status()
            ));
        }

        // Read the SSE stream to get session ID
        // Format: event: endpoint\ndata: /message?sessionId=<uuid>
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read SSE chunk")?;
            let text = String::from_utf8_lossy(&chunk);

            // Look for session ID in SSE messages
            // Format: data: /message?sessionId=5a0401f7-2171-4b18-a94a-a9ac29e4b5da
            for line in text.lines() {
                if line.starts_with("data: /message?sessionId=") {
                    let session_id = &line[25..]; // Skip "data: /message?sessionId="
                    self.session_id = Some(session_id.trim().to_string());
                    return Ok(());
                }
            }
        }

        Err(anyhow!("Failed to receive session ID from server"))
    }

    /// List available tools from the MCP server
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        let session_id = self
            .session_id
            .as_ref()
            .context("No active session")?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        });

        let response = self.send_request(session_id, request).await?;

        // Extract tools from response
        let tools = response["result"]["tools"]
            .as_array()
            .context("Invalid response format")?;

        let mut tool_infos = Vec::new();
        for tool in tools {
            let name = tool["name"]
                .as_str()
                .unwrap_or("unknown")
                .to_string();

            let description = tool["description"]
                .as_str()
                .unwrap_or("")
                .to_string();

            // Format parameters nicely
            let parameters = if let Some(input_schema) = tool.get("inputSchema") {
                if let Some(props) = input_schema.get("properties") {
                    props
                        .as_object()
                        .map(|obj| {
                            obj.keys()
                                .map(|k| k.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            tool_infos.push(ToolInfo {
                name,
                description,
                parameters,
            });
        }

        Ok(tool_infos)
    }

    /// Call a tool on the MCP server
    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        let session_id = self
            .session_id
            .as_ref()
            .context("No active session")?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        let response = self.send_request(session_id, request).await?;

        // Check for errors
        if let Some(error) = response.get("error") {
            return Err(anyhow!("Tool call error: {}", error));
        }

        // Return the result content
        if let Some(result) = response.get("result") {
            if let Some(content) = result.get("content") {
                if let Some(arr) = content.as_array() {
                    if let Some(first) = arr.first() {
                        if let Some(text) = first.get("text") {
                            return Ok(text.clone());
                        }
                    }
                }
            }
            // If no content array, return the whole result
            return Ok(result.clone());
        }

        Err(anyhow!("No result in response"))
    }

    /// Send an MCP request via HTTP POST
    async fn send_request(&self, session_id: &str, request: Value) -> Result<Value> {
        let post_url = format!("{}/message?sessionId={}", self.base_url, session_id);

        let response = timeout(
            Duration::from_secs(10),
            self.client
                .post(&post_url)
                .header("Content-Type", "application/json")
                .body(request.to_string())
                .send()
        )
        .await
        .context("Timeout sending request")?
        .context("Failed to send request")?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Request failed with status: {}",
                response.status()
            ));
        }

        let body = response.text().await.context("Failed to read response")?;
        let json: Value = serde_json::from_str(&body)
            .context(format!("Failed to parse response: {}", body))?;

        Ok(json)
    }
}
