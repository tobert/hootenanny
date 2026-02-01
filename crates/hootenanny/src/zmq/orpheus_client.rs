//! OrpheusClient - ZMQ DEALER client for Orpheus MIDI generation service
//!
//! Orpheus is a neural network for MIDI generation that can generate,
//! continue, bridge, and classify MIDI sequences. It runs as a Python
//! service via hootpy, connected over ZMQ.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for Orpheus client (uses shared HootClient)
pub type OrpheusClient = HootClient;

/// Create an Orpheus client config with appropriate timeout for GPU inference
pub fn orpheus_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    // Orpheus operations can take 30-120s depending on token count
    ClientConfig::new("orpheus", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for Orpheus operations (2 minutes for long sequences)
pub const DEFAULT_ORPHEUS_TIMEOUT_MS: u64 = 120_000;
