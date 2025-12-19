use hooteproto::schema_helpers::*;
use schemars;
use serde::{Deserialize, Serialize};

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
    #[schemars(
        description = "Timeout in milliseconds (capped at 10000ms to prevent SSE disconnects)"
    )]
    #[schemars(schema_with = "u64_schema")]
    pub timeout_ms: u64,

    #[schemars(description = "Job IDs to poll (empty = just timeout/sleep)")]
    #[serde(default)]
    pub job_ids: Vec<String>,

    #[schemars(
        description = "Mode: 'any' (return on first complete) or 'all' (wait for all). Default: 'any'"
    )]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SleepRequest {
    #[schemars(description = "Milliseconds to sleep (max 30000 = 30 seconds)")]
    #[schemars(schema_with = "u64_schema")]
    pub milliseconds: u64,
}

// --- Local Model Requests ---

pub fn default_creator() -> Option<String> {
    Some("unknown".to_string())
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
pub struct ArtifactUploadRequest {
    #[schemars(description = "Absolute path to file to upload")]
    pub file_path: String,

    #[schemars(
        description = "MIME type of the file (e.g., 'audio/wav', 'audio/midi', 'audio/soundfont')"
    )]
    pub mime_type: String,

    #[schemars(description = "Optional variation set ID to group related artifacts")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts")]
    #[serde(default)]
    pub tags: Vec<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactListRequest {
    #[schemars(description = "Filter by artifact tag (e.g., 'type:soundfont', 'type:midi', 'source:orpheus'). Use this for type-based discovery.")]
    pub tag: Option<String>,

    #[schemars(description = "Filter by creator (e.g., 'claude', 'user'). Useful for finding your own artifacts.")]
    pub creator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactGetRequest {
    #[schemars(description = "Artifact ID to retrieve")]
    pub id: String,
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
pub struct AbcValidateRequest {
    #[schemars(description = "ABC notation string to validate")]
    pub abc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AbcTransposeRequest {
    #[schemars(description = "ABC notation to transpose")]
    pub abc: String,

    #[schemars(description = "Semitones to transpose (positive = up)")]
    #[schemars(schema_with = "optional_i8_schema")]
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

// --- Beat Detection Types ---

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

// --- Graph Context Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphContextRequest {
    #[schemars(
        description = "Filter by artifact tag (e.g., 'type:soundfont', 'type:midi', 'source:orpheus'). Use this for type-based discovery."
    )]
    pub tag: Option<String>,

    #[schemars(
        description = "Search annotations/vibes for this text (e.g., 'warm', 'jazzy'). Finds artifacts with matching subjective descriptions."
    )]
    pub vibe_search: Option<String>,

    #[schemars(
        description = "Filter by creator (e.g., 'claude', 'user'). Useful for finding your own artifacts."
    )]
    pub creator: Option<String>,

    #[schemars(
        description = "Maximum number of artifacts to include (default: 20). Keep low for sub-agent context injection."
    )]
    #[schemars(schema_with = "optional_usize_schema")]
    pub limit: Option<usize>,

    #[schemars(
        description = "Include full metadata (default: false). Enable to see MIDI/audio technical details."
    )]
    #[serde(default)]
    pub include_metadata: bool,

    #[schemars(
        description = "Include annotations (default: true). Disable to reduce context size."
    )]
    #[serde(default = "default_true")]
    pub include_annotations: bool,

    #[schemars(
        description = "Time window in minutes for recent artifacts (default: 10). Only applies when no tag/creator filter is set."
    )]
    pub within_minutes: Option<i64>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddAnnotationRequest {
    #[schemars(description = "Artifact ID to annotate")]
    pub artifact_id: String,

    #[schemars(description = "The annotation message")]
    pub message: String,

    #[schemars(description = "Vibe keywords (e.g., 'warm, jazzy, vintage')")]
    pub vibe: Option<String>,

    #[schemars(description = "Source of the annotation (e.g., 'user', 'agent_claude')")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphQueryRequest {
    #[schemars(
        description = "GraphQL query string OR artifact ID containing a saved query. Query example: '{ Artifact(tag: \"type:midi\") { id @output } }'. Artifact example: 'artifact_abc123'. Use @output on fields you want returned. Available entry points: Artifact, SoundFont."
    )]
    pub query: String,

    #[schemars(
        description = "Variables for parameterized queries as JSON object (e.g., {\"artifact_id\": \"artifact_123\"}). Works with both inline queries and query artifacts."
    )]
    #[schemars(schema_with = "json_object_schema")]
    #[serde(default)]
    pub variables: serde_json::Value,

    #[schemars(description = "Maximum number of results to return (default: 100)")]
    #[schemars(schema_with = "optional_usize_schema")]
    pub limit: Option<usize>,
}

/// Schema function for JSON objects (not arbitrary values)
fn json_object_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    serde_json::from_value(serde_json::json!({
        "type": "object"
    }))
    .unwrap()
}
