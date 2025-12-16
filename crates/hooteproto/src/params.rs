//! Tool parameter types with JSON Schema derivation
//!
//! These types are used by holler to generate accurate tool schemas.
//! They mirror the Payload variants but as separate structs that can derive JsonSchema.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// CAS Tools
// ============================================================================

/// Parameters for cas_store tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CasStoreParams {
    /// Base64 encoded data to store
    pub data: String,
    /// MIME type of the content
    pub mime_type: String,
}

/// Parameters for cas_inspect tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CasInspectParams {
    /// Content hash to inspect
    pub hash: String,
}

/// Parameters for cas_upload_file tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CasUploadFileParams {
    /// Path to file to upload
    pub file_path: String,
    /// MIME type of the file
    pub mime_type: String,
}

// ============================================================================
// Orpheus Tools
// ============================================================================

/// Parameters for orpheus_generate tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrpheusGenerateParams {
    /// Model to use (optional)
    pub model: Option<String>,
    /// Temperature for generation (0.0-2.0)
    pub temperature: Option<f32>,
    /// Top-p sampling parameter
    pub top_p: Option<f32>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Number of variations to generate
    pub num_variations: Option<u32>,
    /// Variation set ID for grouping
    pub variation_set_id: Option<String>,
    /// Parent artifact ID
    pub parent_id: Option<String>,
    /// Tags for the generated artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

/// Parameters for orpheus_continue tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrpheusContinueParams {
    /// Hash of MIDI to continue from
    pub input_hash: String,
    /// Model to use (optional)
    pub model: Option<String>,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Top-p sampling parameter
    pub top_p: Option<f32>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Number of variations to generate
    pub num_variations: Option<u32>,
    /// Variation set ID for grouping
    pub variation_set_id: Option<String>,
    /// Parent artifact ID
    pub parent_id: Option<String>,
    /// Tags for the generated artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

// ============================================================================
// Job Tools
// ============================================================================

/// Parameters for job_status tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobStatusParams {
    /// Job ID to check
    pub job_id: String,
}

/// Parameters for job_poll tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobPollParams {
    /// Job IDs to poll
    pub job_ids: Vec<String>,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
    /// Poll mode: "any" or "all"
    #[serde(default = "default_poll_mode")]
    pub mode: String,
}

fn default_poll_mode() -> String {
    "any".to_string()
}

// ============================================================================
// ABC Tools
// ============================================================================

/// Parameters for abc_to_midi tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AbcToMidiParams {
    /// ABC notation string
    pub abc: String,
    /// Override tempo (BPM)
    pub tempo_override: Option<u16>,
    /// Transpose by semitones
    pub transpose: Option<i8>,
    /// MIDI velocity (0-127)
    pub velocity: Option<u8>,
    /// MIDI channel (0-15)
    pub channel: Option<u8>,
    /// Variation set ID for grouping
    pub variation_set_id: Option<String>,
    /// Parent artifact ID
    pub parent_id: Option<String>,
    /// Tags for the generated artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

/// Parameters for abc_parse tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AbcParseParams {
    /// ABC notation string to parse
    pub abc: String,
}

/// Parameters for abc_validate tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AbcValidateParams {
    /// ABC notation string to validate
    pub abc: String,
}

// ============================================================================
// Conversion Tools
// ============================================================================

/// Parameters for convert_midi_to_wav tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MidiToWavParams {
    /// Hash of MIDI file in CAS
    pub input_hash: String,
    /// Hash of SoundFont file in CAS
    pub soundfont_hash: String,
    /// Sample rate (default 44100)
    pub sample_rate: Option<u32>,
    /// Variation set ID for grouping
    pub variation_set_id: Option<String>,
    /// Parent artifact ID
    pub parent_id: Option<String>,
    /// Tags for the generated artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

// ============================================================================
// Graph Tools
// ============================================================================

/// Parameters for graph_query tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphQueryParams {
    /// Trustfall query string
    pub query: String,
    /// Query variables
    #[serde(default)]
    pub variables: serde_json::Value,
    /// Maximum results to return
    pub limit: Option<usize>,
}

/// Parameters for graph_bind tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphBindParams {
    /// Identity ID to bind
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Hints for identity matching
    #[serde(default)]
    pub hints: Vec<GraphHintParam>,
}

/// A hint for graph identity matching
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphHintParam {
    /// Hint kind (e.g., "usb_vendor", "alsa_card")
    pub kind: String,
    /// Hint value
    pub value: String,
    /// Confidence (0.0-1.0)
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orpheus_generate_schema() {
        let schema = schemars::schema_for!(OrpheusGenerateParams);
        let json = serde_json::to_string_pretty(&schema).unwrap();
        assert!(json.contains("temperature"));
        assert!(json.contains("tags"));
    }

    #[test]
    fn cas_store_schema() {
        let schema = schemars::schema_for!(CasStoreParams);
        let json = serde_json::to_string_pretty(&schema).unwrap();
        assert!(json.contains("data"));
        assert!(json.contains("mime_type"));
    }
}
