//! Sampling Client
//!
//! Handles server-initiated LLM sampling requests with bidirectional communication.
//! The server sends sampling/createMessage requests to the client and waits for responses.

use dashmap::DashMap;
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::types::jsonrpc::JsonRpcMessage;
use crate::types::sampling::{SamplingRequest, SamplingResponse};
use crate::session::{SessionRef, SendError};

/// Error type for sampling operations
#[derive(Debug, Clone)]
pub enum SamplingError {
    /// Session not found
    SessionNotFound,
    /// Session has no active SSE connection
    NotConnected,
    /// Failed to send request to client
    SendFailed(String),
    /// Request timed out waiting for response
    Timeout,
    /// Response channel was closed
    ChannelClosed,
    /// Failed to serialize request
    SerializationError(String),
}

impl std::fmt::Display for SamplingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SamplingError::SessionNotFound => write!(f, "session not found"),
            SamplingError::NotConnected => write!(f, "session has no SSE connection"),
            SamplingError::SendFailed(e) => write!(f, "failed to send: {}", e),
            SamplingError::Timeout => write!(f, "sampling request timed out"),
            SamplingError::ChannelClosed => write!(f, "response channel closed"),
            SamplingError::SerializationError(e) => write!(f, "serialization error: {}", e),
        }
    }
}

impl std::error::Error for SamplingError {}

/// Client for sending sampling requests to the connected MCP client
pub struct SamplingClient {
    /// Pending sampling requests awaiting responses
    pending: DashMap<String, oneshot::Sender<SamplingResponse>>,
}

impl SamplingClient {
    /// Create a new sampling client
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
        }
    }

    /// Send a sampling request and wait for response
    ///
    /// # Arguments
    /// * `session` - The session to send the request through
    /// * `request` - The sampling request to send
    /// * `timeout` - Optional timeout (default: 60 seconds)
    pub async fn sample(
        &self,
        session: SessionRef<'_>,
        request: SamplingRequest,
        timeout: Option<Duration>,
    ) -> Result<SamplingResponse, SamplingError> {
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();

        // Store the pending request
        self.pending.insert(request_id.clone(), tx);

        // Serialize the request
        let params = serde_json::to_value(&request)
            .map_err(|e| SamplingError::SerializationError(e.to_string()))?;

        // Create JSON-RPC request message
        let message = JsonRpcMessage::request(&request_id, "sampling/createMessage", params);

        // Serialize message to JSON
        let json = serde_json::to_string(&message)
            .map_err(|e| SamplingError::SerializationError(e.to_string()))?;

        // Send via SSE
        let event = axum::response::sse::Event::default()
            .event("message")
            .data(json);

        session
            .send_event(event)
            .await
            .map_err(|e| match e {
                SendError::NotConnected => SamplingError::NotConnected,
                SendError::ChannelClosed => SamplingError::SendFailed("channel closed".to_string()),
            })?;

        // Wait for response with timeout
        let timeout_duration = timeout.unwrap_or(Duration::from_secs(60));
        match tokio::time::timeout(timeout_duration, rx).await {
            Ok(Ok(response)) => {
                tracing::debug!(request_id = %request_id, "Sampling request completed");
                Ok(response)
            }
            Ok(Err(_)) => {
                self.pending.remove(&request_id);
                Err(SamplingError::ChannelClosed)
            }
            Err(_) => {
                self.pending.remove(&request_id);
                tracing::warn!(request_id = %request_id, "Sampling request timed out");
                Err(SamplingError::Timeout)
            }
        }
    }

    /// Handle incoming sampling response from client
    ///
    /// Matches the response to a pending request by ID and sends it through the channel.
    pub fn handle_response(&self, id: &str, result: SamplingResponse) {
        if let Some((_, tx)) = self.pending.remove(id) {
            tracing::debug!(request_id = %id, "Received sampling response");
            let _ = tx.send(result);
        } else {
            tracing::warn!(request_id = %id, "Received sampling response for unknown request");
        }
    }

    /// Get the number of pending sampling requests
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for SamplingClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_client_creation() {
        let client = SamplingClient::new();
        assert_eq!(client.pending_count(), 0);
    }

    #[test]
    fn test_sampling_error_display() {
        let err = SamplingError::Timeout;
        assert_eq!(err.to_string(), "sampling request timed out");
    }
}
