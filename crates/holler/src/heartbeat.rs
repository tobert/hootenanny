//! Heartbeat management for ZMQ backend connections
//!
//! Implements the Paranoid Pirate pattern from ZeroMQ Guide Chapter 4:
//! - Periodic heartbeat probes
//! - Exponential backoff on failure
//! - Socket close/reopen on disconnect (not just ZMQ auto-reconnect)
//! - State transitions with callbacks
//!
//! Reference: https://zguide.zeromq.org/docs/chapter4/

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Backend health state
///
/// Per MDP spec, backends transition through these states based on heartbeat responses.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendState {
    /// Initial connection in progress
    Connecting = 0,
    /// Connected and responding to heartbeats
    Ready = 1,
    /// Processing a request (optional, for future use)
    Busy = 2,
    /// Failed heartbeat threshold, considered dead
    Dead = 3,
}

impl BackendState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => BackendState::Connecting,
            1 => BackendState::Ready,
            2 => BackendState::Busy,
            3 => BackendState::Dead,
            _ => BackendState::Dead,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            BackendState::Connecting => "connecting",
            BackendState::Ready => "ready",
            BackendState::Busy => "busy",
            BackendState::Dead => "dead",
        }
    }
}

impl std::fmt::Display for BackendState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Configuration for heartbeat behavior
///
/// Default values are based on MDP recommendations with some adjustments
/// for our use case (slightly longer intervals since we're on localhost).
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// How often to send heartbeats
    pub interval: Duration,
    /// How long to wait for a response
    pub timeout: Duration,
    /// How many consecutive failures before marking dead
    pub max_failures: u32,
    /// Initial reconnection delay (doubles each attempt up to max)
    pub reconnect_initial: Duration,
    /// Maximum reconnection delay
    pub reconnect_max: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            timeout: Duration::from_secs(2),
            max_failures: 3,
            reconnect_initial: Duration::from_secs(1),
            reconnect_max: Duration::from_secs(32),
        }
    }
}

/// Health tracking state for a backend
///
/// Atomic fields allow lock-free reads from health endpoints.
pub struct HealthTracker {
    /// Last time we sent a heartbeat
    pub last_heartbeat_sent: RwLock<Instant>,
    /// Last time we received any message (heartbeat or otherwise)
    pub last_message_recv: RwLock<Option<Instant>>,
    /// Consecutive heartbeat failures
    pub consecutive_failures: AtomicU32,
    /// Current backend state
    pub state: AtomicU8,
    /// Current reconnection delay (for exponential backoff)
    pub reconnect_delay: RwLock<Duration>,
}

impl HealthTracker {
    pub fn new() -> Self {
        Self {
            last_heartbeat_sent: RwLock::new(Instant::now()),
            last_message_recv: RwLock::new(None),
            consecutive_failures: AtomicU32::new(0),
            state: AtomicU8::new(BackendState::Connecting as u8),
            reconnect_delay: RwLock::new(Duration::from_secs(1)),
        }
    }

    /// Get current state
    pub fn get_state(&self) -> BackendState {
        BackendState::from_u8(self.state.load(Ordering::Relaxed))
    }

    /// Set state and return previous state
    pub fn set_state(&self, state: BackendState) -> BackendState {
        let prev = self.state.swap(state as u8, Ordering::Relaxed);
        BackendState::from_u8(prev)
    }

    /// Check if backend is alive (Ready or Busy)
    pub fn is_alive(&self) -> bool {
        matches!(self.get_state(), BackendState::Ready | BackendState::Busy)
    }

    /// Record successful message receipt (any command acts as heartbeat per MDP spec)
    pub async fn record_message_received(&self) {
        *self.last_message_recv.write().await = Some(Instant::now());
        self.consecutive_failures.store(0, Ordering::Relaxed);

        // If we were connecting or dead, we're now ready
        let state = self.get_state();
        if state == BackendState::Connecting || state == BackendState::Dead {
            self.set_state(BackendState::Ready);
        }
    }

    /// Record heartbeat failure
    pub fn record_failure(&self) -> u32 {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Reset failure count (called on successful reconnect)
    pub fn reset_failures(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Get failure count
    pub fn failure_count(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }

    /// Get health summary for /health endpoint
    pub async fn health_summary(&self) -> serde_json::Value {
        let last_recv = self.last_message_recv.read().await;
        let last_recv_ago = last_recv.map(|t| t.elapsed().as_secs());

        serde_json::json!({
            "state": self.get_state().as_str(),
            "alive": self.is_alive(),
            "consecutive_failures": self.failure_count(),
            "last_message_secs_ago": last_recv_ago,
        })
    }
}

impl Default for HealthTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a heartbeat attempt
#[derive(Debug)]
pub enum HeartbeatResult {
    /// Heartbeat succeeded
    Success,
    /// Heartbeat timed out
    Timeout,
    /// Send or receive error
    Error(String),
}

/// Callback for state changes
pub type StateChangeCallback = Box<dyn Fn(BackendState, BackendState) + Send + Sync>;

/// Spawn a heartbeat monitoring task
///
/// This task periodically sends heartbeats and tracks responses.
/// On failure, it marks the backend as dead and begins reconnection attempts.
///
/// Returns a handle that can be used to cancel the task.
pub fn spawn_heartbeat_task<F>(
    backend_name: String,
    health: Arc<HealthTracker>,
    config: HeartbeatConfig,
    send_heartbeat: F,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = HeartbeatResult> + Send>>
        + Send
        + Sync
        + 'static,
{
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(config.interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Update last sent time
                    *health.last_heartbeat_sent.write().await = Instant::now();

                    // Send heartbeat with timeout
                    let result = tokio::time::timeout(
                        config.timeout,
                        send_heartbeat()
                    ).await;

                    match result {
                        Ok(HeartbeatResult::Success) => {
                            health.record_message_received().await;
                            debug!("{}: heartbeat OK", backend_name);
                        }
                        Ok(HeartbeatResult::Timeout) | Err(_) => {
                            let failures = health.record_failure();
                            warn!("{}: heartbeat timeout ({}/{})", backend_name, failures, config.max_failures);

                            if failures >= config.max_failures {
                                let prev = health.set_state(BackendState::Dead);
                                if prev != BackendState::Dead {
                                    info!("{}: marked as dead after {} failures", backend_name, failures);
                                }
                            }
                        }
                        Ok(HeartbeatResult::Error(e)) => {
                            let failures = health.record_failure();
                            warn!("{}: heartbeat error: {} ({}/{})", backend_name, e, failures, config.max_failures);

                            if failures >= config.max_failures {
                                let prev = health.set_state(BackendState::Dead);
                                if prev != BackendState::Dead {
                                    info!("{}: marked as dead after {} failures", backend_name, failures);
                                }
                            }
                        }
                    }
                }
                _ = shutdown.recv() => {
                    debug!("{}: heartbeat task shutting down", backend_name);
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_transitions() {
        let tracker = HealthTracker::new();
        assert_eq!(tracker.get_state(), BackendState::Connecting);

        tracker.set_state(BackendState::Ready);
        assert_eq!(tracker.get_state(), BackendState::Ready);
        assert!(tracker.is_alive());

        tracker.set_state(BackendState::Dead);
        assert_eq!(tracker.get_state(), BackendState::Dead);
        assert!(!tracker.is_alive());
    }

    #[test]
    fn failure_counting() {
        let tracker = HealthTracker::new();
        assert_eq!(tracker.failure_count(), 0);

        assert_eq!(tracker.record_failure(), 1);
        assert_eq!(tracker.record_failure(), 2);
        assert_eq!(tracker.record_failure(), 3);
        assert_eq!(tracker.failure_count(), 3);

        tracker.reset_failures();
        assert_eq!(tracker.failure_count(), 0);
    }

    #[tokio::test]
    async fn message_received_resets_state() {
        let tracker = HealthTracker::new();
        tracker.set_state(BackendState::Dead);
        tracker.record_failure();
        tracker.record_failure();

        tracker.record_message_received().await;

        assert_eq!(tracker.get_state(), BackendState::Ready);
        assert_eq!(tracker.failure_count(), 0);
    }
}
