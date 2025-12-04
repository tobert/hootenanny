//! MCP Logger
//!
//! Sends log messages to MCP clients via SSE notifications.

use std::sync::Arc;

use axum::response::sse::Event;
use crate::session::SessionStore;
use crate::types::jsonrpc::JsonRpcMessage;
use crate::types::logging::LogMessage;

/// Logger for sending log messages to MCP clients.
#[derive(Clone)]
pub struct McpLogger {
    sessions: Arc<dyn SessionStore>,
}

impl McpLogger {
    /// Create a new MCP logger.
    pub fn new(sessions: Arc<dyn SessionStore>) -> Self {
        Self { sessions }
    }

    /// Send a log message to a specific session.
    pub async fn log(&self, session_id: &str, message: LogMessage) {
        if let Some(session) = self.sessions.get(session_id) {
            // Check if this log level should be sent
            if !session.should_log(message.level) {
                return;
            }

            // Send as notification/message
            let notification = JsonRpcMessage::notification(
                "notifications/message",
                serde_json::to_value(&message).unwrap_or_default(),
            );

            // Convert to SSE event
            if let Ok(json) = serde_json::to_string(&notification) {
                let event = Event::default().data(json);
                if let Err(e) = session.send_event(event).await {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %e,
                        "Failed to send log message"
                    );
                }
            }
        }
    }
}
