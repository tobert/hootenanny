//! MidiRoleClient - ZMQ DEALER client for MIDI voice role classification service
//!
//! Classifies separated MIDI voices into musical roles (melody, bass, etc.)
//! using a scikit-learn model. Runs as a Python service via hootpy.
//!
//! Uses hooteproto::HootClient for connection management.

pub use hooteproto::{ClientConfig, HootClient};

/// Type alias for MIDI role classification client (uses shared HootClient)
pub type MidiRoleClient = HootClient;

/// Create a MIDI role classifier client config with appropriate timeout
pub fn midi_role_config(endpoint: &str, timeout_ms: u64) -> ClientConfig {
    ClientConfig::new("midi-role", endpoint).with_timeout(timeout_ms)
}

/// Default timeout for MIDI role classification (30 seconds)
pub const DEFAULT_MIDI_ROLE_TIMEOUT_MS: u64 = 30_000;
