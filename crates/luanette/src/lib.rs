//! Luanette - Lua Scripting MCP Server
//!
//! A programmable control plane for the MCP ecosystem using Lua scripts.
//!
//! # Overview
//!
//! Luanette allows AI agents to compose, transform, and automate tools from
//! multiple upstream MCP servers using Lua scripts. Scripts are CAS-addressed
//! artifacts, enabling the creation of high-level "Meta-Tools" without recompilation.
//!
//! # Features
//!
//! - **Sandboxed Lua Runtime**: Safe script execution with timeout and restricted globals
//! - **MCP Tool Bridge**: Call upstream MCP tools via `mcp.<namespace>.<tool>` syntax
//! - **Job System**: Async script execution with polling, cancellation
//! - **OpenTelemetry**: Full tracing, logging, and metrics integration
//! - **MIDI Processing**: Built-in MIDI manipulation via `midi.*` namespace
//! - **AI-Friendly Errors**: Enhanced error messages with suggestions and hints

pub mod clients;
pub mod error;
pub mod handler;
pub mod job_system;
pub mod otel_bridge;
pub mod runtime;
pub mod schema;
pub mod stdlib;
pub mod telemetry;
pub mod tool_bridge;

// Re-export key types for library users
pub use handler::LuanetteHandler;
pub use job_system::{JobId, JobInfo, JobStatus, JobStore};
