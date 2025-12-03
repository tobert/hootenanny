//! MCP HTTP Transport
//!
//! Implements MCP HTTP transports:
//!
//! ## SSE Transport (legacy)
//! - GET /sse - Establish SSE connection, receive responses
//! - POST /message - Send JSON-RPC requests
//!
//! ## Streamable HTTP Transport (recommended)
//! - POST / - Send JSON-RPC request, receive response directly
//! - DELETE / - Terminate session
//! - Session ID via Mcp-Session-Id header

mod message;
mod sampling;
mod sse;
mod streamable;

pub use message::message_handler;
pub use sampling::{SamplingClient, SamplingError};
pub use sse::sse_handler;
pub use streamable::{streamable_handler, delete_handler};

use axum::Router;
use std::sync::Arc;

use crate::session::{InMemorySessionStore, SessionStore};

/// Shared state for MCP handlers.
pub struct McpState<H> {
    /// The application's tool/resource/prompt handler.
    pub handler: Arc<H>,

    /// Session store.
    pub sessions: Arc<dyn SessionStore>,

    /// Server name for protocol responses.
    pub server_name: String,

    /// Server version for protocol responses.
    pub server_version: String,

    /// Sampling client for server-initiated LLM requests.
    pub sampling_client: Arc<SamplingClient>,
}

impl<H> McpState<H> {
    /// Create new MCP state with the given handler.
    pub fn new(handler: H, server_name: impl Into<String>, server_version: impl Into<String>) -> Self {
        Self {
            handler: Arc::new(handler),
            sessions: Arc::new(InMemorySessionStore::new()),
            server_name: server_name.into(),
            server_version: server_version.into(),
            sampling_client: Arc::new(SamplingClient::new()),
        }
    }

    /// Create new MCP state with a custom session store.
    pub fn with_session_store(
        handler: H,
        sessions: Arc<dyn SessionStore>,
        server_name: impl Into<String>,
        server_version: impl Into<String>,
    ) -> Self {
        Self {
            handler: Arc::new(handler),
            sessions,
            server_name: server_name.into(),
            server_version: server_version.into(),
            sampling_client: Arc::new(SamplingClient::new()),
        }
    }
}

/// Build an axum Router for MCP SSE transport.
///
/// Routes:
/// - GET /sse - SSE connection endpoint
/// - POST /message - JSON-RPC message endpoint
pub fn router<H>(state: Arc<McpState<H>>) -> Router
where
    H: crate::Handler + 'static,
{
    Router::new()
        .route("/sse", axum::routing::get(sse_handler::<H>))
        .route("/message", axum::routing::post(message_handler::<H>))
        .with_state(state)
}

/// Build an axum Router for MCP Streamable HTTP transport.
///
/// Routes:
/// - POST / - JSON-RPC request/response
/// - DELETE / - Session termination
///
/// Session ID is passed via Mcp-Session-Id header.
pub fn streamable_router<H>(state: Arc<McpState<H>>) -> Router
where
    H: crate::Handler + 'static,
{
    Router::new()
        .route("/", axum::routing::post(streamable_handler::<H>))
        .route("/", axum::routing::delete(delete_handler::<H>))
        .with_state(state)
}

/// Build an axum Router supporting both transports.
///
/// Routes:
/// - POST / - Streamable HTTP (recommended)
/// - DELETE / - Session termination
/// - GET /sse - SSE transport (legacy)
/// - POST /message - SSE message endpoint (legacy)
pub fn dual_router<H>(state: Arc<McpState<H>>) -> Router
where
    H: crate::Handler + 'static,
{
    Router::new()
        // Streamable HTTP transport (primary)
        .route("/", axum::routing::post(streamable_handler::<H>))
        .route("/", axum::routing::delete(delete_handler::<H>))
        // SSE transport (legacy/fallback)
        .route("/sse", axum::routing::get(sse_handler::<H>))
        .route("/message", axum::routing::post(message_handler::<H>))
        .with_state(state)
}
