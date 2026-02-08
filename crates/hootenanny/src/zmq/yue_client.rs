//! YueClient - ZMQ DEALER client for YuE text-to-song service
//!
//! YuE generates full songs from lyrics + genre tags using a dual-stage
//! model (7B semantic + 1B acoustic). Generation is slow (several minutes).
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for YuE client (uses shared HootClient)
pub type YueClient = HootClient;

/// Create a YuE client config with generous timeout for slow generation
pub fn yue_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    ClientConfig::new("yue", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for YuE operations (10 minutes — generation is slow)
pub const DEFAULT_YUE_TIMEOUT_MS: u64 = 600_000;
