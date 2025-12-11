//! GardenServer - the daemon side of the ZMQ IPC layer
//!
//! Binds to all 5 sockets and handles incoming requests.

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use zeromq::{PubSocket, RepSocket, RouterSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

use crate::ipc::{
    wire, ControlReply, ControlRequest, ExecutionState, GardenEndpoints, IOPubEvent, Message,
    QueryReply, QueryRequest, ShellReply, ShellRequest,
};

/// Handler trait for processing incoming requests
///
/// Implement this trait in the daemon to handle messages.
pub trait Handler: Send + Sync + 'static {
    fn handle_shell(&self, req: ShellRequest) -> ShellReply;
    fn handle_control(&self, req: ControlRequest) -> ControlReply;
    fn handle_query(&self, req: QueryRequest) -> QueryReply;
}

/// Server side of the chaosgarden IPC layer
pub struct GardenServer {
    control: RouterSocket,
    shell: RouterSocket,
    iopub: PubSocket,
    heartbeat: RepSocket,
    query: RepSocket,
    shutdown_tx: broadcast::Sender<()>,
}

impl GardenServer {
    /// Bind to all endpoints and create the server
    pub async fn bind(endpoints: &GardenEndpoints) -> Result<Self> {
        let mut control = RouterSocket::new();
        control
            .bind(&endpoints.control)
            .await
            .with_context(|| format!("failed to bind control socket to {}", endpoints.control))?;
        info!("control socket bound to {}", endpoints.control);

        let mut shell = RouterSocket::new();
        shell
            .bind(&endpoints.shell)
            .await
            .with_context(|| format!("failed to bind shell socket to {}", endpoints.shell))?;
        info!("shell socket bound to {}", endpoints.shell);

        let mut iopub = PubSocket::new();
        iopub
            .bind(&endpoints.iopub)
            .await
            .with_context(|| format!("failed to bind iopub socket to {}", endpoints.iopub))?;
        info!("iopub socket bound to {}", endpoints.iopub);

        let mut heartbeat = RepSocket::new();
        heartbeat
            .bind(&endpoints.heartbeat)
            .await
            .with_context(|| {
                format!("failed to bind heartbeat socket to {}", endpoints.heartbeat)
            })?;
        info!("heartbeat socket bound to {}", endpoints.heartbeat);

        let mut query = RepSocket::new();
        query
            .bind(&endpoints.query)
            .await
            .with_context(|| format!("failed to bind query socket to {}", endpoints.query))?;
        info!("query socket bound to {}", endpoints.query);

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            control,
            shell,
            iopub,
            heartbeat,
            query,
            shutdown_tx,
        })
    }

    /// Publish an event on the IOPub channel
    pub async fn publish(&mut self, event: IOPubEvent) -> Result<()> {
        let session = uuid::Uuid::nil();
        let msg = Message::new(session, "iopub_event", event);
        let bytes = wire::serialize(&msg)?;

        let zmq_msg = ZmqMessage::from(bytes);
        self.iopub
            .send(zmq_msg)
            .await
            .context("failed to publish iopub event")?;

        Ok(())
    }

    /// Broadcast execution state change
    pub async fn set_state(&mut self, state: ExecutionState) -> Result<()> {
        self.publish(IOPubEvent::Status {
            execution_state: state,
        })
        .await
    }

    /// Get a shutdown signal receiver
    pub fn shutdown_signal(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Run the server event loop with the given handler
    pub async fn run<H: Handler>(mut self, handler: Arc<H>) -> Result<()> {
        info!("chaosgarden server starting");
        self.set_state(ExecutionState::Starting).await?;

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        self.set_state(ExecutionState::Idle).await?;
        info!("chaosgarden server ready");

        loop {
            tokio::select! {
                // Control channel (priority)
                result = self.control.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.handle_control_message(msg, &handler).await {
                                error!("error handling control message: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("control socket error: {}", e);
                        }
                    }
                }

                // Shell channel
                result = self.shell.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.handle_shell_message(msg, &handler).await {
                                error!("error handling shell message: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("shell socket error: {}", e);
                        }
                    }
                }

                // Heartbeat channel
                result = self.heartbeat.recv() => {
                    match result {
                        Ok(msg) => {
                            debug!("heartbeat received");
                            if let Err(e) = self.heartbeat.send(msg).await {
                                warn!("heartbeat reply failed: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("heartbeat socket error: {}", e);
                        }
                    }
                }

                // Query channel
                result = self.query.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.handle_query_message(msg, &handler).await {
                                error!("error handling query message: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("query socket error: {}", e);
                        }
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("shutdown signal received");
                    break;
                }
            }
        }

        self.set_state(ExecutionState::ShuttingDown).await?;
        info!("chaosgarden server shutting down");
        Ok(())
    }

    async fn handle_control_message<H: Handler>(
        &mut self,
        zmq_msg: ZmqMessage,
        handler: &Arc<H>,
    ) -> Result<()> {
        let frames: Vec<_> = zmq_msg.into_vec();
        if frames.len() < 2 {
            return Ok(());
        }

        let identity = frames[0].clone();
        let data = &frames[frames.len() - 1];

        let msg: Message<ControlRequest> = wire::deserialize(data)?;
        debug!("control request: {:?}", msg.content);

        self.set_state(ExecutionState::Busy).await?;

        let should_shutdown = matches!(msg.content, ControlRequest::Shutdown);
        let reply = handler.handle_control(msg.content);

        let reply_msg = Message::reply(&msg.header, "control_reply", reply);
        let reply_bytes = wire::serialize(&reply_msg)?;

        let mut response = ZmqMessage::from(identity);
        response.push_back(reply_bytes.into());
        self.control.send(response).await?;

        if should_shutdown {
            let _ = self.shutdown_tx.send(());
        } else {
            self.set_state(ExecutionState::Idle).await?;
        }

        Ok(())
    }

    async fn handle_shell_message<H: Handler>(
        &mut self,
        zmq_msg: ZmqMessage,
        handler: &Arc<H>,
    ) -> Result<()> {
        let frames: Vec<_> = zmq_msg.into_vec();
        if frames.len() < 2 {
            return Ok(());
        }

        let identity = frames[0].clone();
        let data = &frames[frames.len() - 1];

        let msg: Message<ShellRequest> = wire::deserialize(data)?;
        debug!("shell request: {:?}", msg.content);

        self.set_state(ExecutionState::Busy).await?;

        let reply = handler.handle_shell(msg.content);

        let reply_msg = Message::reply(&msg.header, "shell_reply", reply);
        let reply_bytes = wire::serialize(&reply_msg)?;

        let mut response = ZmqMessage::from(identity);
        response.push_back(reply_bytes.into());
        self.shell.send(response).await?;

        self.set_state(ExecutionState::Idle).await?;

        Ok(())
    }

    async fn handle_query_message<H: Handler>(
        &mut self,
        zmq_msg: ZmqMessage,
        handler: &Arc<H>,
    ) -> Result<()> {
        let data = zmq_msg.into_vec().pop().context("empty query message")?;

        let msg: Message<QueryRequest> = wire::deserialize(&data)?;
        debug!("query request: {}", msg.content.query);

        let reply = handler.handle_query(msg.content);

        let reply_msg = Message::reply(&msg.header, "query_reply", reply);
        let reply_bytes = wire::serialize(&reply_msg)?;

        self.query.send(ZmqMessage::from(reply_bytes)).await?;

        Ok(())
    }

    /// Trigger shutdown
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}
