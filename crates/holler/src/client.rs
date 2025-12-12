//! ZMQ DEALER client for communicating with Hootenanny backends

use anyhow::{Context, Result};
use hooteproto::{Envelope, Payload};
use std::time::Duration;
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

/// A simple ZMQ DEALER client for request/reply communication
pub struct Client {
    socket: DealerSocket,
    timeout: Duration,
}

impl Client {
    /// Connect to a ZMQ ROUTER endpoint
    pub async fn connect(endpoint: &str, timeout_ms: u64) -> Result<Self> {
        let mut socket = DealerSocket::new();
        socket
            .connect(endpoint)
            .await
            .with_context(|| format!("Failed to connect to {}", endpoint))?;

        Ok(Self {
            socket,
            timeout: Duration::from_millis(timeout_ms),
        })
    }

    /// Send a Payload and receive the response
    pub async fn request(&mut self, payload: Payload) -> Result<Envelope> {
        let envelope = Envelope::new(payload);
        
        // Serialize to MsgPack
        let bytes = rmp_serde::to_vec(&envelope)?;

        // Send the message
        let msg = ZmqMessage::from(bytes);
        tokio::time::timeout(self.timeout, self.socket.send(msg))
            .await
            .context("Send timeout")?
            .context("Failed to send message")?;

        // Receive the response
        let response = tokio::time::timeout(self.timeout, self.socket.recv())
            .await
            .context("Receive timeout")?
            .context("Failed to receive response")?;

        // Parse the response
        let response_bytes = response
            .get(0)
            .context("Empty response")?;
            
        let response_envelope: Envelope = rmp_serde::from_slice(response_bytes)
            .with_context(|| "Failed to deserialize MsgPack response")?;

        Ok(response_envelope)
    }
}