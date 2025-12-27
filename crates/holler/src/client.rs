//! ZMQ DEALER client for communicating with Hootenanny backends

use anyhow::{Context as AnyhowContext, Result};
use hooteproto::socket_config::configure_dealer;
use hooteproto::{Envelope, Payload};
use rzmq::{Context, Msg, Socket, SocketType};
use std::time::Duration;

/// A simple ZMQ DEALER client for request/reply communication
pub struct Client {
    #[allow(dead_code)]
    context: Context,
    socket: Socket,
    timeout: Duration,
}

impl Client {
    /// Connect to a ZMQ ROUTER endpoint
    pub async fn connect(endpoint: &str, timeout_ms: u64) -> Result<Self> {
        let context = Context::new()
            .with_context(|| "Failed to create ZMQ context")?;
        let socket = context
            .socket(SocketType::Dealer)
            .with_context(|| "Failed to create DEALER socket")?;

        configure_dealer(&socket, "holler-client", b"holler-client").await?;

        socket
            .connect(endpoint)
            .await
            .with_context(|| format!("Failed to connect to {}", endpoint))?;

        Ok(Self {
            context,
            socket,
            timeout: Duration::from_millis(timeout_ms),
        })
    }

    /// Send a Payload and receive the response
    pub async fn request(&self, payload: Payload) -> Result<Envelope> {
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

        // Serialize HootFrame to rzmq multipart
        let frames = frame.to_frames();
        let msgs: Vec<Msg> = frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect();

        tokio::time::timeout(self.timeout, self.socket.send_multipart(msgs))
            .await
            .context("Send timeout")?
            .context("Failed to send message")?;

        // Receive the response
        let response = tokio::time::timeout(self.timeout, self.socket.recv_multipart())
            .await
            .context("Receive timeout")?
            .context("Failed to receive response")?;

        // Parse HootFrame from response
        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
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