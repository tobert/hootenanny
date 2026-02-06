//! AnticipatoryClient - ZMQ DEALER client for Anticipatory Music Transformer
//!
//! Anticipatory generates, continues, and embeds polyphonic MIDI sequences.
//! Runs as a Python service via hootpy.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for Anticipatory client (uses shared HootClient)
pub type AnticipatoryClient = HootClient;

/// Create an Anticipatory client config with appropriate timeout
pub fn anticipatory_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    // Anticipatory generation takes 10-60s depending on length
    ClientConfig::new("anticipatory", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for Anticipatory operations (2 minutes)
pub const DEFAULT_ANTICIPATORY_TIMEOUT_MS: u64 = 120_000;
