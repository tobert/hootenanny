//! Centralized ZMQ socket configuration for hootenanny peers
//!
//! All socket setup goes through these helpers to ensure consistent configuration
//! across all services. Fixing bugs here fixes them everywhere.
//!
//! ## Workarounds Applied
//!
//! - `RECONNECT_IVL_MAX` capped at 60s to prevent runaway backoff (rzmq issue)
//! - `LINGER` set to 0 for clean shutdown
//! - `RECONNECT_IVL` set to 1s for responsive reconnection
//!
//! See: docs/issues/rzmq-req-idle-timeout.md, docs/issues/rzmq-backoff-cap.md

use anyhow::{Context, Result};
use rzmq::socket::options::{
    HEARTBEAT_IVL, HEARTBEAT_TIMEOUT, LINGER, RECONNECT_IVL, RECONNECT_IVL_MAX, ROUTER_MANDATORY,
    ROUTING_ID, SUBSCRIBE,
};
use rzmq::Socket;
use tracing::warn;

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

/// Configure a socket with standard hootenanny options.
///
/// Applies:
/// - LINGER = 0 (don't block on close)
/// - RECONNECT_IVL = 1s (responsive reconnection)
/// - RECONNECT_IVL_MAX = 60s (cap exponential backoff)
/// - HEARTBEAT_IVL = 30s (ZMTP keepalive pings)
/// - HEARTBEAT_TIMEOUT = 90s (connection dead if no pong)
pub async fn configure_socket(socket: &Socket, name: &str) -> Result<()> {
    // Don't block on close - let pending messages drop
    if let Err(e) = socket.set_option_raw(LINGER, &0i32.to_ne_bytes()).await {
        warn!("{}: Failed to set LINGER: {}", name, e);
    }

    // Start with 1 second reconnect interval
    if let Err(e) = socket
        .set_option_raw(RECONNECT_IVL, &DEFAULT_RECONNECT_IVL_MS.to_ne_bytes())
        .await
    {
        warn!("{}: Failed to set RECONNECT_IVL: {}", name, e);
    }

    // Cap reconnect backoff at 60 seconds
    if let Err(e) = socket
        .set_option_raw(RECONNECT_IVL_MAX, &DEFAULT_RECONNECT_IVL_MAX_MS.to_ne_bytes())
        .await
    {
        warn!("{}: Failed to set RECONNECT_IVL_MAX: {}", name, e);
    }

    // ZMTP heartbeats keep connections alive
    // Without these, idle connections die after rzmq's 300s RCVTIMEO default
    if let Err(e) = socket
        .set_option_raw(HEARTBEAT_IVL, &DEFAULT_HEARTBEAT_IVL_MS.to_ne_bytes())
        .await
    {
        warn!("{}: Failed to set HEARTBEAT_IVL: {}", name, e);
    }

    if let Err(e) = socket
        .set_option_raw(HEARTBEAT_TIMEOUT, &DEFAULT_HEARTBEAT_TIMEOUT_MS.to_ne_bytes())
        .await
    {
        warn!("{}: Failed to set HEARTBEAT_TIMEOUT: {}", name, e);
    }

    Ok(())
}

/// Configure a DEALER socket with routing identity.
pub async fn configure_dealer(socket: &Socket, name: &str, identity: &[u8]) -> Result<()> {
    configure_socket(socket, name).await?;

    if let Err(e) = socket.set_option_raw(ROUTING_ID, identity).await {
        warn!("{}: Failed to set ROUTING_ID: {}", name, e);
    }

    Ok(())
}

/// Configure a ROUTER socket.
///
/// ROUTER sockets don't need special options beyond the base configuration.
/// This function exists for API consistency.
pub async fn configure_router(socket: &Socket, name: &str) -> Result<()> {
    configure_socket(socket, name).await
}

/// Configure a SUB socket to receive all messages.
pub async fn configure_subscriber(socket: &Socket, name: &str) -> Result<()> {
    configure_socket(socket, name).await?;

    // Subscribe to all messages (empty prefix)
    if let Err(e) = socket.set_option_raw(SUBSCRIBE, b"").await {
        warn!("{}: Failed to subscribe to all: {}", name, e);
    }

    Ok(())
}

/// Create and configure a SUB socket, then connect to an endpoint.
///
/// Sets SUBSCRIBE to empty prefix (receive all messages).
pub async fn create_subscriber_and_connect(
    ctx: &rzmq::Context,
    endpoint: &str,
    name: &str,
) -> Result<Socket> {
    let socket = ctx
        .socket(rzmq::SocketType::Sub)
        .with_context(|| format!("Failed to create {} socket", name))?;

    configure_subscriber(&socket, name).await?;

    socket
        .connect(endpoint)
        .await
        .with_context(|| format!("Failed to connect {} socket to {}", name, endpoint))?;

    Ok(socket)
}

/// Create and configure a socket, then connect to an endpoint.
pub async fn create_and_connect(
    ctx: &rzmq::Context,
    socket_type: rzmq::SocketType,
    endpoint: &str,
    name: &str,
) -> Result<Socket> {
    let socket = ctx
        .socket(socket_type)
        .with_context(|| format!("Failed to create {} socket", name))?;

    configure_socket(&socket, name).await?;

    socket
        .connect(endpoint)
        .await
        .with_context(|| format!("Failed to connect {} socket to {}", name, endpoint))?;

    Ok(socket)
}

/// Create and configure a socket, then bind to an endpoint.
pub async fn create_and_bind(
    ctx: &rzmq::Context,
    socket_type: rzmq::SocketType,
    endpoint: &str,
    name: &str,
) -> Result<Socket> {
    let socket = ctx
        .socket(socket_type)
        .with_context(|| format!("Failed to create {} socket", name))?;

    configure_socket(&socket, name).await?;

    socket
        .bind(endpoint)
        .await
        .with_context(|| format!("Failed to bind {} socket to {}", name, endpoint))?;

    Ok(socket)
}

/// Create and configure a ROUTER socket with ROUTER_MANDATORY, then bind.
///
/// ROUTER_MANDATORY ensures proper error reporting instead of silently dropping
/// messages to unknown identities.
pub async fn create_router_and_bind(
    ctx: &rzmq::Context,
    endpoint: &str,
    name: &str,
) -> Result<Socket> {
    let socket = ctx
        .socket(rzmq::SocketType::Router)
        .with_context(|| format!("Failed to create {} ROUTER socket", name))?;

    configure_router(&socket, name).await?;

    // Enable ROUTER_MANDATORY for proper error reporting
    if let Err(e) = socket
        .set_option_raw(ROUTER_MANDATORY, &1i32.to_ne_bytes())
        .await
    {
        warn!("{}: Failed to set ROUTER_MANDATORY: {}", name, e);
    }

    socket
        .bind(endpoint)
        .await
        .with_context(|| format!("Failed to bind {} socket to {}", name, endpoint))?;

    Ok(socket)
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
