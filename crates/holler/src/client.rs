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
        use bytes::Bytes;
        use hooteproto::{payload_to_capnp_envelope, capnp_envelope_to_payload, Command, ContentType, HootFrame};
        use uuid::Uuid;

        // Generate request ID
        let request_id = Uuid::new_v4();

        // Convert payload to Cap'n Proto envelope
        let message = payload_to_capnp_envelope(request_id, &payload)
            .context("Failed to convert payload to capnp")?;

        // Serialize to bytes
        let body_bytes = capnp::serialize::write_message_to_words(&message);

        // Create HootFrame
        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::CapnProto,
            request_id,
            service: "hootenanny".to_string(),
            traceparent: None,
            body: Bytes::from(body_bytes),
        };

        // Serialize HootFrame to ZMQ message
        let frames = frame.to_frames();
        let mut msg = ZmqMessage::from(Vec::<u8>::new());
        for (i, frame) in frames.iter().enumerate() {
            if i == 0 {
                msg = ZmqMessage::from(frame.to_vec());
            } else {
                msg.push_back(frame.to_vec().into());
            }
        }

        tokio::time::timeout(self.timeout, self.socket.send(msg))
            .await
            .context("Send timeout")?
            .context("Failed to send message")?;

        // Receive the response
        let response = tokio::time::timeout(self.timeout, self.socket.recv())
            .await
            .context("Receive timeout")?
            .context("Failed to receive response")?;

        // Parse HootFrame from response
        let response_frames: Vec<Bytes> = response
            .into_vec()
            .into_iter()
            .map(|bytes| Bytes::from(bytes))
            .collect();

        let response_frame = HootFrame::from_frames(&response_frames)
            .context("Failed to parse response HootFrame")?;

        // Parse Cap'n Proto response
        let reader = response_frame.read_capnp()
            .context("Failed to read capnp from response")?;

        let envelope_reader = reader.get_root::<hooteproto::envelope_capnp::envelope::Reader>()
            .context("Failed to get envelope root")?;

        let response_payload = capnp_envelope_to_payload(envelope_reader)
            .context("Failed to convert capnp to payload")?;

        Ok(Envelope {
            id: request_id,
            payload: response_payload,
            traceparent: response_frame.traceparent.clone(),
        })
    }
}