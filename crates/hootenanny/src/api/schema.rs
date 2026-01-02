//! Internal types for service communication.
//!
//! These types are for hootenanny's internal use (e.g., beatthis HTTP client).
//! MCP tool schemas are in holler's manual_schemas module.

use serde::{Deserialize, Serialize};

// --- Beat Detection Types (internal beatthis HTTP client) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatThisServiceRequest {
    pub audio: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatThisServiceResponse {
    pub beats: Vec<f64>,
    pub downbeats: Vec<f64>,
    pub bpm: f64,
    pub num_beats: usize,
    pub num_downbeats: usize,
    pub duration: f64,
    pub frames: Option<BeatFrames>,
    pub metadata: Option<BeatMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatFrames {
    pub beat_probs: Vec<f64>,
    pub downbeat_probs: Vec<f64>,
    pub fps: u32,
    pub num_frames: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatMetadata {
    pub client_job_id: Option<String>,
}
