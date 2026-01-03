//! Hootenanny - HalfRemembered MCP Server
//!
//! Library exposing core modules for testing and reuse.

#![allow(unused, clippy::unnecessary_cast, clippy::too_many_arguments)]

pub mod api;
pub mod artifact_store;
pub mod cas;
pub mod event_buffer;
pub mod gpu_monitor;
pub mod job_system;
pub mod mcp_tools;
pub mod persistence;
pub mod pipewire;
pub mod sessions;
pub mod streams;
pub mod telemetry;
pub mod types;
pub mod web;
pub mod zmq;
