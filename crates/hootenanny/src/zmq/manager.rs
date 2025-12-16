//! GardenManager - manages the connection to chaosgarden daemon
//!
//! Wraps GardenClient with connection management, reconnection logic,
//! and event forwarding.

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tracing::{debug, error, info, warn};

use chaosgarden::ipc::client::GardenClient;
use chaosgarden::ipc::{
    ControlReply, ControlRequest, GardenEndpoints, IOPubEvent, QueryReply, ShellReply,
    ShellRequest,
};

/// Connection state to chaosgarden
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Manages the connection to chaosgarden daemon
///
/// Provides a higher-level interface than GardenClient with:
/// - Automatic reconnection
/// - Connection state tracking
/// - Event broadcasting
pub struct GardenManager {
    endpoints: GardenEndpoints,
    client: Arc<RwLock<Option<GardenClient>>>,
    state: Arc<RwLock<ConnectionState>>,
    event_tx: mpsc::Sender<IOPubEvent>,
    event_rx: Arc<RwLock<Option<mpsc::Receiver<IOPubEvent>>>>,
}

impl GardenManager {
    /// Create a new garden manager with the given endpoints
    pub fn new(endpoints: GardenEndpoints) -> Self {
        let (event_tx, event_rx) = mpsc::channel(256);

        Self {
            endpoints,
            client: Arc::new(RwLock::new(None)),
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
        }
    }

    /// Create with local IPC endpoints
    pub fn local() -> Self {
        Self::new(GardenEndpoints::local())
    }

    /// Create with TCP endpoints
    pub fn tcp(host: &str, base_port: u16) -> Self {
        Self::new(GardenEndpoints::tcp(host, base_port))
    }

    /// Get current connection state
    pub async fn state(&self) -> ConnectionState {
        *self.state.read().await
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        *self.state.read().await == ConnectionState::Connected
    }

    /// Connect to chaosgarden daemon
    pub async fn connect(&self) -> Result<()> {
        {
            let mut state = self.state.write().await;
            if *state == ConnectionState::Connected {
                return Ok(());
            }
            *state = ConnectionState::Connecting;
        }

        info!("Connecting to chaosgarden at {:?}", self.endpoints);

        match GardenClient::connect(&self.endpoints).await {
            Ok(client) => {
                info!("Connected to chaosgarden, session={}", client.session());
                *self.client.write().await = Some(client);
                *self.state.write().await = ConnectionState::Connected;
                Ok(())
            }
            Err(e) => {
                error!("Failed to connect to chaosgarden: {}", e);
                *self.state.write().await = ConnectionState::Disconnected;
                Err(e)
            }
        }
    }

    /// Disconnect from chaosgarden
    pub async fn disconnect(&self) {
        *self.client.write().await = None;
        *self.state.write().await = ConnectionState::Disconnected;
        info!("Disconnected from chaosgarden");
    }

    /// Send a shell request
    pub async fn request(&self, req: ShellRequest) -> Result<ShellReply> {
        self.request_with_job_id(req, None).await
    }

    /// Send a shell request with job_id for correlation
    ///
    /// The job_id is passed to chaosgarden as opaque metadata. Chaosgarden will
    /// include it in any response or IOPub events related to this request.
    pub async fn request_with_job_id(
        &self,
        req: ShellRequest,
        job_id: Option<&str>,
    ) -> Result<ShellReply> {
        let mut client_guard = self.client.write().await;
        let client = client_guard
            .as_mut()
            .context("not connected to chaosgarden")?;

        client.request_with_job_id(req, job_id).await
    }

    /// Send a control request (priority channel)
    pub async fn control(&self, req: ControlRequest) -> Result<ControlReply> {
        let mut client_guard = self.client.write().await;
        let client = client_guard
            .as_mut()
            .context("not connected to chaosgarden")?;

        client.control(req).await
    }

    /// Execute a Trustfall query
    pub async fn query(
        &self,
        query_str: &str,
        variables: std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<QueryReply> {
        let mut client_guard = self.client.write().await;
        let client = client_guard
            .as_mut()
            .context("not connected to chaosgarden")?;

        client.query(query_str, variables).await
    }

    /// Ping the daemon
    pub async fn ping(&self, timeout: Duration) -> Result<bool> {
        let mut client_guard = self.client.write().await;
        let client = client_guard
            .as_mut()
            .context("not connected to chaosgarden")?;

        client.ping(timeout).await
    }

    /// Take the event stream (can only be called once)
    ///
    /// Returns None if already taken.
    pub async fn take_events(&self) -> Option<impl Stream<Item = IOPubEvent>> {
        let rx = self.event_rx.write().await.take()?;
        Some(ReceiverStream::new(rx))
    }

    /// Start the event listener in the background
    ///
    /// This spawns a task that listens for IOPub events and forwards them
    /// to the event channel. Must be called after connect().
    pub async fn start_event_listener(&self) -> Result<()> {
        // Take ownership of the client for the event stream
        let client = {
            let mut guard = self.client.write().await;
            guard.take().context("not connected to chaosgarden")?
        };

        let event_tx = self.event_tx.clone();
        let state = self.state.clone();

        // Reconnect with a fresh client for request/control channels
        let new_client = GardenClient::connect(&self.endpoints).await?;
        *self.client.write().await = Some(new_client);

        // Spawn listener task with the old client (which owns the SUB socket)
        tokio::spawn(async move {
            use tokio_stream::StreamExt;

            let mut events = client.events();

            while let Some(event) = events.next().await {
                debug!("IOPub event: {:?}", event);

                if event_tx.send(event).await.is_err() {
                    warn!("Event channel closed, stopping listener");
                    break;
                }
            }

            warn!("IOPub stream ended");
            *state.write().await = ConnectionState::Disconnected;
        });

        Ok(())
    }

    // --- Convenience methods for common operations ---

    /// Play
    pub async fn play(&self) -> Result<ShellReply> {
        self.request(ShellRequest::Play).await
    }

    /// Pause
    pub async fn pause(&self) -> Result<ShellReply> {
        self.request(ShellRequest::Pause).await
    }

    /// Stop
    pub async fn stop(&self) -> Result<ShellReply> {
        self.request(ShellRequest::Stop).await
    }

    /// Seek to beat position
    pub async fn seek(&self, beat: f64) -> Result<ShellReply> {
        self.request(ShellRequest::Seek {
            beat: chaosgarden::ipc::Beat(beat),
        })
        .await
    }

    /// Set tempo
    pub async fn set_tempo(&self, bpm: f64) -> Result<ShellReply> {
        self.request(ShellRequest::SetTempo { bpm }).await
    }

    /// Get transport state
    pub async fn get_transport_state(&self) -> Result<ShellReply> {
        self.request(ShellRequest::GetTransportState).await
    }

    /// Emergency pause (control channel)
    pub async fn emergency_pause(&self) -> Result<ControlReply> {
        self.control(ControlRequest::EmergencyPause).await
    }

    /// Shutdown the daemon (control channel)
    pub async fn shutdown_daemon(&self) -> Result<ControlReply> {
        self.control(ControlRequest::Shutdown).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_garden_manager_new() {
        let manager = GardenManager::local();
        assert_eq!(manager.endpoints.control, "ipc:///tmp/chaosgarden-control");
    }

    #[test]
    fn test_garden_manager_tcp() {
        let manager = GardenManager::tcp("192.168.1.100", 5555);
        assert_eq!(manager.endpoints.control, "tcp://192.168.1.100:5555");
        assert_eq!(manager.endpoints.shell, "tcp://192.168.1.100:5556");
    }

    #[tokio::test]
    async fn test_initial_state_disconnected() {
        let manager = GardenManager::local();
        assert_eq!(manager.state().await, ConnectionState::Disconnected);
        assert!(!manager.is_connected().await);
    }
}
