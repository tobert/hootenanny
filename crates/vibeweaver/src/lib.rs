//! Vibeweaver - Python-embedded Rust process for AI music agents
//!
//! Provides a natural programming interface for AI agents making music,
//! with persistent session state and reactive scheduling.

pub mod api;
pub mod async_bridge;
pub mod broadcast;
pub mod db;
pub mod kernel;
pub mod scheduler;
pub mod session;
pub mod state;
pub mod zmq_client;
pub mod zmq_server;

pub use db::Database;
pub use kernel::Kernel;
pub use session::{Session, SessionId};
pub use state::KernelState;
pub use zmq_server::{Server, ServerConfig};
