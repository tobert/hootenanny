//! Capture sessions - group multiple streams with timing and segments.
#![allow(dead_code)]
//!
//! Sessions provide the organizational layer above individual streams, enabling:
//! - Multi-stream coordination (audio + MIDI together)
//! - Segmented recording (start/stop/resume without creating new sessions)
//! - Multi-clock capture (correlate different timing sources)
//! - Session-level artifacts with lineage
//!
//! ## Architecture
//!
//! - **SessionManager**: Coordinates session lifecycle, delegates to StreamManager
//! - **CaptureSession**: Logical container for multiple streams
//! - **SessionSegment**: Contiguous recording period within a session
//! - **SessionTimeline**: Multi-clock snapshots for correlation
//!
//! ## Session Modes
//!
//! - **Passive**: Continuous capture, slice retrospectively
//! - **RequestResponse**: Send MIDI, capture audio response
//!
//! ## Lifecycle
//!
//! ```text
//! create_session()
//!      ↓
//! play()  → start segment, begin recording
//!      ↓
//! pause() → end segment (can call play() again for new segment)
//!      ↓
//! stop()  → finalize session, create artifact
//! ```

pub mod manager;
pub mod types;

pub use manager::SessionManager;
