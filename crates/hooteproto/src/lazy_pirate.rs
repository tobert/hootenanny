//! Lazy Pirate pattern abstraction for reliable ZMQ request-reply.
//!
//! From zguide Chapter 4: The Lazy Pirate pattern handles unreliable servers by:
//! - Retrying requests on timeout
//! - Tracking peer health via successful responses
//! - Capping backoff to prevent hours-long retry delays
//!
//! This module provides a common abstraction that can be implemented by
//! different socket types (DEALER for HootClient, REQ for GardenClient).
//!
//! ## Workarounds for rzmq Issues
//!
//! This abstraction includes workarounds for two rzmq issues:
//!
//! 1. **REQ idle timeout**: rzmq's SessionConnectionActorX unconditionally reads
//!    in Operational phase, causing REQ sockets to timeout after 300s even when
//!    idle. Workaround: periodic keepalives.
//!    See: docs/issues/rzmq-req-idle-timeout.md
//!
//! 2. **Unbounded backoff**: Reconnection backoff can grow to 8192s (2.3 hours).
//!    Workaround: set RECONNECT_IVL_MAX explicitly.
//!    See: docs/issues/rzmq-backoff-cap.md

use std::time::Duration;

use async_trait::async_trait;

// Re-export ConnectionState from client module for consistency
pub use crate::client::ConnectionState;

/// Configuration for Lazy Pirate pattern.
///
/// These settings control retry behavior and health tracking across all
/// LazyPirateClient implementations.
#[derive(Debug, Clone)]
pub struct LazyPirateConfig {
    /// Timeout per request attempt
    pub timeout: Duration,
    /// Maximum retry attempts before failing a request
    pub max_retries: u32,
    /// Initial backoff between retries
    pub backoff_base: Duration,
    /// Maximum backoff between retries (caps exponential growth)
    pub backoff_max: Duration,
    /// Consecutive failures before marking peer as dead
    pub max_failures: u32,
    /// Interval for keepalive heartbeats (workaround for rzmq 300s idle timeout)
    pub keepalive_interval: Duration,
}

impl Default for LazyPirateConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_retries: 3,
            backoff_base: Duration::from_millis(100),
            backoff_max: Duration::from_secs(5),
            max_failures: 5,
            keepalive_interval: Duration::from_secs(60),
        }
    }
}

impl LazyPirateConfig {
    /// Create config with custom timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Create config with custom retry count
    pub fn with_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Create config with custom keepalive interval
    pub fn with_keepalive(mut self, interval: Duration) -> Self {
        self.keepalive_interval = interval;
        self
    }

    /// Calculate backoff delay for a given attempt number
    ///
    /// Uses exponential backoff capped at backoff_max:
    /// attempt 1: backoff_base
    /// attempt 2: backoff_base * 2
    /// attempt n: min(backoff_base * 2^(n-1), backoff_max)
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        let multiplier = 2u32.saturating_pow(attempt.saturating_sub(1));
        let delay = self.backoff_base.saturating_mul(multiplier);
        std::cmp::min(delay, self.backoff_max)
    }
}

/// Result of a single request attempt (before retry logic).
#[derive(Debug)]
pub enum AttemptResult<T> {
    /// Request succeeded with response
    Success(T),
    /// Request timed out (may retry)
    Timeout,
    /// Send failed (may retry)
    SendFailed(String),
    /// Peer is not responding (too many failures)
    PeerDead,
}

/// Trait for clients implementing the Lazy Pirate pattern.
///
/// This provides a common interface for reliable request-reply over ZMQ,
/// regardless of the underlying socket type (DEALER, REQ).
///
/// Implementors should:
/// - Handle socket reconnection via ZMQ (don't destroy sockets)
/// - Track health via successful responses, not connection state
/// - Cap reconnection backoff to prevent runaway delays
#[async_trait]
pub trait LazyPirateClient: Send + Sync {
    /// Request type sent to peer
    type Request: Send;
    /// Response type received from peer
    type Response: Send;

    /// Send a request with retry logic following Lazy Pirate pattern.
    ///
    /// This method handles:
    /// - Timeouts with configurable retries
    /// - Exponential backoff between attempts
    /// - Health tracking based on success/failure
    async fn request_with_retry(
        &self,
        request: Self::Request,
        config: &LazyPirateConfig,
    ) -> anyhow::Result<Self::Response>;

    /// Get current health state of the connection.
    ///
    /// This tracks whether the PEER is responding, not whether the socket
    /// is connected (ZMQ handles that automatically).
    fn health_state(&self) -> ConnectionState;

    /// Check if peer is currently responding
    fn is_connected(&self) -> bool {
        self.health_state() == ConnectionState::Connected
    }

    /// Check if peer is still alive (not marked dead)
    fn is_alive(&self) -> bool {
        self.health_state() != ConnectionState::Dead
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_calculation() {
        let config = LazyPirateConfig {
            backoff_base: Duration::from_millis(100),
            backoff_max: Duration::from_secs(5),
            ..Default::default()
        };

        // Attempt 0 = no delay
        assert_eq!(config.backoff_for_attempt(0), Duration::ZERO);

        // Attempt 1 = base (100ms)
        assert_eq!(config.backoff_for_attempt(1), Duration::from_millis(100));

        // Attempt 2 = base * 2 (200ms)
        assert_eq!(config.backoff_for_attempt(2), Duration::from_millis(200));

        // Attempt 3 = base * 4 (400ms)
        assert_eq!(config.backoff_for_attempt(3), Duration::from_millis(400));

        // Attempt 7 = base * 64 (6.4s) capped at 5s
        assert_eq!(config.backoff_for_attempt(7), Duration::from_secs(5));

        // Large attempt = still capped
        assert_eq!(config.backoff_for_attempt(100), Duration::from_secs(5));
    }

    #[test]
    fn default_config_values() {
        let config = LazyPirateConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.keepalive_interval, Duration::from_secs(60));
    }
}
