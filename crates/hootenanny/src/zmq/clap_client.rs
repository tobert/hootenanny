//! ClapClient - ZMQ DEALER client for CLAP audio analysis service
//!
//! CLAP provides audio embeddings, zero-shot classification, similarity,
//! and genre/mood analysis. It runs as a Python service via hootpy,
//! connected over ZMQ.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for CLAP client (uses shared HootClient)
pub type ClapClient = HootClient;

/// Create a CLAP client config with appropriate timeout
pub fn clap_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    // CLAP operations are relatively fast (~10-30s)
    ClientConfig::new("clap", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for CLAP operations (1 minute)
pub const DEFAULT_CLAP_TIMEOUT_MS: u64 = 60_000;
