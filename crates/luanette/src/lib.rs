//! Luanette - Lua Scripting ZMQ Server
//!
//! A programmable control plane for the Hootenanny system using Lua scripts.
//!
//! # Overview
//!
//! Luanette allows AI agents to compose, transform, and automate tools using
//! Lua scripts. Scripts are CAS-addressed artifacts, enabling the creation of
//! high-level workflows without recompilation.
//!
//! # Features
//!
//! - **Sandboxed Lua Runtime**: Safe script execution with timeout and restricted globals
//! - **ZMQ Server**: Accepts connections from Holler, Chaosgarden, and holler CLI
//! - **Job System**: Async script execution with polling, cancellation
//! - **OpenTelemetry**: Full tracing, logging, and metrics integration
//! - **MIDI Processing**: Built-in MIDI manipulation via `midi.*` namespace
//! - **AI-Friendly Errors**: Enhanced error messages with suggestions and hints

pub mod clients;
pub mod dispatch;
pub mod error;
pub mod handler;
pub mod job_system;
pub mod otel_bridge;
pub mod runtime;
pub mod schema;
pub mod stdlib;
pub mod telemetry;
pub mod tool_bridge;
pub mod zmq_server;

// Re-export key types for library users
pub use dispatch::Dispatcher;
pub use handler::LuanetteHandler;
pub use hooteproto::{JobId, JobInfo, JobStatus};
pub use job_system::JobStore;
pub use zmq_server::{Server as ZmqServer, ServerConfig as ZmqServerConfig};
