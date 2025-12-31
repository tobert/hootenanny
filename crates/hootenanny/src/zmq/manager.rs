//! GardenManager - manages the connection to chaosgarden daemon
//!
//! Wraps GardenPeer with connection management, reconnection logic,
//! and event forwarding.

use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use tracing::{debug, error, info, warn};

use hooteproto::garden::{
    Beat, ControlReply, ControlRequest, IOPubEvent, ShellReply, ShellRequest,
};
use hooteproto::{GardenEndpoints, GardenPeer};

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
/// Provides a higher-level interface than GardenPeer with:
/// - Automatic reconnection
/// - Connection state tracking
/// - Event broadcasting
pub struct GardenManager {
    endpoints: GardenEndpoints,
    client: Arc<RwLock<Option<GardenPeer>>>,
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

    /// Create with IPC endpoints in a specific directory
    ///
    /// Use this with `paths.socket_dir` from HootConfig:
    /// ```ignore
    /// let socket_dir = config.infra.paths.require_socket_dir()?;
    /// let manager = GardenManager::from_socket_dir(&socket_dir.to_string_lossy());
    /// ```
    pub fn from_socket_dir(dir: &str) -> Self {
        Self::new(GardenEndpoints::from_socket_dir(dir))
    }

    /// Create from HootConfig (recommended)
    ///
    /// This validates that socket_dir is present and configured.
    pub fn from_config(config: &hooteconf::HootConfig) -> anyhow::Result<Self> {
        let endpoints = GardenEndpoints::from_config(config)?;
        Ok(Self::new(endpoints))
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

        match GardenPeer::connect(&self.endpoints).await {
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

        // Reconnect with a fresh peer for request/control channels
        let new_client = GardenPeer::connect(&self.endpoints).await?;
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
            beat: Beat(beat),
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

    /// Get a state snapshot for local Trustfall query evaluation
    ///
    /// Returns a GardenSnapshot containing all queryable state from chaosgarden.
    /// This snapshot can be used with GardenStateAdapter to evaluate queries locally.
    pub async fn get_snapshot(&self) -> Result<hooteproto::GardenSnapshot> {
        let reply = self.request(ShellRequest::GetSnapshot).await?;
        match reply {
            ShellReply::Snapshot { snapshot } => Ok(snapshot),
            ShellReply::Error { error, .. } => {
                anyhow::bail!("chaosgarden error: {}", error)
            }
            other => {
                anyhow::bail!("unexpected reply: {:?}", other)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_garden_manager_from_socket_dir() {
        let manager = GardenManager::from_socket_dir("/tmp");
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
        let manager = GardenManager::from_socket_dir("/tmp");
        assert_eq!(manager.state().await, ConnectionState::Disconnected);
        assert!(!manager.is_connected().await);
    }
}
