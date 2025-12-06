//! SSE (Server-Sent Events) MCP Client.
//!
//! This is the legacy transport for MCP clients. It uses an SSE connection for
//! receiving responses and notifications, with HTTP POST for sending requests.
//!
//! Use this transport when you need:
//! - Progress notifications during long-running operations
//! - Log messages from the server
//! - Real-time updates

use bytes::Bytes;
use futures::StreamExt;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tokio::time::{timeout, Duration};

use super::{ClientOptions, InitializeResult, LogMessage, Notification, NotificationCallback, ToolInfo};
use crate::types::completion::CompletionResult;
use crate::types::progress::ProgressNotification;

/// MCP client using SSE (Server-Sent Events) transport.
///
/// This client maintains a persistent SSE connection to receive responses
/// and notifications. Requests are sent via HTTP POST.
pub struct SseClient {
    base_url: String,
    client: Client,
    session_id: Option<String>,
    responses: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: Arc<Mutex<u64>>,
    notification_callback: Option<NotificationCallback>,
    options: ClientOptions,
}

impl SseClient {
    /// Connect to an MCP server via SSE.
    ///
    /// This establishes the SSE connection, extracts the session ID, and
    /// performs the MCP initialization handshake.
    pub async fn connect(base_url: &str) -> Result<Self, SseClientError> {
        Self::connect_with_options(base_url, ClientOptions::default(), None).await
    }

    /// Connect with a notification callback.
    ///
    /// The callback will be called for progress and log notifications.
    pub async fn connect_with_callback(
        base_url: &str,
        callback: NotificationCallback,
    ) -> Result<Self, SseClientError> {
        Self::connect_with_options(base_url, ClientOptions::default(), Some(callback)).await
    }

    /// Connect with full options.
    pub async fn connect_with_options(
        base_url: &str,
        options: ClientOptions,
        callback: Option<NotificationCallback>,
    ) -> Result<Self, SseClientError> {
        let client = Client::new();
        let mut sse_client = Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            session_id: None,
            responses: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
            notification_callback: callback,
            options,
        };

        sse_client.establish_session().await?;
        Ok(sse_client)
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the session ID.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    async fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock().await;
        let current = *id;
        *id += 1;
        current
    }

    /// Establish SSE connection and perform initialization.
    async fn establish_session(&mut self) -> Result<(), SseClientError> {
        let sse_url = format!("{}/sse", self.base_url);

        let response = timeout(
            Duration::from_secs(self.options.timeout_secs),
            self.client.get(&sse_url).send(),
        )
        .await
        .map_err(|_| SseClientError::Timeout("SSE connection".into()))?
        .map_err(|e| SseClientError::Transport(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SseClientError::Http {
                status: response.status().as_u16(),
                body: format!("SSE connection failed"),
            });
        }

        let stream = response.bytes_stream();

        // Channel for receiving session ID from the listener
        let (session_tx, session_rx) = oneshot::channel();

        // Spawn background listener
        let responses = self.responses.clone();
        let callback = self.notification_callback.clone();
        tokio::spawn(async move {
            listen_for_responses(stream, responses, session_tx, callback).await;
        });

        // Wait for session ID
        let session_id = timeout(Duration::from_secs(5), session_rx)
            .await
            .map_err(|_| SseClientError::Timeout("session ID".into()))?
            .map_err(|_| SseClientError::Protocol("Failed to receive session ID".into()))?;

        self.session_id = Some(session_id);

        // Perform MCP initialization
        self.initialize().await?;

        Ok(())
    }

    /// Initialize the MCP session.
    async fn initialize(&self) -> Result<InitializeResult, SseClientError> {
        let id = self.next_id().await;

        let mut capabilities = serde_json::json!({});
        if self.options.enable_sampling {
            capabilities["sampling"] = serde_json::json!({});
        }

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": capabilities,
                "clientInfo": {
                    "name": self.options.client_name,
                    "version": self.options.client_version
                }
            }
        });

        let response = self.send_request(id, request).await?;

        let result: InitializeResult = serde_json::from_value(
            response
                .get("result")
                .cloned()
                .ok_or_else(|| SseClientError::Protocol("Missing result".into()))?,
        )
        .map_err(|e| SseClientError::Protocol(e.to_string()))?;

        // Send initialized notification
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        self.send_notification(notification).await?;

        Ok(result)
    }

    /// List available tools.
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>, SseClientError> {
        let id = self.next_id().await;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {}
        });

        let response = self.send_request(id, request).await?;

        let tools = response
            .get("result")
            .and_then(|r| r.get("tools"))
            .ok_or_else(|| SseClientError::Protocol("Missing tools".into()))?;

        serde_json::from_value(tools.clone())
            .map_err(|e| SseClientError::Protocol(e.to_string()))
    }

    /// Call a tool.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, SseClientError> {
        let id = self.next_id().await;
        let progress_token = format!("progress_{}", id);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments,
                "_meta": {
                    "progressToken": progress_token
                }
            }
        });

        let response = self.send_request(id, request).await?;

        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            return Err(SseClientError::ToolCall {
                name: name.to_string(),
                message: message.to_string(),
            });
        }

        let result = response
            .get("result")
            .ok_or_else(|| SseClientError::Protocol("Missing result".into()))?;

        // Extract structured content if available
        if let Some(structured) = result.get("structuredContent") {
            return Ok(structured.clone());
        }

        // Fall back to content array
        if let Some(content) = result.get("content") {
            if let Some(arr) = content.as_array() {
                if let Some(first) = arr.first() {
                    if let Some(text) = first.get("text").and_then(|t| t.as_str()) {
                        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                            return Ok(parsed);
                        }
                        return Ok(Value::String(text.to_string()));
                    }
                }
            }
        }

        Ok(result.clone())
    }

    /// Request argument completions.
    pub async fn complete_argument(
        &self,
        tool_name: &str,
        argument_name: &str,
        partial: &str,
    ) -> Result<CompletionResult, SseClientError> {
        let id = self.next_id().await;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "completion/complete",
            "params": {
                "ref": {
                    "type": "ref/argument",
                    "name": tool_name,
                    "argumentName": argument_name
                },
                "argument": {
                    "name": argument_name,
                    "value": partial
                }
            }
        });

        let response = self.send_request(id, request).await?;

        let result = response
            .get("completion")
            .ok_or_else(|| SseClientError::Protocol("Missing completion".into()))?;

        serde_json::from_value(result.clone())
            .map_err(|e| SseClientError::Protocol(e.to_string()))
    }

    /// Send a request and wait for response via SSE.
    async fn send_request(&self, id: u64, request: Value) -> Result<Value, SseClientError> {
        let session_id = self
            .session_id
            .as_ref()
            .ok_or_else(|| SseClientError::Protocol("No active session".into()))?;

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
            .map_err(|e| SseClientError::Transport(e.to_string()))?;

        if response.status() != reqwest::StatusCode::ACCEPTED {
            return Err(SseClientError::Http {
                status: response.status().as_u16(),
                body: format!("Request failed"),
            });
        }

        // Wait for response from SSE stream
        timeout(Duration::from_secs(self.options.timeout_secs), rx)
            .await
            .map_err(|_| SseClientError::Timeout("response".into()))?
            .map_err(|_| SseClientError::Protocol("Response channel closed".into()))
    }

    /// Send a notification.
    async fn send_notification(&self, notification: Value) -> Result<(), SseClientError> {
        let session_id = self
            .session_id
            .as_ref()
            .ok_or_else(|| SseClientError::Protocol("No active session".into()))?;

        let post_url = format!("{}/message?sessionId={}", self.base_url, session_id);

        let response = timeout(
            Duration::from_secs(5),
            self.client
                .post(&post_url)
                .header("Content-Type", "application/json")
                .body(notification.to_string())
                .send(),
        )
        .await
        .map_err(|_| SseClientError::Timeout("notification".into()))?
        .map_err(|e| SseClientError::Transport(e.to_string()))?;

        if response.status() != reqwest::StatusCode::ACCEPTED {
            return Err(SseClientError::Http {
                status: response.status().as_u16(),
                body: format!("Notification failed"),
            });
        }

        Ok(())
    }
}

/// Background task to listen for SSE messages.
async fn listen_for_responses(
    mut stream: impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
    responses: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    session_sender: oneshot::Sender<String>,
    notification_callback: Option<NotificationCallback>,
) {
    let mut current_event_type: Option<String> = None;
    let mut current_data = String::new();
    let mut buffer = String::new();
    let mut session_sender = Some(session_sender);

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                let text = String::from_utf8_lossy(&chunk);
                buffer.push_str(&text);

                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    let trimmed = line.trim();

                    // Blank line marks end of event
                    if trimmed.is_empty() {
                        if !current_data.is_empty() {
                            // Extract session ID from endpoint event
                            if let Some(sender) = session_sender.take() {
                                if current_event_type.as_deref() == Some("endpoint") {
                                    if let Some(session_id) = extract_session_id(&current_data) {
                                        let _ = sender.send(session_id);
                                        current_data.clear();
                                        current_event_type = None;
                                        continue;
                                    }
                                }
                                session_sender = Some(sender);
                            }

                            // Process message events
                            if current_event_type.as_deref() == Some("message") {
                                if let Ok(value) = serde_json::from_str::<Value>(&current_data) {
                                    // Check if it's a notification or response
                                    if value.get("id").is_none() {
                                        // Notification
                                        if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                                            handle_notification(method, &value, &notification_callback);
                                        }
                                    } else if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                                        // Response
                                        let mut resp_map = responses.lock().await;
                                        if let Some(sender) = resp_map.remove(&id) {
                                            let _ = sender.send(value);
                                        }
                                    }
                                }
                            }

                            current_data.clear();
                        }
                        current_event_type = None;
                        continue;
                    }

                    if trimmed.starts_with("event:") {
                        current_event_type = Some(trimmed[6..].trim().to_string());
                        continue;
                    }

                    if trimmed.starts_with("data:") {
                        current_data.push_str(trimmed[5..].trim());
                    }
                }
            }
            Err(_) => break,
        }
    }
}

fn extract_session_id(data: &str) -> Option<String> {
    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
        if let Some(uri) = parsed.get("uri").and_then(|v| v.as_str()) {
            if let Some(pos) = uri.find("sessionId=") {
                return Some(uri[pos + 10..].to_string());
            }
        }
    }
    None
}

fn handle_notification(method: &str, value: &Value, callback: &Option<NotificationCallback>) {
    let Some(cb) = callback else { return };

    match method {
        "notifications/progress" => {
            if let Ok(notif) = serde_json::from_value::<ProgressNotification>(
                value.get("params").cloned().unwrap_or(Value::Null),
            ) {
                cb(Notification::Progress(notif));
            }
        }
        "notifications/message" => {
            if let Ok(msg) = serde_json::from_value::<LogMessage>(
                value.get("params").cloned().unwrap_or(Value::Null),
            ) {
                cb(Notification::Log(msg));
            }
        }
        _ => {}
    }
}

/// Errors that can occur when using the SSE client.
#[derive(Debug, thiserror::Error)]
pub enum SseClientError {
    #[error("Transport error: {0}")]
    Transport(String),

    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Timeout waiting for {0}")]
    Timeout(String),

    #[error("Tool '{name}' failed: {message}")]
    ToolCall { name: String, message: String },
}
