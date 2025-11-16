use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures::StreamExt;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
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
    responses: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: Arc<Mutex<u64>>,
}

impl McpClient {
    /// Connect to the MCP server via SSE
    pub async fn connect(base_url: &str) -> Result<Self> {
        let client = reqwest::Client::new();
        let mut mcp_client = Self {
            base_url: base_url.to_string(),
            client,
            session_id: None,
            responses: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)), // Start IDs from 1
        };

        // Connect to SSE endpoint to get session ID and start listener
        mcp_client.establish_session().await?;

        Ok(mcp_client)
    }

    /// Establish SSE connection, extract session ID, and start listener task
    async fn establish_session(&mut self) -> Result<()> {
        let sse_url = format!("{}/sse", self.base_url);

        eprintln!("[MCP] Connecting to SSE endpoint: {}", sse_url);

        let response = timeout(Duration::from_secs(5), self.client.get(&sse_url).send())
            .await
            .context("Timeout connecting to SSE endpoint")?
            .context("Failed to connect to SSE endpoint")?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "SSE connection failed with status: {}",
                response.status()
            ));
        }

        eprintln!("[MCP] SSE connection successful, reading stream for session ID...");

        let mut stream = response.bytes_stream();

        // First, get the session ID from the initial "endpoint" event
        let session_id_future = async {
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("Failed to read SSE chunk")?;
                let text = String::from_utf8_lossy(&chunk);
                if let Some(session_id) = extract_session_id(&text) {
                    self.session_id = Some(session_id.clone());
                    return Ok::<String, anyhow::Error>(session_id);
                }
            }
            Err(anyhow!("SSE stream ended without providing session ID"))
        };

        let session_id = timeout(Duration::from_secs(5), session_id_future)
            .await
            .context("Timeout waiting for session ID from SSE stream")??;

        eprintln!("[MCP] Got session ID: {}", session_id);

        // Spawn the background task to listen for SSE messages
        let responses = self.responses.clone();
        tokio::spawn(async move {
            eprintln!("[MCP] SSE listener task started");
            listen_for_responses(stream, responses).await;
            eprintln!("[MCP] SSE listener task ended");
        });

        eprintln!("[MCP] Starting MCP initialization handshake...");

        // Perform MCP initialization
        self.initialize().await.context("MCP initialization failed")?;

        eprintln!("[MCP] MCP client fully connected and initialized");

        Ok(())
    }

    /// Initialize the MCP session with full handshake
    async fn initialize(&self) -> Result<()> {
        // Step 1: Send initialize request
        let mut next_id = self.next_id.lock().await;
        let id = *next_id;
        *next_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {
                    "name": "hrcli",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        let _response = self.send_request(id, request).await?;

        // Step 2: Send notifications/initialized (no ID, it's a notification)
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });

        // Send notification directly without expecting response
        self.send_notification(notification).await?;

        Ok(())
    }

    /// List available tools from the MCP server
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        let mut next_id = self.next_id.lock().await;
        let id = *next_id;
        *next_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {}
        });

        let response = self.send_request(id, request).await?;

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
        let mut next_id = self.next_id.lock().await;
        let id = *next_id;
        *next_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        let response = self.send_request(id, request).await?;

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
                            // The text is a JSON string, so we need to parse it
                            let parsed_text: Value = serde_json::from_str(text.as_str().unwrap_or(""))
                                .context("Failed to parse text field in response")?;
                            return Ok(parsed_text);
                        }
                    }
                }
            }
            // If no content array, return the whole result
            return Ok(result.clone());
        }

        Err(anyhow!("No result in response"))
    }

    /// Send an MCP request via HTTP POST and wait for the response from the SSE stream
    async fn send_request(&self, id: u64, request: Value) -> Result<Value> {
        let session_id = self
            .session_id
            .as_ref()
            .context("No active session")?;

        let (tx, rx) = oneshot::channel();
        self.responses.lock().await.insert(id, tx);

        let post_url = format!("{}/message?sessionId={}", self.base_url, session_id);

        let response = self
            .client
            .post(&post_url)
            .header("Content-Type", "application/json")
            .body(request.to_string())
            .send()
            .await
            .context("Failed to send request")?;

        if response.status() != reqwest::StatusCode::ACCEPTED {
            return Err(anyhow!(
                "Request failed with status: {}",
                response.status()
            ));
        }

        // Wait for the response from the SSE listener
        let response = timeout(Duration::from_secs(10), rx)
            .await
            .context("Timeout waiting for response")?
            .context("Failed to receive response")?;

        Ok(response)
    }

    /// Send a notification (no ID, no response expected)
    async fn send_notification(&self, notification: Value) -> Result<()> {
        let session_id = self
            .session_id
            .as_ref()
            .context("No active session")?;

        let post_url = format!("{}/message?sessionId={}", self.base_url, session_id);

        let response = timeout(
            Duration::from_secs(5),
            self.client
                .post(&post_url)
                .header("Content-Type", "application/json")
                .body(notification.to_string())
                .send()
        )
        .await
        .context("Timeout sending notification")?
        .context("Failed to send notification")?;

        if response.status() != reqwest::StatusCode::ACCEPTED {
            return Err(anyhow!(
                "Notification failed with status: {}",
                response.status()
            ));
        }

        Ok(())
    }
}

/// Extract session ID from the initial SSE "endpoint" event
fn extract_session_id(text: &str) -> Option<String> {
    for line in text.lines() {
        if line.starts_with("data: /message?sessionId=") {
            return Some(line[25..].trim().to_string());
        }
    }
    None
}

/// Background task to listen for SSE messages and dispatch them
async fn listen_for_responses(
    mut stream: impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
    responses: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
) {
    let mut current_event_type: Option<String> = None;
    let mut chunk_count = 0;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                chunk_count += 1;
                let text = String::from_utf8_lossy(&chunk);
                eprintln!("[MCP-LISTENER] Chunk {}: {:?}", chunk_count, &text[..text.len().min(100)]);

                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Track event type
                    if line.starts_with("event:") {
                        current_event_type = Some(line[6..].trim().to_string());
                        continue;
                    }

                    // Process data lines
                    if line.starts_with("data:") {
                        let data = line[5..].trim();

                        // Only try to parse JSON for "message" events
                        if current_event_type.as_deref() == Some("message") {
                            match serde_json::from_str::<Value>(data) {
                                Ok(value) => {

                                    if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                                        let mut resp_map = responses.lock().await;
                                        if let Some(sender) = resp_map.remove(&id) {
                                            let _ = sender.send(value);
                                        }
                                    }
                                }
                                Err(_) => {
                                    // Ignore parse errors silently
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                break; // SSE stream closed or error
            }
        }
    }
}
