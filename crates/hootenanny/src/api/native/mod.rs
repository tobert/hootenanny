//! Native tools - unified model-agnostic interfaces.
//!
//! These tools provide a high-level abstraction over model-specific tools:
//! - `sample(space)` → unified generation across Orpheus/MusicGen/YuE
//! - `extend(encoding)` → continue content via appropriate model
//! - `analyze(tasks)` → run multiple analyses (beats, genre, classify)
//! - `bridge(from, to)` → create MIDI transitions
//! - `project(encoding, target)` → format conversion (MIDI→audio, ABC→MIDI)
//! - `schedule(encoding, at)` → schedule content on timeline
//!
//! All request types are re-exported from hooteproto for type unification.

pub mod analyze;
pub mod bridge;
pub mod extend;
pub mod project;
pub mod sample;
pub mod schedule;

// Re-export request types from hooteproto
pub use hooteproto::request::{
    AnalyzeRequest, BridgeRequest, ExtendRequest, ProjectRequest, SampleRequest, ScheduleRequest,
};
