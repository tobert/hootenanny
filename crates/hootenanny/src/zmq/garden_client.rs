//! GardenClient - ZMQ client for chaosgarden daemon
//!
//! Connects to chaosgarden using the Jupyter-inspired 5-socket protocol over HOOT01 frames.
//! Uses JSON serialization for garden message envelopes.

use anyhow::{Context, Result};
use bytes::Bytes;
use chaosgarden::ipc::{
    ControlReply, ControlRequest, GardenEndpoints, IOPubEvent, Message, QueryReply,
    QueryRequest, ShellReply, ShellRequest,
};
use futures::stream::Stream;
use hooteproto::{Command, ContentType, HootFrame, PROTOCOL_VERSION};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use zeromq::{DealerSocket, ReqSocket, Socket, SocketRecv, SocketSend, SubSocket, ZmqMessage};

/// Client for connecting to chaosgarden's ZMQ endpoints
pub struct GardenClient {
    session: Uuid,
    control: Arc<RwLock<DealerSocket>>,
    shell: Arc<RwLock<DealerSocket>>,
    iopub: Arc<RwLock<SubSocket>>,
    heartbeat: Arc<RwLock<ReqSocket>>,
    query: Arc<RwLock<ReqSocket>>,
    timeout: Duration,
}

impl GardenClient {
    /// Connect to chaosgarden at the given endpoints
    pub async fn connect(endpoints: &GardenEndpoints) -> Result<Self> {
        let session = Uuid::new_v4();

        debug!("Creating sockets for chaosgarden session {}", session);

        // Create and connect all sockets
        let mut control = DealerSocket::new();
        control.connect(&endpoints.control).await.with_context(|| {
            format!("Failed to connect control socket to {}", endpoints.control)
        })?;

        let mut shell = DealerSocket::new();
        shell
            .connect(&endpoints.shell)
            .await
            .with_context(|| format!("Failed to connect shell socket to {}", endpoints.shell))?;

        let mut iopub = SubSocket::new();
        iopub.subscribe("").await?; // Subscribe to all messages
        iopub
            .connect(&endpoints.iopub)
            .await
            .with_context(|| format!("Failed to connect iopub socket to {}", endpoints.iopub))?;

        let mut heartbeat = ReqSocket::new();
        heartbeat
            .connect(&endpoints.heartbeat)
            .await
            .with_context(|| {
                format!(
                    "Failed to connect heartbeat socket to {}",
                    endpoints.heartbeat
                )
            })?;

        let mut query = ReqSocket::new();
        query
            .connect(&endpoints.query)
            .await
            .with_context(|| format!("Failed to connect query socket to {}", endpoints.query))?;

        info!("Connected to chaosgarden, session={}", session);

        Ok(Self {
            session,
            control: Arc::new(RwLock::new(control)),
            shell: Arc::new(RwLock::new(shell)),
            iopub: Arc::new(RwLock::new(iopub)),
            heartbeat: Arc::new(RwLock::new(heartbeat)),
            query: Arc::new(RwLock::new(query)),
            timeout: Duration::from_secs(30),
        })
    }

    /// Get the session ID
    pub fn session(&self) -> Uuid {
        self.session
    }

    /// Send a shell request
    #[allow(dead_code)]
    pub async fn request(&self, req: ShellRequest) -> Result<ShellReply> {
        self.request_with_job_id(req, None).await
    }

    /// Send a shell request with job_id for correlation
    pub async fn request_with_job_id(
        &self,
        req: ShellRequest,
        _job_id: Option<&str>,
    ) -> Result<ShellReply> {
        let msg = Message::new(self.session, "shell_request", req);
        let msg_json = serde_json::to_vec(&msg).context("Failed to serialize shell request")?;

        let request_id = msg.header.msg_id;

        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::Json,
            request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: Bytes::from(msg_json),
        };

        let frames = frame.to_frames();
        let zmq_msg = frames_to_zmq_message(&frames);

        debug!("Sending shell request ({})", request_id);

        let mut socket = self.shell.write().await;

        // Send
        tokio::time::timeout(self.timeout, socket.send(zmq_msg))
            .await
            .context("Shell request send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.timeout, socket.recv())
            .await
            .context("Shell response receive timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response.into_vec();

        let response_frame =
            HootFrame::from_frames(&response_frames).context("Failed to parse response frame")?;

        if response_frame.content_type != ContentType::Json {
            anyhow::bail!(
                "Expected JSON response, got {:?}",
                response_frame.content_type
            );
        }

        let response_msg: Message<ShellReply> = serde_json::from_slice(&response_frame.body)
            .context("Failed to deserialize shell reply")?;

        Ok(response_msg.content)
    }

    /// Send a control request (priority channel)
    pub async fn control(&self, req: ControlRequest) -> Result<ControlReply> {
        let msg = Message::new(self.session, "control_request", req);
        let msg_json = serde_json::to_vec(&msg).context("Failed to serialize control request")?;

        let request_id = msg.header.msg_id;

        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::Json,
            request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: Bytes::from(msg_json),
        };

        let frames = frame.to_frames();
        let zmq_msg = frames_to_zmq_message(&frames);

        debug!("Sending control request ({})", request_id);

        let mut socket = self.control.write().await;

        // Send
        tokio::time::timeout(self.timeout, socket.send(zmq_msg))
            .await
            .context("Control request send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.timeout, socket.recv())
            .await
            .context("Control response receive timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response.into_vec();

        let response_frame =
            HootFrame::from_frames(&response_frames).context("Failed to parse response frame")?;

        if response_frame.content_type != ContentType::Json {
            anyhow::bail!(
                "Expected JSON response, got {:?}",
                response_frame.content_type
            );
        }

        let response_msg: Message<ControlReply> = serde_json::from_slice(&response_frame.body)
            .context("Failed to deserialize control reply")?;

        Ok(response_msg.content)
    }

    /// Execute a Trustfall query
    pub async fn query(
        &self,
        query_str: &str,
        variables: HashMap<String, serde_json::Value>,
    ) -> Result<QueryReply> {
        let req = QueryRequest {
            query: query_str.to_string(),
            variables,
        };

        let msg = Message::new(self.session, "query_request", req);
        let msg_json = serde_json::to_vec(&msg).context("Failed to serialize query request")?;

        let request_id = msg.header.msg_id;

        let frame = HootFrame {
            command: Command::Request,
            content_type: ContentType::Json,
            request_id,
            service: "chaosgarden".to_string(),
            traceparent: None,
            body: Bytes::from(msg_json),
        };

        let frames = frame.to_frames();
        let zmq_msg = frames_to_zmq_message(&frames);

        debug!("Sending query ({})", request_id);

        let mut socket = self.query.write().await;

        // Send
        tokio::time::timeout(self.timeout, socket.send(zmq_msg))
            .await
            .context("Query send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.timeout, socket.recv())
            .await
            .context("Query response timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response.into_vec();

        let response_frame =
            HootFrame::from_frames(&response_frames).context("Failed to parse response frame")?;

        if response_frame.content_type != ContentType::Json {
            anyhow::bail!(
                "Expected JSON response, got {:?}",
                response_frame.content_type
            );
        }

        let response_msg: Message<QueryReply> = serde_json::from_slice(&response_frame.body)
            .context("Failed to deserialize query reply")?;

        Ok(response_msg.content)
    }

    /// Ping the daemon via heartbeat
    pub async fn ping(&self, timeout: Duration) -> Result<bool> {
        let frame = HootFrame::heartbeat("chaosgarden");
        let frames = frame.to_frames();
        let msg = frames_to_zmq_message(&frames);

        let mut socket = self.heartbeat.write().await;

        // Send heartbeat
        tokio::time::timeout(timeout, socket.send(msg))
            .await
            .context("Heartbeat send timeout")??;

        // Wait for response
        let response = tokio::time::timeout(timeout, socket.recv())
            .await
            .context("Heartbeat receive timeout")??;

        // Check for HOOT01 heartbeat reply
        let response_frames: Vec<Bytes> =
            response.iter().map(|f| Bytes::copy_from_slice(f)).collect();

        if response_frames
            .iter()
            .any(|f| f.as_ref() == PROTOCOL_VERSION)
        {
            match HootFrame::from_frames(&response_frames) {
                Ok(resp_frame) if resp_frame.command == Command::Heartbeat => Ok(true),
                Ok(_) => Ok(true), // Got a different command - still alive
                Err(e) => {
                    warn!("Heartbeat parse error: {}", e);
                    Ok(false)
                }
            }
        } else {
            // Legacy response - still indicates liveness
            Ok(true)
        }
    }

    /// Get IOPub event stream
    pub fn events(&self) -> Pin<Box<dyn Stream<Item = IOPubEvent> + Send + 'static>> {
        let iopub = self.iopub.clone();

        Box::pin(async_stream::stream! {
            loop {
                let msg = {
                    let mut socket = iopub.write().await;
                    socket.recv().await
                };

                match msg {
                    Ok(zmq_msg) => {
                        let frames: Vec<Bytes> = zmq_msg.into_vec();

                        // Skip subscription filter frame if present
                        let frame_result = if frames.len() > 1 && frames[0].is_empty() {
                            HootFrame::from_frames(&frames[1..])
                        } else {
                            HootFrame::from_frames(&frames)
                        };

                        match frame_result {
                            Ok(frame) if frame.content_type == ContentType::Json => {
                                match serde_json::from_slice::<Message<IOPubEvent>>(&frame.body) {
                                    Ok(msg) => yield msg.content,
                                    Err(e) => {
                                        error!("Failed to deserialize IOPub event: {}", e);
                                    }
                                }
                            }
                            Ok(frame) => {
                                warn!("Unexpected IOPub content type: {:?}", frame.content_type);
                            }
                            Err(e) => {
                                error!("Failed to parse IOPub frame: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("IOPub socket error: {}", e);
                        break;
                    }
                }
            }
        })
    }
}

/// Convert Vec<Bytes> to ZmqMessage
fn frames_to_zmq_message(frames: &[Bytes]) -> ZmqMessage {
    if frames.is_empty() {
        return ZmqMessage::from(Vec::<u8>::new());
    }

    let mut msg = ZmqMessage::from(frames[0].to_vec());
    for frame in frames.iter().skip(1) {
        msg.push_back(frame.to_vec().into());
    }
    msg
}
