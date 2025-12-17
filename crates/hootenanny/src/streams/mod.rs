//! Stream capture - manage streaming input to CAS-backed chunks.
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
pub use manifest::{ChunkReference, StreamManifest};
pub use slicing::{SliceOutput, SliceRequest, SliceResult, SlicingEngine, TimeSpec};
pub use types::{
    AudioFormat, SampleFormat, StreamDefinition, StreamFormat, StreamStatus, StreamUri,
};
