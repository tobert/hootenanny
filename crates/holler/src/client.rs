//! ZMQ DEALER client for communicating with Hootenanny backends

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use hooteproto::socket_config::{create_dealer_and_connect, ZmqContext, Multipart};
use hooteproto::{
    capnp_envelope_to_payload, envelope_capnp, payload_to_capnp_envelope, Command, ContentType,
    Envelope, HootFrame, Payload,
};
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Boxed sink type for sending messages
type BoxedSink = Pin<Box<dyn futures::Sink<Multipart, Error = tmq::TmqError> + Send>>;

/// Boxed stream type for receiving messages
type BoxedStream =
    Pin<Box<dyn futures::Stream<Item = Result<Multipart, tmq::TmqError>> + Send + Unpin>>;

/// Convert tmq Multipart to Vec<Bytes> for frame processing
fn multipart_to_frames(mp: Multipart) -> Vec<Bytes> {
    mp.into_iter()
        .map(|msg| Bytes::from(msg.to_vec()))
        .collect()
}

/// Convert Vec<Bytes> frames to tmq Multipart
fn frames_to_multipart(frames: &[Bytes]) -> Multipart {
    frames
        .iter()
        .map(|f| f.to_vec())
        .collect::<Vec<_>>()
        .into()
}

/// A simple ZMQ DEALER client for request/reply communication
pub struct Client {
    #[allow(dead_code)]
    context: ZmqContext,
    socket_tx: Mutex<BoxedSink>,
    socket_rx: Mutex<BoxedStream>,
    timeout: Duration,
}

impl Client {
    /// Connect to a ZMQ ROUTER endpoint
    pub async fn connect(endpoint: &str, timeout_ms: u64) -> Result<Self> {
        let context = ZmqContext::new();
        let socket = create_dealer_and_connect(&context, endpoint, b"holler-client", "holler-client")?;

        let (tx, rx) = socket.split();

        Ok(Self {
            context,
            socket_tx: Mutex::new(Box::pin(tx)),
            socket_rx: Mutex::new(Box::pin(rx)),
            timeout: Duration::from_millis(timeout_ms),
        })
    }

    /// Send a Payload and receive the response
    pub async fn request(&self, payload: Payload) -> Result<Envelope> {
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

        // Serialize HootFrame to multipart
        let frames = frame.to_frames();
        let multipart = frames_to_multipart(&frames);

        // Send with timeout
        {
            let mut tx = self.socket_tx.lock().await;
            tokio::time::timeout(self.timeout, tx.send(multipart))
                .await
                .context("Send timeout")?
                .map_err(|e| anyhow::anyhow!("Failed to send message: {}", e))?;
        }

        // Receive the response with timeout
        let response = {
            let mut rx = self.socket_rx.lock().await;
            tokio::time::timeout(self.timeout, rx.next())
                .await
                .context("Receive timeout")?
                .ok_or_else(|| anyhow::anyhow!("Socket stream ended"))?
                .map_err(|e| anyhow::anyhow!("Failed to receive response: {}", e))?
        };

        // Parse HootFrame from response
        let response_frames = multipart_to_frames(response);

        let response_frame = HootFrame::from_frames(&response_frames)
            .context("Failed to parse response HootFrame")?;

        // Parse Cap'n Proto response
        let reader = response_frame
            .read_capnp()
            .context("Failed to read capnp from response")?;

        let envelope_reader = reader
            .get_root::<envelope_capnp::envelope::Reader>()
            .context("Failed to get envelope root")?;

        let response_payload =
            capnp_envelope_to_payload(envelope_reader).context("Failed to convert capnp to payload")?;

        Ok(Envelope {
            id: request_id,
            payload: response_payload,
            traceparent: response_frame.traceparent.clone(),
        })
    }
}
