use serde::{Deserialize, Serialize};
use schemars;

// --- Job Management Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetJobStatusRequest {
    #[schemars(description = "Job ID to check")]
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CancelJobRequest {
    #[schemars(description = "Job ID to cancel")]
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PollRequest {
    #[schemars(description = "Timeout in milliseconds (capped at 10000ms to prevent SSE disconnects)")]
    pub timeout_ms: u64,

    #[schemars(description = "Job IDs to poll (empty = just timeout/sleep)")]
    #[serde(default)]
    pub job_ids: Vec<String>,

    #[schemars(description = "Mode: 'any' (return on first complete) or 'all' (wait for all). Default: 'any'")]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SleepRequest {
    #[schemars(description = "Milliseconds to sleep (max 30000 = 30 seconds)")]
    pub milliseconds: u64,
}

// --- Local Model Requests ---

pub fn default_creator() -> Option<String> {
    Some("unknown".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusGenerateRequest {
    #[schemars(description = "Model variant (default: 'base'). Options: 'base', 'children', 'mono_melodies'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts (e.g., ['phase:initial', 'experiment:upbeat'])")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusGenerateSeededRequest {
    #[schemars(description = "CAS hash of seed MIDI (required)")]
    pub seed_hash: String,

    #[schemars(description = "Model variant (default: 'base'). Options: 'base', 'children', 'mono_melodies'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusContinueRequest {
    #[schemars(description = "CAS hash of MIDI to continue (required)")]
    pub input_hash: String,

    #[schemars(description = "Model variant (default: 'base'). Options: 'base', 'children', 'mono_melodies'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusBridgeRequest {
    #[schemars(description = "CAS hash of first section MIDI (required)")]
    pub section_a_hash: String,

    #[schemars(description = "CAS hash of second section (optional, for future use)")]
    pub section_b_hash: Option<String>,

    #[schemars(description = "Model variant (default: 'bridge'). Recommended: 'bridge'")]
    pub model: Option<String>,

    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024). Lower = shorter output")]
    pub max_tokens: Option<u32>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related generations")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

// --- CAS Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CasStoreRequest {
    pub content_base64: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CasInspectRequest {
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UploadFileRequest {
    #[schemars(description = "Absolute path to file to upload")]
    pub file_path: String,

    #[schemars(description = "MIME type of the file (e.g., 'audio/soundfont', 'audio/midi')")]
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MidiToWavRequest {
    #[schemars(description = "CAS hash of MIDI file to render (required)")]
    pub input_hash: String,

    #[schemars(description = "CAS hash of SoundFont file (required)")]
    pub soundfont_hash: String,

    #[schemars(description = "Sample rate (default: 44100)")]
    pub sample_rate: Option<u32>,

    // Artifact tracking fields
    #[schemars(description = "Optional variation set ID to group related conversions")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

// --- Audio Graph Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphFindRequest {
    #[schemars(description = "Filter by identity name (substring match)")]
    pub name: Option<String>,

    #[schemars(description = "Filter by tag namespace")]
    pub tag_namespace: Option<String>,

    #[schemars(description = "Filter by tag value")]
    pub tag_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphBindRequest {
    #[schemars(description = "Identity ID to create or update")]
    pub id: String,

    #[schemars(description = "Human-readable name for the identity")]
    pub name: String,

    #[schemars(description = "Hints for matching devices (kind, value, confidence)")]
    #[serde(default)]
    pub hints: Vec<GraphHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphHint {
    #[schemars(description = "Hint kind (usb_device_id, midi_name, alsa_card, pipewire_name)")]
    pub kind: String,

    #[schemars(description = "Hint value")]
    pub value: String,

    #[schemars(description = "Confidence score 0.0-1.0")]
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphTagRequest {
    #[schemars(description = "Identity ID to tag")]
    pub identity_id: String,

    #[schemars(description = "Tag namespace")]
    pub namespace: String,

    #[schemars(description = "Tag value")]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphConnectRequest {
    #[schemars(description = "Source identity ID")]
    pub from_identity: String,

    #[schemars(description = "Source port name")]
    pub from_port: String,

    #[schemars(description = "Destination identity ID")]
    pub to_identity: String,

    #[schemars(description = "Destination port name")]
    pub to_port: String,

    #[schemars(description = "Transport kind (din_midi, usb_midi, patch_cable_cv, etc.)")]
    pub transport: Option<String>,
}

// --- ABC Notation Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcParseRequest {
    #[schemars(description = "ABC notation string to parse")]
    pub abc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcToMidiRequest {
    #[schemars(description = "ABC notation to convert")]
    pub abc: String,

    #[schemars(description = "Override tempo (BPM)")]
    pub tempo_override: Option<u16>,

    #[schemars(description = "Semitones to transpose")]
    pub transpose: Option<i8>,

    #[schemars(description = "MIDI velocity (1-127)")]
    pub velocity: Option<u8>,

    #[schemars(description = "MIDI channel (0-15, default 0). Use 9 for GM drums.")]
    pub channel: Option<u8>,

    // Standard artifact fields
    #[schemars(description = "Optional variation set ID")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID")]
    pub parent_id: Option<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcValidateRequest {
    #[schemars(description = "ABC notation string to validate")]
    pub abc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcTransposeRequest {
    #[schemars(description = "ABC notation to transpose")]
    pub abc: String,

    #[schemars(description = "Semitones to transpose (positive = up)")]
    pub semitones: Option<i8>,

    #[schemars(description = "Target key (e.g., 'Am', 'Bb')")]
    pub target_key: Option<String>,
}

// --- SoundFont Inspection ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SoundfontInspectRequest {
    #[schemars(description = "CAS hash of SoundFont file to inspect")]
    pub soundfont_hash: String,

    #[schemars(description = "Include detailed drum mappings for percussion presets (bank 128)")]
    #[serde(default)]
    pub include_drum_map: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SoundfontPresetInspectRequest {
    #[schemars(description = "CAS hash of SoundFont file")]
    pub soundfont_hash: String,

    #[schemars(description = "Bank number (0 for melodic, 128 for drums)")]
    pub bank: i32,

    #[schemars(description = "Program/preset number (0-127)")]
    pub program: i32,
}

// --- Beat Detection Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeBeatsRequest {
    #[schemars(description = "Path to audio file (WAV, 22050 Hz mono, ≤30s)")]
    pub audio_path: Option<String>,

    #[schemars(description = "CAS hash of audio file (alternative to path)")]
    pub audio_hash: Option<String>,

    #[schemars(description = "Return frame-level probabilities (can be large)")]
    #[serde(default)]
    pub include_frames: bool,
}

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
