//! LuanetteClient - ZMQ DEALER client for luanette backend
//!
//! Connects to luanette's ZMQ ROUTER for Lua scripting tools.
//! Used by hootenanny to proxy lua_*, job_*, script_* requests.

use anyhow::{Context, Result};
use bytes::Bytes;
use hooteproto::{Command, Envelope, HootFrame, Payload, PROTOCOL_VERSION};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use zeromq::{DealerSocket, Socket, SocketRecv, SocketSend, ZmqMessage};

/// Connection state to luanette
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected = 0,
    Connecting = 1,
    Connected = 2,
    Dead = 3,
}

impl ConnectionState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => ConnectionState::Disconnected,
            1 => ConnectionState::Connecting,
            2 => ConnectionState::Connected,
            3 => ConnectionState::Dead,
            _ => ConnectionState::Disconnected,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionState::Disconnected => "disconnected",
            ConnectionState::Connecting => "connecting",
            ConnectionState::Connected => "connected",
            ConnectionState::Dead => "dead",
        }
    }
}

/// Health tracking for luanette connection
pub struct HealthTracker {
    pub state: AtomicU8,
    pub consecutive_failures: AtomicU32,
    pub last_message: RwLock<Option<Instant>>,
}

impl HealthTracker {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(ConnectionState::Disconnected as u8),
            consecutive_failures: AtomicU32::new(0),
            last_message: RwLock::new(None),
        }
    }

    pub fn get_state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Relaxed))
    }

    pub fn set_state(&self, state: ConnectionState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }

    pub fn is_connected(&self) -> bool {
        self.get_state() == ConnectionState::Connected
    }

    pub async fn record_success(&self) {
        *self.last_message.write().await = Some(Instant::now());
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.set_state(ConnectionState::Connected);
    }

    pub fn record_failure(&self) -> u32 {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub async fn health_summary(&self) -> serde_json::Value {
        let last_recv = self.last_message.read().await;
        let last_recv_ago = last_recv.map(|t| t.elapsed().as_secs());

        serde_json::json!({
            "state": self.get_state().as_str(),
            "connected": self.is_connected(),
            "consecutive_failures": self.consecutive_failures.load(Ordering::Relaxed),
            "last_message_secs_ago": last_recv_ago,
        })
    }
}

impl Default for HealthTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Client for connecting to luanette's ZMQ ROUTER
pub struct LuanetteClient {
    endpoint: String,
    socket: RwLock<DealerSocket>,
    pub health: Arc<HealthTracker>,
    timeout: Duration,
}

impl LuanetteClient {
    /// Connect to luanette at the given endpoint
    pub async fn connect(endpoint: &str, timeout_ms: u64) -> Result<Self> {
        debug!("Creating DEALER socket for luanette");
        let mut socket = DealerSocket::new();

        // Connect with timeout
        tokio::time::timeout(Duration::from_secs(5), socket.connect(endpoint))
            .await
            .with_context(|| format!("Timeout connecting to luanette at {}", endpoint))?
            .with_context(|| format!("Failed to connect to luanette at {}", endpoint))?;

        info!("Connected to luanette at {}", endpoint);

        let health = Arc::new(HealthTracker::new());
        health.set_state(ConnectionState::Connected);

        Ok(Self {
            endpoint: endpoint.to_string(),
            socket: RwLock::new(socket),
            health,
            timeout: Duration::from_millis(timeout_ms),
        })
    }

    /// Get the endpoint
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.health.is_connected()
    }

    /// Send a payload to luanette and wait for response
    pub async fn request(&self, payload: Payload, traceparent: Option<String>) -> Result<Payload> {
        let envelope = if let Some(tp) = traceparent {
            Envelope::new(payload).with_traceparent(tp)
        } else {
            Envelope::new(payload)
        };

        let bytes = rmp_serde::to_vec(&envelope)?;
        debug!("Sending to luanette ({} bytes)", bytes.len());

        let mut socket = self.socket.write().await;

        // Send
        let msg = ZmqMessage::from(bytes);
        tokio::time::timeout(self.timeout, socket.send(msg))
            .await
            .context("Send timeout")?
            .context("Failed to send")?;

        // Receive
        let response = tokio::time::timeout(self.timeout, socket.recv())
            .await
            .context("Receive timeout")?
            .context("Failed to receive")?;

        let response_bytes = response.get(0).context("Empty response")?;

        let response_envelope: Envelope = rmp_serde::from_slice(response_bytes)
            .with_context(|| "Failed to deserialize response")?;

        self.health.record_success().await;
        Ok(response_envelope.payload)
    }

    /// Send a HOOT01 heartbeat and wait for response
    pub async fn send_heartbeat(&self) -> Result<()> {
        let frame = HootFrame::heartbeat("luanette");
        let frames = frame.to_frames();
        let msg = frames_to_zmq_message(&frames);

        let mut socket = self.socket.write().await;

        // Send heartbeat
        tokio::time::timeout(Duration::from_secs(2), socket.send(msg))
            .await
            .context("Heartbeat send timeout")?
            .context("Heartbeat send failed")?;

        // Wait for response
        let response = tokio::time::timeout(Duration::from_secs(2), socket.recv())
            .await
            .context("Heartbeat receive timeout")?
            .context("Heartbeat receive failed")?;

        // Parse response - check for HOOT01 heartbeat reply
        let response_frames: Vec<Bytes> = response
            .iter()
            .map(|f| Bytes::copy_from_slice(f))
            .collect();

        if response_frames.iter().any(|f| f.as_ref() == PROTOCOL_VERSION) {
            match HootFrame::from_frames(&response_frames) {
                Ok(resp_frame) if resp_frame.command == Command::Heartbeat => {
                    self.health.record_success().await;
                    Ok(())
                }
                Ok(_) => {
                    // Got a different command - still alive
                    self.health.record_success().await;
                    Ok(())
                }
                Err(e) => {
                    self.health.record_failure();
                    Err(anyhow::anyhow!("Parse error: {}", e))
                }
            }
        } else {
            // Legacy response - still indicates liveness
            self.health.record_success().await;
            Ok(())
        }
    }
}

/// Convert a Vec<Bytes> to a ZmqMessage
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

/// Spawn a heartbeat task for luanette connection monitoring
pub fn spawn_heartbeat_task(
    client: Arc<LuanetteClient>,
    interval: Duration,
    max_failures: u32,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        info!("💓 Heartbeat monitoring started for luanette");

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match client.send_heartbeat().await {
                        Ok(()) => {
                            debug!("luanette: heartbeat OK");
                        }
                        Err(e) => {
                            let failures = client.health.record_failure();
                            warn!("luanette: heartbeat failed: {} ({}/{})", e, failures, max_failures);

                            if failures >= max_failures {
                                let state = client.health.get_state();
                                if state != ConnectionState::Dead {
                                    client.health.set_state(ConnectionState::Dead);
                                    warn!("luanette: marked as DEAD after {} consecutive failures", failures);
                                }
                            }
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("luanette: heartbeat task shutting down");
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_state() {
        let tracker = HealthTracker::new();
        assert_eq!(tracker.get_state(), ConnectionState::Disconnected);

        tracker.set_state(ConnectionState::Connected);
        assert_eq!(tracker.get_state(), ConnectionState::Connected);
        assert!(tracker.is_connected());

        tracker.set_state(ConnectionState::Dead);
        assert!(!tracker.is_connected());
    }

    #[test]
    fn test_failure_tracking() {
        let tracker = HealthTracker::new();
        assert_eq!(tracker.consecutive_failures.load(Ordering::Relaxed), 0);

        assert_eq!(tracker.record_failure(), 1);
        assert_eq!(tracker.record_failure(), 2);
        assert_eq!(tracker.record_failure(), 3);
    }
}
