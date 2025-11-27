use serde::{Deserialize, Serialize};
use rmcp::schemars;

/// Request to add a node to the conversation tree.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddNodeRequest {
    #[schemars(description = "Note to play (C, D, E, F, G, A, B)")]
    pub what: String,

    #[schemars(description = "How to play it (softly, normally, boldly, questioning)")]
    pub how: String,

    #[schemars(description = "Valence: joy-sorrow axis, -1.0 to 1.0")]
    pub valence: f32,

    #[schemars(description = "Arousal: energy-stillness axis, 0.0 to 1.0")]
    pub arousal: f32,

    #[schemars(description = "Agency: initiative-responsiveness axis, -1.0 to 1.0")]
    pub agency: f32,

    #[schemars(description = "Agent ID (your identifier)")]
    pub agent_id: String,

    #[schemars(description = "Optional branch ID (defaults to current branch)")]
    pub branch_id: Option<String>,

    #[schemars(description = "Optional description of this musical contribution")]
    pub description: Option<String>,

    // Artifact/variation tracking fields
    #[schemars(description = "Optional variation set ID to group related contributions")]
    pub variation_set_id: Option<String>,

    #[schemars(description = "Optional parent artifact ID for refinements")]
    pub parent_id: Option<String>,

    #[schemars(description = "Optional tags for organizing artifacts (e.g., ['role:melody', 'emotion:joyful'])")]
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to fork a conversation branch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ForkRequest {
    #[schemars(description = "Name for the new branch")]
    pub branch_name: String,

    #[schemars(description = "Node ID to fork from (defaults to current head)")]
    pub from_node: Option<u64>,

    #[schemars(description = "Reason for forking")]
    pub reason_description: String,

    #[schemars(description = "Agent IDs participating in this fork")]
    pub participants: Vec<String>,
}

/// Request to merge two branches.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MergeRequest {
    #[schemars(description = "Branch to merge from")]
    pub from: String,
    #[schemars(description = "Branch to merge into")]
    pub into: String,
}

/// Request to prune a branch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PruneRequest {
    #[schemars(description = "Branch to prune")]
    pub branch: String,
}

/// Request to evaluate a branch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EvaluateRequest {
    #[schemars(description = "Branch to evaluate")]
    pub branch: String,
}

/// Request to get the musical context.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetContextRequest {
    #[schemars(description = "Time to get the context at")]
    pub at_time: String,
}

/// Request to broadcast a message.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BroadcastMessageRequest {
    #[schemars(description = "Message to broadcast")]
    pub msg: String,
}

// --- Job Management Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetJobStatusRequest {
    #[schemars(description = "Job ID to check")]
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WaitForJobRequest {
    #[schemars(description = "Job ID to wait for")]
    pub job_id: String,

    #[schemars(description = "Timeout in seconds (default: 86400 = 24 hours)")]
    pub timeout_seconds: Option<u64>,
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrpheusLoopsRequest {
    #[schemars(description = "CAS hash of seed MIDI (optional)")]
    pub seed_hash: Option<String>,

    #[schemars(description = "Model variant (default: 'loops'). Recommended: 'loops'")]
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
pub struct OrpheusClassifyRequest {
    #[schemars(description = "Model to use (default: 'classifier')")]
    pub model: Option<String>,

    #[schemars(description = "CAS hash of MIDI file to classify")]
    pub input_hash: String,
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
pub struct GraphQueryRequest {
    #[schemars(description = "GraphQL query to execute against the audio graph")]
    pub query: String,
}

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
