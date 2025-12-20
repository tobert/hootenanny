//! GardenClient - ZMQ client for chaosgarden daemon
//!
//! Connects to chaosgarden using the Jupyter-inspired 5-socket protocol over HOOT01 frames.
//! Uses JSON serialization for garden message envelopes.

use anyhow::{Context as AnyhowContext, Result};
use bytes::Bytes;
use chaosgarden::ipc::{
    ControlReply, ControlRequest, GardenEndpoints, IOPubEvent, Message, QueryReply,
    QueryRequest, ShellReply, ShellRequest,
};
use futures::stream::Stream;
use hooteproto::{Command, ContentType, HootFrame, PROTOCOL_VERSION};
use rzmq::{Context, Msg, Socket, SocketType};
use rzmq::socket::options::{LINGER, RECONNECT_IVL, ROUTING_ID, SUBSCRIBE};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Client for connecting to chaosgarden's ZMQ endpoints
pub struct GardenClient {
    session: Uuid,
    #[allow(dead_code)]
    context: Context,
    control: Arc<RwLock<Socket>>,
    shell: Arc<RwLock<Socket>>,
    iopub: Arc<RwLock<Socket>>,
    heartbeat: Arc<RwLock<Socket>>,
    query: Arc<RwLock<Socket>>,
    timeout: Duration,
}

impl GardenClient {
    /// Connect to chaosgarden at the given endpoints
    pub async fn connect(endpoints: &GardenEndpoints) -> Result<Self> {
        let session = Uuid::new_v4();

        debug!("Creating sockets for chaosgarden session {}", session);

        let context = Context::new()
            .with_context(|| "Failed to create ZMQ context")?;

        // Helper to set common socket options
        async fn set_socket_opts(socket: &Socket, name: &str) {
            if let Err(e) = socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await {
                warn!("{}: Failed to set LINGER: {}", name, e);
            }
            if let Err(e) = socket.set_option_raw(RECONNECT_IVL, &1000i32.to_ne_bytes()).await {
                warn!("{}: Failed to set RECONNECT_IVL: {}", name, e);
            }
        }

        // Create and connect all sockets
        let control = context.socket(SocketType::Dealer)
            .with_context(|| "Failed to create control socket")?;
        set_socket_opts(&control, "control").await;
        if let Err(e) = control.set_option_raw(ROUTING_ID, b"garden-control").await {
            warn!("control: Failed to set ROUTING_ID: {}", e);
        }
        control.connect(&endpoints.control).await.with_context(|| {
            format!("Failed to connect control socket to {}", endpoints.control)
        })?;

        let shell = context.socket(SocketType::Dealer)
            .with_context(|| "Failed to create shell socket")?;
        set_socket_opts(&shell, "shell").await;
        if let Err(e) = shell.set_option_raw(ROUTING_ID, b"garden-shell").await {
            warn!("shell: Failed to set ROUTING_ID: {}", e);
        }
        shell
            .connect(&endpoints.shell)
            .await
            .with_context(|| format!("Failed to connect shell socket to {}", endpoints.shell))?;

        let iopub = context.socket(SocketType::Sub)
            .with_context(|| "Failed to create iopub socket")?;
        set_socket_opts(&iopub, "iopub").await;
        // Subscribe to all messages
        if let Err(e) = iopub.set_option_raw(SUBSCRIBE, b"").await {
            warn!("iopub: Failed to subscribe: {}", e);
        }
        iopub
            .connect(&endpoints.iopub)
            .await
            .with_context(|| format!("Failed to connect iopub socket to {}", endpoints.iopub))?;

        let heartbeat = context.socket(SocketType::Req)
            .with_context(|| "Failed to create heartbeat socket")?;
        set_socket_opts(&heartbeat, "heartbeat").await;
        heartbeat
            .connect(&endpoints.heartbeat)
            .await
            .with_context(|| {
                format!(
                    "Failed to connect heartbeat socket to {}",
                    endpoints.heartbeat
                )
            })?;

        let query = context.socket(SocketType::Req)
            .with_context(|| "Failed to create query socket")?;
        set_socket_opts(&query, "query").await;
        query
            .connect(&endpoints.query)
            .await
            .with_context(|| format!("Failed to connect query socket to {}", endpoints.query))?;

        info!("Connected to chaosgarden, session={}", session);

        Ok(Self {
            session,
            context,
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
        let msgs = frames_to_msgs(&frames);

        debug!("Sending shell request ({})", request_id);

        let socket = self.shell.write().await;

        // Send
        tokio::time::timeout(self.timeout, socket.send_multipart(msgs))
            .await
            .context("Shell request send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.timeout, socket.recv_multipart())
            .await
            .context("Shell response receive timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

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
        let msgs = frames_to_msgs(&frames);

        debug!("Sending control request ({})", request_id);

        let socket = self.control.write().await;

        // Send
        tokio::time::timeout(self.timeout, socket.send_multipart(msgs))
            .await
            .context("Control request send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.timeout, socket.recv_multipart())
            .await
            .context("Control response receive timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

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
        let msgs = frames_to_msgs(&frames);

        debug!("Sending query ({})", request_id);

        let socket = self.query.write().await;

        // Send
        tokio::time::timeout(self.timeout, socket.send_multipart(msgs))
            .await
            .context("Query send timeout")??;

        // Receive
        let response = tokio::time::timeout(self.timeout, socket.recv_multipart())
            .await
            .context("Query response timeout")??;

        // Parse response
        let response_frames: Vec<Bytes> = response
            .into_iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

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
        let msgs = frames_to_msgs(&frames);

        let socket = self.heartbeat.write().await;

        // Send heartbeat
        tokio::time::timeout(timeout, socket.send_multipart(msgs))
            .await
            .context("Heartbeat send timeout")??;

        // Wait for response
        let response = tokio::time::timeout(timeout, socket.recv_multipart())
            .await
            .context("Heartbeat receive timeout")??;

        // Check for HOOT01 heartbeat reply
        let response_frames: Vec<Bytes> = response
            .iter()
            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
            .collect();

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
                    let socket = iopub.write().await;
                    socket.recv_multipart().await
                };

                match msg {
                    Ok(zmq_msgs) => {
                        let frames: Vec<Bytes> = zmq_msgs
                            .into_iter()
                            .map(|m| Bytes::from(m.data().map(|d| d.to_vec()).unwrap_or_default()))
                            .collect();

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

/// Convert Vec<Bytes> to Vec<Msg> for rzmq multipart
fn frames_to_msgs(frames: &[Bytes]) -> Vec<Msg> {
    frames.iter().map(|f| Msg::from_vec(f.to_vec())).collect()
}
