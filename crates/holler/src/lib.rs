//! holler - MCP gateway library for the Hootenanny system
//!
//! This library provides:
//! - `backend`: ZMQ backend connection using hooteproto::HootClient
//! - `dispatch`: JSON → typed Payload conversion (JSON boundary)
//! - `handler`: MCP handler implementation
//! - `serve`: MCP gateway server
//! - `client`: ZMQ client utilities
//! - `subscriber`: ZMQ subscriber for broadcasts

pub mod backend;
pub mod client;
pub mod commands;
pub mod dispatch;
pub mod handler;
pub mod serve;
pub mod subscriber;
pub mod telemetry;
