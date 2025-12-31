//! GardenListener - Server-side ZMQ socket infrastructure
//!
//! Binds and configures the 4-socket garden protocol for services like chaosgarden.
//! Uses centralized socket configuration from `socket_config` module so fixing
//! socket issues here fixes them for all services.
//!
//! ## Usage
//!
//! ```ignore
//! use hooteconf::HootConfig;
//! use hooteproto::{GardenListener, GardenSockets};
//!
//! let config = HootConfig::load()?;
//! let listener = GardenListener::from_config(&config)?;
//! let sockets = listener.bind()?;
//!
//! // Now handle messages on the sockets...
//! // Use sockets.control.tx/rx, sockets.shell.tx/rx, etc.
//! ```
//!
//! ## Architecture
//!
//! This module provides socket infrastructure only. Message parsing and dispatch
//! remain in the implementing service (chaosgarden). This keeps hooteproto focused
//! on the wire protocol while allowing services flexibility in their handlers.
//!
//! The key benefit: socket configuration (LINGER, RECONNECT_IVL_MAX, etc.) is
//! centralized. Fixing issues in `socket_config.rs` fixes them everywhere.

use std::pin::Pin;

use anyhow::Result;
use futures::stream::{Stream, StreamExt};
use futures::Sink;
use tokio::sync::Mutex;
use tracing::info;

use crate::garden::GardenEndpoints;
use crate::socket_config::{
    create_publisher_and_bind, create_router_and_bind, Multipart, ZmqContext,
};

/// Boxed sink type for sending messages
type BoxedSink = Pin<Box<dyn Sink<Multipart, Error = tmq::TmqError> + Send>>;

/// Boxed stream type for receiving messages
type BoxedStream = Pin<Box<dyn Stream<Item = Result<Multipart, tmq::TmqError>> + Send>>;

/// Split ROUTER socket (tx + rx halves)
pub struct SplitRouter {
    pub tx: Mutex<BoxedSink>,
    pub rx: Mutex<BoxedStream>,
}

/// Split PUB socket (tx only)
pub struct SplitPublisher {
    pub tx: Mutex<BoxedSink>,
}

/// Helper to create split router
fn split_router<S>(socket: S) -> SplitRouter
where
    S: Stream<Item = Result<Multipart, tmq::TmqError>>
        + Sink<Multipart, Error = tmq::TmqError>
        + Unpin
        + Send
        + 'static,
{
    let (tx, rx) = socket.split();
    SplitRouter {
        tx: Mutex::new(Box::pin(tx)),
        rx: Mutex::new(Box::pin(rx)),
    }
}

/// Helper to create split publisher
fn split_publisher<S>(socket: S) -> SplitPublisher
where
    S: Sink<Multipart, Error = tmq::TmqError> + Unpin + Send + 'static,
{
    SplitPublisher {
        tx: Mutex::new(Box::pin(socket)),
    }
}

/// Bound socket set for garden protocol
///
/// The 4-socket Jupyter-inspired protocol:
/// - `control`: ROUTER - Priority commands (shutdown, interrupt)
/// - `shell`: ROUTER - Normal commands (transport, streams)
/// - `iopub`: PUB - Event broadcasts (state changes, metrics)
/// - `heartbeat`: ROUTER - Liveness detection (ROUTER for DEALER clients)
pub struct GardenSockets {
    /// Priority command socket (ROUTER) - split into tx/rx
    pub control: SplitRouter,
    /// Normal command socket (ROUTER) - split into tx/rx
    pub shell: SplitRouter,
    /// Event broadcast socket (PUB) - tx only
    pub iopub: SplitPublisher,
    /// Liveness detection socket (ROUTER) - split into tx/rx
    pub heartbeat: SplitRouter,
}

impl GardenSockets {
    /// Get endpoints from environment (useful for logging)
    pub fn endpoints(&self) -> &'static str {
        "garden sockets (use listener.endpoints() for details)"
    }
}

/// Server-side garden protocol listener
///
/// Creates and binds ZMQ sockets with proper configuration for the
/// Jupyter-inspired 4-socket protocol.
pub struct GardenListener {
    endpoints: GardenEndpoints,
}

impl GardenListener {
    /// Create a listener from validated endpoints
    pub fn new(endpoints: GardenEndpoints) -> Self {
        Self { endpoints }
    }

    /// Create from HootConfig (validates socket_dir is present)
    ///
    /// This is the recommended way to create a listener. It validates
    /// that the configuration contains a socket_dir path.
    pub fn from_config(config: &hooteconf::HootConfig) -> Result<Self> {
        let endpoints = GardenEndpoints::from_config(config)?;
        Ok(Self::new(endpoints))
    }

    /// Bind all sockets and return the bound socket set
    ///
    /// Sockets are configured with:
    /// - LINGER = 0 (don't block on close)
    /// - RECONNECT_IVL = 1s (responsive reconnection)
    /// - RECONNECT_IVL_MAX = 60s (cap exponential backoff)
    pub fn bind(&self) -> Result<GardenSockets> {
        let context = ZmqContext::new();

        let control = create_router_and_bind(&context, &self.endpoints.control, "control")?;
        info!("control socket bound to {}", self.endpoints.control);

        let shell = create_router_and_bind(&context, &self.endpoints.shell, "shell")?;
        info!("shell socket bound to {}", self.endpoints.shell);

        let iopub = create_publisher_and_bind(&context, &self.endpoints.iopub, "iopub")?;
        info!("iopub socket bound to {}", self.endpoints.iopub);

        let heartbeat = create_router_and_bind(&context, &self.endpoints.heartbeat, "heartbeat")?;
        info!("heartbeat socket bound to {}", self.endpoints.heartbeat);

        info!("garden listener ready (4 sockets bound)");

        Ok(GardenSockets {
            control: split_router(control),
            shell: split_router(shell),
            iopub: split_publisher(iopub),
            heartbeat: split_router(heartbeat),
        })
    }

    /// Get the endpoints this listener will bind to
    pub fn endpoints(&self) -> &GardenEndpoints {
        &self.endpoints
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listener_new() {
        let endpoints = GardenEndpoints::inproc("test");
        let listener = GardenListener::new(endpoints);
        assert!(listener.endpoints().control.contains("test"));
    }
}
