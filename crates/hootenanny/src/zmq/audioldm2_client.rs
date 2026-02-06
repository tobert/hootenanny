//! Audioldm2Client - ZMQ DEALER client for AudioLDM2 text-to-audio service
//!
//! AudioLDM2 generates audio (sounds, music, speech) from text prompts
//! using a diffusion pipeline. Runs as a Python service via hootpy.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for AudioLDM2 client (uses shared HootClient)
pub type Audioldm2Client = HootClient;

/// Create an AudioLDM2 client config with appropriate timeout for diffusion
pub fn audioldm2_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    // AudioLDM2 diffusion takes 30-180s depending on steps and duration
    ClientConfig::new("audioldm2", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for AudioLDM2 operations (3 minutes for high step counts)
pub const DEFAULT_AUDIOLDM2_TIMEOUT_MS: u64 = 180_000;
