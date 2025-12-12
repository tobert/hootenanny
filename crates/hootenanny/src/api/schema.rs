use hooteproto::schema_helpers::*;
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
    #[schemars(schema_with = "u64_schema")]
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
    #[schemars(schema_with = "u64_schema")]
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
    #[schemars(schema_with = "optional_u32_schema")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    #[schemars(schema_with = "optional_u32_schema")]
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
    #[schemars(schema_with = "optional_u32_schema")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    #[schemars(schema_with = "optional_u32_schema")]
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
    #[schemars(schema_with = "optional_u32_schema")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    #[schemars(schema_with = "optional_u32_schema")]
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
    #[schemars(description = "Sampling temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (default: 0.95). Lower = more focused")]
    pub top_p: Option<f32>,

    #[schemars(description = "Max tokens to generate (default: 1024)")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Number of variations to generate (default: 1)")]
    pub num_variations: Option<u32>,

    #[schemars(description = "CAS hash of seed MIDI for seeded generation (optional)")]
    pub seed_hash: Option<String>,

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
    #[schemars(description = "CAS hash of MIDI to classify (required)")]
    pub midi_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClapAnalyzeRequest {
    #[schemars(description = "CAS hash of audio file to analyze (required)")]
    pub audio_hash: String,

    #[schemars(description = "Tasks to run: 'embeddings', 'genre', 'mood', 'zero_shot', 'similarity'. Default: ['embeddings']")]
    #[serde(default = "default_clap_tasks")]
    pub tasks: Vec<String>,

    #[schemars(description = "CAS hash of second audio for similarity comparison")]
    pub audio_b_hash: Option<String>,

    #[schemars(description = "Custom text labels for zero_shot classification")]
    #[serde(default)]
    pub text_candidates: Vec<String>,

    #[schemars(description = "Optional parent artifact ID")]
    pub parent_id: Option<String>,

    #[schemars(description = "Creator identifier (agent or user ID)")]
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

fn default_clap_tasks() -> Vec<String> {
    vec!["embeddings".to_string()]
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MusicgenGenerateRequest {
    #[schemars(description = "Text prompt describing the music to generate")]
    pub prompt: Option<String>,

    #[schemars(description = "Duration in seconds (0.5-30.0, default: 10.0)")]
    pub duration: Option<f32>,

    #[schemars(description = "Sampling temperature (0.01-2.0, default: 1.0)")]
    pub temperature: Option<f32>,

    #[schemars(description = "Top-k filtering (0-1000, default: 250)")]
    pub top_k: Option<u32>,

    #[schemars(description = "Nucleus sampling (0.0-1.0, default: 0.9)")]
    pub top_p: Option<f32>,

    #[schemars(description = "Classifier-free guidance scale (1.0-15.0, default: 3.0). Higher = stronger prompt adherence")]
    pub guidance_scale: Option<f32>,

    #[schemars(description = "Enable sampling vs greedy decoding (default: true)")]
    pub do_sample: Option<bool>,

    #[schemars(description = "Optional variation set ID")]
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
pub struct AnticipatoryGenerateRequest {
    #[schemars(description = "Duration in seconds to generate (1.0-120.0, default: 20.0)")]
    pub length_seconds: Option<f32>,

    #[schemars(description = "Nucleus sampling (0.1-1.0, default: 0.95)")]
    pub top_p: Option<f32>,

    #[schemars(description = "Number of variations (1-5, default: 1)")]
    pub num_variations: Option<u32>,

    #[schemars(description = "Model size: 'small', 'medium', or 'large' (default: 'small')")]
    pub model_size: Option<String>,

    #[schemars(description = "Optional variation set ID")]
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
pub struct AnticipatoryContinueRequest {
    #[schemars(description = "CAS hash of MIDI to continue (required)")]
    pub midi_hash: String,

    #[schemars(description = "Seconds of input to use as prime (1.0-60.0, default: 5.0)")]
    pub prime_seconds: Option<f32>,

    #[schemars(description = "Seconds of new music to generate (1.0-120.0, default: 20.0)")]
    pub length_seconds: Option<f32>,

    #[schemars(description = "Nucleus sampling (0.1-1.0, default: 0.95)")]
    pub top_p: Option<f32>,

    #[schemars(description = "Number of variations (1-5, default: 1)")]
    pub num_variations: Option<u32>,

    #[schemars(description = "Model size: 'small', 'medium', or 'large' (default: 'small')")]
    pub model_size: Option<String>,

    #[schemars(description = "Optional variation set ID")]
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
pub struct AnticipatoryEmbedRequest {
    #[schemars(description = "CAS hash of MIDI to embed (required)")]
    pub midi_hash: String,

    #[schemars(description = "Hidden layer to extract (-12 to -1, default: -3 = layer 10)")]
    pub embed_layer: Option<i32>,

    #[schemars(description = "Model size: 'small', 'medium', or 'large' (default: 'small')")]
    pub model_size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct YueGenerateRequest {
    #[schemars(description = "Lyrics with structure markers like [verse], [chorus], [bridge]")]
    pub lyrics: String,

    #[schemars(description = "Genre (e.g., 'Pop', 'Rock', 'Jazz', 'Electronic'). Default: 'Pop'")]
    pub genre: Option<String>,

    #[schemars(description = "Max tokens for stage 1 generation (default: 3000)")]
    pub max_new_tokens: Option<u32>,

    #[schemars(description = "Number of song segments to generate (default: 2)")]
    pub run_n_segments: Option<u32>,

    #[schemars(description = "Random seed for reproducibility (default: 42)")]
    pub seed: Option<u64>,

    #[schemars(description = "Optional variation set ID")]
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

    #[schemars(description = "MIME type of the file (e.g., 'audio/wav', 'audio/midi', 'audio/soundfont')")]
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
pub struct MidiToWavRequest {
    #[schemars(description = "CAS hash of MIDI file to render (required)")]
    pub input_hash: String,

    #[schemars(description = "CAS hash of SoundFont file (required)")]
    pub soundfont_hash: String,

    #[schemars(description = "Sample rate (default: 44100)")]
    #[schemars(schema_with = "optional_u32_schema")]
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
    #[schemars(schema_with = "optional_u16_schema")]
    pub tempo_override: Option<u16>,

    #[schemars(description = "Semitones to transpose")]
    #[schemars(schema_with = "optional_i8_schema")]
    pub transpose: Option<i8>,

    #[schemars(description = "MIDI velocity (1-127)")]
    #[schemars(schema_with = "optional_u8_schema")]
    pub velocity: Option<u8>,

    #[schemars(description = "MIDI channel (0-15, default 0). Use 9 for GM drums.")]
    #[schemars(schema_with = "optional_u8_schema")]
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

// --- Graph Context Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphContextRequest {
    #[schemars(description = "Filter by artifact tag (e.g., 'type:soundfont', 'type:midi', 'source:orpheus'). Use this for type-based discovery.")]
    pub tag: Option<String>,

    #[schemars(description = "Search annotations/vibes for this text (e.g., 'warm', 'jazzy'). Finds artifacts with matching subjective descriptions.")]
    pub vibe_search: Option<String>,

    #[schemars(description = "Filter by creator (e.g., 'claude', 'user'). Useful for finding your own artifacts.")]
    pub creator: Option<String>,

    #[schemars(description = "Maximum number of artifacts to include (default: 20). Keep low for sub-agent context injection.")]
    #[schemars(schema_with = "optional_usize_schema")]
    pub limit: Option<usize>,

    #[schemars(description = "Include full metadata (default: false). Enable to see MIDI/audio technical details.")]
    #[serde(default)]
    pub include_metadata: bool,

    #[schemars(description = "Include annotations (default: true). Disable to reduce context size.")]
    #[serde(default = "default_true")]
    pub include_annotations: bool,
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
    #[schemars(description = "GraphQL query string OR artifact ID containing a saved query. Query example: '{ Artifact(tag: \"type:midi\") { id @output } }'. Artifact example: 'artifact_abc123'. Use @output on fields you want returned. Available entry points: Artifact, SoundFont.")]
    pub query: String,

    #[schemars(description = "Variables for parameterized queries as JSON object (e.g., {\"artifact_id\": \"artifact_123\"}). Works with both inline queries and query artifacts.")]
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
    })).unwrap()
}

// --- Sampling Requests ---

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SampleLlmRequest {
    #[schemars(description = "The question or prompt to send to the client's LLM")]
    pub prompt: String,

    #[schemars(description = "Maximum tokens to generate (default: 500)")]
    #[schemars(schema_with = "optional_u32_schema")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Temperature 0.0-2.0 (default: 1.0). Higher = more random")]
    pub temperature: Option<f64>,

    #[schemars(description = "System prompt to use (optional)")]
    pub system_prompt: Option<String>,
}
