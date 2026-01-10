//! holler - MCP gateway library for the Hootenanny system
//!
//! This library provides:
//! - `backend`: ZMQ backend connection using hooteproto::HootClient
//! - `dispatch`: JSON → typed Payload conversion (JSON boundary)
//! - `handler`: MCP handler implementation
//! - `serve`: MCP gateway server (HTTP transport)
//! - `stdio`: MCP stdio transport for Claude Code
//! - `client`: ZMQ client utilities
//! - `subscriber`: ZMQ subscriber for broadcasts

pub mod backend;
pub mod client;
pub mod commands;
pub mod dispatch;
pub mod handler;
pub mod help;
pub mod manual_schemas;
pub mod serve;
pub mod stdio;
pub mod subscriber;
pub mod telemetry;
pub mod tools_registry;
