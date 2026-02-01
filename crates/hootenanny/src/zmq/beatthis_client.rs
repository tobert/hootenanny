//! BeatthisClient - ZMQ DEALER client for beat-this beat detection service
//!
//! Beat-this is a neural network for beat/downbeat detection in audio.
//! It runs as a Python service via hootpy, connected over ZMQ.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for beat-this client (uses shared HootClient)
pub type BeatthisClient = HootClient;

/// Create a beat-this client config with appropriate timeout
pub fn beatthis_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    // Beat-this operations are relatively fast (~10-30s)
    ClientConfig::new("beatthis", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for beat-this operations (1 minute)
pub const DEFAULT_BEATTHIS_TIMEOUT_MS: u64 = 60_000;
