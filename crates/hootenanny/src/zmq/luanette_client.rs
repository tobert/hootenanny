//! LuanetteClient - ZMQ DEALER client for luanette backend
//!
//! Uses hooteproto::HootClient for connection management.
//! This module provides a type alias for backward compatibility.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for luanette client (uses shared HootClient)
pub type LuanetteClient = HootClient;

/// Create a luanette client config
pub fn luanette_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    ClientConfig::new("luanette", endpoint).with_timeout(timeout_ms)
}
