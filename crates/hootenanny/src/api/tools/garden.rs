//! Garden tool request types for schema generation

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Request to seek playback position
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenSeekRequest {
    /// Beat position to seek to
    pub beat: f64,
}

/// Request to set tempo
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GardenSetTempoRequest {
    /// Tempo in beats per minute
    pub bpm: f64,
}
