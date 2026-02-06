//! MusicgenClient - ZMQ DEALER client for MusicGen text-to-music service
//!
//! MusicGen generates audio from text prompts using Meta's model.
//! It runs as a Python service via hootpy, connected over ZMQ.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for MusicGen client (uses shared HootClient)
pub type MusicgenClient = HootClient;

/// Create a MusicGen client config with appropriate timeout for GPU inference
pub fn musicgen_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    // MusicGen generation takes 20-120s depending on duration
    ClientConfig::new("musicgen", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for MusicGen operations (2 minutes for longer generations)
pub const DEFAULT_MUSICGEN_TIMEOUT_MS: u64 = 120_000;
