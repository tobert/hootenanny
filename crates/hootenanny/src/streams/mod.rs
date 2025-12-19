//! Stream capture - manage streaming input to CAS-backed chunks.
#![allow(dead_code)]
//!
//! This module handles the hootenanny side of stream capture:
//! - Stream lifecycle (create, start, stop)
//! - Chunk management (staging, sealing)
//! - Manifest persistence
//! - Slicing operations

pub mod manager;
pub mod manifest;
pub mod slicing;
pub mod types;

pub use manager::StreamManager;
pub use slicing::SlicingEngine;
pub use types::StreamUri;
