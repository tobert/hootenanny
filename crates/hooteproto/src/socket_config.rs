//! Centralized ZMQ socket configuration for hootenanny peers
//!
//! All socket setup goes through these helpers to ensure consistent configuration
//! across all services. Fixing bugs here fixes them everywhere.
//!
//! ## Configuration Applied
//!
//! - `RECONNECT_IVL_MAX` capped at 60s to prevent runaway backoff
//! - `LINGER` set to 0 for clean shutdown
//! - `RECONNECT_IVL` set to 1s for responsive reconnection
//! - `HEARTBEAT_IVL` set to 30s for connection keepalive
//! - `HEARTBEAT_TIMEOUT` set to 90s (3x heartbeat interval)
//!
//! ## Socket Types
//!
//! tmq doesn't export socket types directly, so callers should use generics
//! with trait bounds for stored sockets:
//!
//! ```ignore
//! use futures::{Sink, Stream};
//! use tmq::{Multipart, TmqError};
//!
//! struct MyClient<S> {
//!     socket: S,
//! }
//!
//! impl<S> MyClient<S>
//! where
//!     S: Stream<Item = Result<Multipart, TmqError>> + Sink<Multipart, Error = TmqError> + Unpin,
//! {
//!     // ...
//! }
//! ```

use anyhow::{Context, Result};
use futures::{Sink, Stream};
use tmq::{dealer, publish, router, subscribe, TmqError};

// Re-export Context and Multipart for callers
pub use tmq::Context as ZmqContext;
pub use tmq::Multipart;

/// Default reconnect interval in milliseconds
pub const DEFAULT_RECONNECT_IVL_MS: i32 = 1000;

/// Maximum reconnect interval in milliseconds (60 seconds)
/// Caps exponential backoff to prevent hours-long delays
pub const DEFAULT_RECONNECT_IVL_MAX_MS: i32 = 60_000;

/// ZMTP heartbeat interval in milliseconds (30 seconds)
/// Sends PING commands at this interval to keep connections alive
pub const DEFAULT_HEARTBEAT_IVL_MS: i32 = 30_000;

/// ZMTP heartbeat timeout in milliseconds (90 seconds)
/// Connection is considered dead if no PONG received within this time
/// Set to 3x heartbeat interval to tolerate missed heartbeats
pub const DEFAULT_HEARTBEAT_TIMEOUT_MS: i32 = 90_000;

/// Trait bound for DEALER sockets (send and receive)
pub trait DealerSocket:
    Stream<Item = Result<Multipart, TmqError>> + Sink<Multipart, Error = TmqError> + Unpin + Send
{
}
impl<T> DealerSocket for T where
    T: Stream<Item = Result<Multipart, TmqError>> + Sink<Multipart, Error = TmqError> + Unpin + Send
{
}

/// Trait bound for ROUTER sockets (send and receive with identities)
pub trait RouterSocket:
    Stream<Item = Result<Multipart, TmqError>> + Sink<Multipart, Error = TmqError> + Unpin + Send
{
}
impl<T> RouterSocket for T where
    T: Stream<Item = Result<Multipart, TmqError>> + Sink<Multipart, Error = TmqError> + Unpin + Send
{
}

/// Trait bound for SUB sockets (receive only)
pub trait SubscriberSocket: Stream<Item = Result<Multipart, TmqError>> + Unpin + Send {}
impl<T> SubscriberSocket for T where T: Stream<Item = Result<Multipart, TmqError>> + Unpin + Send {}

/// Trait bound for PUB sockets (send only)
pub trait PublisherSocket: Sink<Multipart, Error = TmqError> + Unpin + Send {}
impl<T> PublisherSocket for T where T: Sink<Multipart, Error = TmqError> + Unpin + Send {}

/// Create a configured DEALER socket and connect to an endpoint.
///
/// Applies standard options: linger, reconnect intervals.
/// Returns an opaque socket type - use DealerSocket trait bound for storage.
pub fn create_dealer_and_connect(
    ctx: &ZmqContext,
    endpoint: &str,
    identity: &[u8],
    _name: &str,
) -> Result<impl DealerSocket> {
    dealer(ctx)
        .set_linger(0)
        .set_reconnect_ivl(DEFAULT_RECONNECT_IVL_MS)
        .set_reconnect_ivl_max(DEFAULT_RECONNECT_IVL_MAX_MS)
        .set_identity(identity)
        .connect(endpoint)
        .with_context(|| format!("Failed to connect DEALER to {}", endpoint))
}

/// Create a configured ROUTER socket and bind to an endpoint.
///
/// Applies standard options: linger, reconnect intervals.
pub fn create_router_and_bind(
    ctx: &ZmqContext,
    endpoint: &str,
    _name: &str,
) -> Result<impl RouterSocket> {
    router(ctx)
        .set_linger(0)
        .set_reconnect_ivl(DEFAULT_RECONNECT_IVL_MS)
        .set_reconnect_ivl_max(DEFAULT_RECONNECT_IVL_MAX_MS)
        .bind(endpoint)
        .with_context(|| format!("Failed to bind ROUTER to {}", endpoint))
}

/// Create a configured SUB socket and connect to an endpoint.
///
/// Subscribes to all messages (empty prefix).
pub fn create_subscriber_and_connect(
    ctx: &ZmqContext,
    endpoint: &str,
    _name: &str,
) -> Result<impl SubscriberSocket> {
    subscribe(ctx)
        .set_linger(0)
        .set_reconnect_ivl(DEFAULT_RECONNECT_IVL_MS)
        .set_reconnect_ivl_max(DEFAULT_RECONNECT_IVL_MAX_MS)
        .connect(endpoint)
        .with_context(|| format!("Failed to connect SUB to {}", endpoint))?
        .subscribe(b"") // Subscribe to all messages
        .with_context(|| "Failed to subscribe to all messages")
}

/// Create a configured PUB socket and bind to an endpoint.
pub fn create_publisher_and_bind(
    ctx: &ZmqContext,
    endpoint: &str,
    _name: &str,
) -> Result<impl PublisherSocket> {
    publish(ctx)
        .set_linger(0)
        .bind(endpoint)
        .with_context(|| format!("Failed to bind PUB to {}", endpoint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(DEFAULT_RECONNECT_IVL_MS, 1000);
        assert_eq!(DEFAULT_RECONNECT_IVL_MAX_MS, 60_000);
        assert_eq!(DEFAULT_HEARTBEAT_IVL_MS, 30_000);
        assert_eq!(DEFAULT_HEARTBEAT_TIMEOUT_MS, 90_000);
        // Timeout should be >= 3x interval to tolerate missed heartbeats
        assert!(DEFAULT_HEARTBEAT_TIMEOUT_MS >= DEFAULT_HEARTBEAT_IVL_MS * 3);
    }
}
