//! RaveClient - ZMQ DEALER client for RAVE audio codec service
//!
//! RAVE (Realtime Audio Variational autoEncoder) is a neural audio codec
//! that can encode audio to a latent representation and decode it back.
//! It runs as a Python service via hootpy, connected over ZMQ.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for RAVE client (uses shared HootClient)
pub type RaveClient = HootClient;

/// Create a RAVE client config with appropriate timeout for GPU inference
pub fn rave_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    // RAVE operations can take 30-60s for long audio files
    ClientConfig::new("rave", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for RAVE operations (2 minutes for long audio)
pub const DEFAULT_RAVE_TIMEOUT_MS: u64 = 120_000;
