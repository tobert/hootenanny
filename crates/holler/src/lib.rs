//! holler - MCP gateway library for the Hootenanny system
//!
//! This library provides:
//! - `backend`: ZMQ backend connection using hooteproto::HootClient
//! - `handler`: MCP handler implementation
//! - `serve`: MCP gateway server
//! - `client`: ZMQ client utilities
//! - `subscriber`: ZMQ subscriber for broadcasts

pub mod backend;
pub mod client;
pub mod commands;
pub mod handler;
pub mod serve;
pub mod subscriber;
pub mod telemetry;
