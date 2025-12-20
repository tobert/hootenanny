//! VibeweaverClient - ZMQ DEALER client for vibeweaver backend
//!
//! Uses hooteproto::HootClient for connection management.
//! This module provides a type alias for backward compatibility.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for vibeweaver client (uses shared HootClient)
pub type VibeweaverClient = HootClient;

/// Create a vibeweaver client config
pub fn vibeweaver_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    ClientConfig::new("vibeweaver", endpoint).with_timeout(timeout_ms)
}
