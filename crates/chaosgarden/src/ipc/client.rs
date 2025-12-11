//! GardenClient - the hootenanny side of the ZMQ IPC layer
//!
//! Connects to chaosgarden daemon and provides methods for sending requests
//! and receiving events.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tracing::{debug, warn};
use uuid::Uuid;
use zeromq::{DealerSocket, ReqSocket, Socket, SocketRecv, SocketSend, SubSocket, ZmqMessage};

use crate::ipc::{
    wire, ControlReply, ControlRequest, GardenEndpoints, IOPubEvent, Message, QueryReply,
    QueryRequest, ShellReply, ShellRequest,
};

/// Client side of the chaosgarden IPC layer
///
/// Used by hootenanny to communicate with the chaosgarden daemon.
pub struct GardenClient {
    session: Uuid,
    control: DealerSocket,
    shell: DealerSocket,
    iopub: SubSocket,
    heartbeat: ReqSocket,
    query: ReqSocket,
}

impl GardenClient {
    /// Connect to a running chaosgarden daemon
    pub async fn connect(endpoints: &GardenEndpoints) -> Result<Self> {
        let session = Uuid::new_v4();

        let mut control = DealerSocket::new();
        control.connect(&endpoints.control).await.with_context(|| {
            format!("failed to connect control socket to {}", endpoints.control)
        })?;

        let mut shell = DealerSocket::new();
        shell
            .connect(&endpoints.shell)
            .await
            .with_context(|| format!("failed to connect shell socket to {}", endpoints.shell))?;

        let mut iopub = SubSocket::new();
        iopub
            .connect(&endpoints.iopub)
            .await
            .with_context(|| format!("failed to connect iopub socket to {}", endpoints.iopub))?;
        iopub
            .subscribe("")
            .await
            .context("failed to subscribe to iopub")?;

        let mut heartbeat = ReqSocket::new();
        heartbeat
            .connect(&endpoints.heartbeat)
            .await
            .with_context(|| {
                format!(
                    "failed to connect heartbeat socket to {}",
                    endpoints.heartbeat
                )
            })?;

        let mut query = ReqSocket::new();
        query
            .connect(&endpoints.query)
            .await
            .with_context(|| format!("failed to connect query socket to {}", endpoints.query))?;

        Ok(Self {
            session,
            control,
            shell,
            iopub,
            heartbeat,
            query,
        })
    }

    /// Get the session ID
    pub fn session(&self) -> Uuid {
        self.session
    }

    /// Send a shell request and wait for reply
    pub async fn request(&mut self, req: ShellRequest) -> Result<ShellReply> {
        let msg = Message::new(self.session, "shell_request", req);
        let bytes = wire::serialize(&msg)?;

        self.shell.send(ZmqMessage::from(bytes)).await?;

        let response = self.shell.recv().await?;
        let data = response.into_vec().pop().context("empty shell response")?;

        let reply_msg: Message<ShellReply> = wire::deserialize(&data)?;
        Ok(reply_msg.content)
    }

    /// Send a control request (priority channel)
    pub async fn control(&mut self, req: ControlRequest) -> Result<ControlReply> {
        let msg = Message::new(self.session, "control_request", req);
        let bytes = wire::serialize(&msg)?;

        self.control.send(ZmqMessage::from(bytes)).await?;

        let response = self.control.recv().await?;
        let data = response
            .into_vec()
            .pop()
            .context("empty control response")?;

        let reply_msg: Message<ControlReply> = wire::deserialize(&data)?;
        Ok(reply_msg.content)
    }

    /// Execute a Trustfall query
    pub async fn query(
        &mut self,
        query_str: &str,
        variables: HashMap<String, serde_json::Value>,
    ) -> Result<QueryReply> {
        let req = QueryRequest {
            query: query_str.to_string(),
            variables,
        };

        let msg = Message::new(self.session, "query_request", req);
        let bytes = wire::serialize(&msg)?;

        self.query.send(ZmqMessage::from(bytes)).await?;

        let response = self.query.recv().await?;
        let data = response.into_vec().pop().context("empty query response")?;

        let reply_msg: Message<QueryReply> = wire::deserialize(&data)?;
        Ok(reply_msg.content)
    }

    /// Subscribe to IOPub events, returning a stream
    pub fn events(mut self) -> impl Stream<Item = IOPubEvent> {
        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            loop {
                match self.iopub.recv().await {
                    Ok(zmq_msg) => {
                        let data = match zmq_msg.into_vec().pop() {
                            Some(d) => d,
                            None => continue,
                        };

                        match wire::deserialize::<IOPubEvent>(&data) {
                            Ok(msg) => {
                                if tx.send(msg.content).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("failed to deserialize iopub event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("iopub recv error: {}", e);
                        break;
                    }
                }
            }
        });

        ReceiverStream::new(rx)
    }

    /// Check if daemon is alive with ping/pong
    pub async fn ping(&mut self, timeout: Duration) -> Result<bool> {
        let ping_data = b"ping".to_vec();

        self.heartbeat
            .send(ZmqMessage::from(ping_data.clone()))
            .await?;

        let result = tokio::time::timeout(timeout, self.heartbeat.recv()).await;

        match result {
            Ok(Ok(response)) => {
                let data = response.into_vec().pop();
                debug!("heartbeat response: {:?}", data);
                Ok(true)
            }
            Ok(Err(e)) => {
                warn!("heartbeat error: {}", e);
                Ok(false)
            }
            Err(_) => {
                warn!("heartbeat timeout");
                Ok(false)
            }
        }
    }

    /// Convenience: wait for a specific event with predicate
    pub async fn wait_for<F>(
        mut self,
        predicate: F,
        timeout: Duration,
    ) -> Result<Option<IOPubEvent>>
    where
        F: Fn(&IOPubEvent) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }

            let result = tokio::time::timeout(remaining, self.iopub.recv()).await;

            match result {
                Ok(Ok(zmq_msg)) => {
                    let data = match zmq_msg.into_vec().pop() {
                        Some(d) => d,
                        None => continue,
                    };

                    match wire::deserialize::<IOPubEvent>(&data) {
                        Ok(msg) => {
                            if predicate(&msg.content) {
                                return Ok(Some(msg.content));
                            }
                        }
                        Err(e) => {
                            warn!("failed to deserialize iopub event: {}", e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    return Err(e.into());
                }
                Err(_) => {
                    return Ok(None);
                }
            }
        }
    }
}
