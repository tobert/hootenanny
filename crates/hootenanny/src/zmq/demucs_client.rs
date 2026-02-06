//! DemucsClient - ZMQ DEALER client for Demucs audio separation service
//!
//! Demucs separates audio into stems (drums, bass, vocals, other).
//! Runs as a Python service via hootpy.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for Demucs client (uses shared HootClient)
pub type DemucsClient = HootClient;

/// Create a Demucs client config with appropriate timeout
pub fn demucs_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    // Demucs separation takes 20-120s depending on audio length
    ClientConfig::new("demucs", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for Demucs operations (3 minutes for long tracks)
pub const DEFAULT_DEMUCS_TIMEOUT_MS: u64 = 180_000;
