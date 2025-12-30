//! Typed response types for tool dispatch.
//!
//! These types provide structured responses for internal communication.
//! JSON conversion happens only at gateway edges (holler).
//!
//! ## Design Principles
//!
//! 1. **Rich types** - Use domain types, not primitives
//! 2. **Option for optional** - Use `Option<T>` instead of nullable JSON
//! 3. **Enums for variants** - Use Rust enums, not string discriminators
//! 4. **Cap'n Proto friendly** - All types map to capnp schemas

use serde::{Deserialize, Serialize};

/// Unified response type for all tools.
///
/// Each variant corresponds to a tool or tool category.
/// Gateway layer (holler) converts this to JSON; internal layers use Cap'n Proto.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResponse {
    // === CAS Operations ===
    CasStored(CasStoredResponse),
    CasContent(CasContentResponse),
    CasInspected(CasInspectedResponse),
    CasStats(CasStatsResponse),

    // === Artifacts ===
    ArtifactCreated(ArtifactCreatedResponse),
    ArtifactInfo(ArtifactInfoResponse),
    ArtifactList(ArtifactListResponse),

    // === Jobs ===
    JobStarted(JobStartedResponse),
    JobStatus(JobStatusResponse),
    JobList(JobListResponse),
    JobPollResult(JobPollResultResponse),
    JobPoll(JobPollResponse),
    JobCancel(JobCancelResponse),
    JobSleep(JobSleepResponse),

    // === ABC Notation ===
    AbcParsed(AbcParsedResponse),
    AbcValidated(AbcValidatedResponse),
    AbcTransposed(AbcTransposedResponse),
    AbcConverted(AbcConvertedResponse),
    AbcToMidi(AbcToMidiResponse),

    // === Audio Conversion ===
    MidiToWav(MidiToWavResponse),

    // === SoundFont ===
    SoundfontInfo(SoundfontInfoResponse),
    SoundfontPresetInfo(SoundfontPresetInfoResponse),

    // === Orpheus MIDI Generation ===
    OrpheusGenerated(OrpheusGeneratedResponse),
    OrpheusClassified(OrpheusClassifiedResponse),

    // === Audio Generation ===
    AudioGenerated(AudioGeneratedResponse),

    // === Audio Analysis ===
    BeatsAnalyzed(BeatsAnalyzedResponse),
    ClapAnalyzed(ClapAnalyzedResponse),

    // === Garden/Transport ===
    GardenStatus(GardenStatusResponse),
    GardenRegions(GardenRegionsResponse),
    GardenRegionCreated(GardenRegionCreatedResponse),
    GardenQueryResult(GardenQueryResultResponse),
    GardenAudioStatus(GardenAudioStatusResponse),
    GardenInputStatus(GardenInputStatusResponse),
    GardenMonitorStatus(GardenMonitorStatusResponse),

    // === Graph ===
    GraphIdentity(GraphIdentityResponse),
    GraphIdentities(GraphIdentitiesResponse),
    GraphConnection(GraphConnectionResponse),
    GraphTags(GraphTagsResponse),
    GraphContext(GraphContextResponse),
    GraphQueryResult(GraphQueryResultResponse),
    GraphBind(GraphBindResponse),
    GraphTag(GraphTagResponse),
    GraphConnect(GraphConnectResponse),

    // === Config ===
    ConfigValue(ConfigValueResponse),

    // === Admin ===
    ToolsList(ToolsListResponse),

    // === Simple Acknowledgements ===
    Ack(AckResponse),

    // === Annotations ===
    AnnotationAdded(AnnotationAddedResponse),

    // === Vibeweaver (Python kernel) ===
    WeaveEval(WeaveEvalResponse),
    WeaveSession(WeaveSessionResponse),
    WeaveReset(WeaveResetResponse),
    WeaveHelp(WeaveHelpResponse),

    // === Timeline Scheduling ===
    Scheduled(ScheduledResponse),

    // === Native Tools ===
    /// Response from analyze tool (multiple analysis tasks)
    AnalyzeResult(AnalyzeResultResponse),
    /// Response from project tool (format conversion)
    ProjectResult(ProjectResultResponse),
}

// =============================================================================
// CAS Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasStoredResponse {
    pub hash: String,
    pub size: usize,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasContentResponse {
    pub hash: String,
    pub size: usize,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasInspectedResponse {
    pub hash: String,
    pub exists: bool,
    pub size: Option<usize>,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasStatsResponse {
    pub total_items: u64,
    pub total_bytes: u64,
    pub cas_dir: String,
}

// =============================================================================
// Artifact Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactCreatedResponse {
    pub artifact_id: String,
    pub content_hash: String,
    pub tags: Vec<String>,
    pub creator: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactInfoResponse {
    pub id: String,
    pub content_hash: String,
    pub mime_type: String,
    pub tags: Vec<String>,
    pub creator: String,
    pub created_at: u64,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
    pub metadata: Option<ArtifactMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub duration_seconds: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub midi_info: Option<MidiMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiMetadata {
    pub tracks: u16,
    pub ticks_per_quarter: u16,
    pub duration_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactListResponse {
    pub artifacts: Vec<ArtifactInfoResponse>,
    pub count: usize,
}

// =============================================================================
// Job Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobStartedResponse {
    pub job_id: String,
    pub tool: String,
}

/// Response from tools that spawn async jobs (more detailed than JobStartedResponse)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSpawnResponse {
    /// Unique job identifier
    pub job_id: String,
    /// Current job status
    pub status: JobState,
    /// Artifact ID if job completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// Content hash if job completed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Human-readable message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Pending,
    Running,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobStatusResponse {
    pub job_id: String,
    pub status: JobState,
    pub source: String,
    pub result: Option<Box<ToolResponse>>,
    pub error: Option<String>,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobListResponse {
    pub jobs: Vec<JobStatusResponse>,
    pub total: usize,
    pub by_status: JobCounts,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JobCounts {
    pub pending: usize,
    pub running: usize,
    pub complete: usize,
    pub failed: usize,
    pub cancelled: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobPollResultResponse {
    pub completed: Vec<String>,
    pub failed: Vec<String>,
    pub pending: Vec<String>,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobPollResponse {
    pub completed: Vec<String>,
    pub failed: Vec<String>,
    pub pending: Vec<String>,
    pub reason: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobCancelResponse {
    pub job_id: String,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSleepResponse {
    pub slept_ms: u64,
}

// =============================================================================
// ABC Notation Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcParsedResponse {
    pub valid: bool,
    pub title: Option<String>,
    pub key: Option<String>,
    pub meter: Option<String>,
    pub tempo: Option<u16>,
    pub notes_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcValidatedResponse {
    pub valid: bool,
    pub errors: Vec<AbcValidationError>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcValidationError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcTransposedResponse {
    pub abc: String,
    pub original_key: Option<String>,
    pub new_key: Option<String>,
    pub semitones: i8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcConvertedResponse {
    pub artifact_id: String,
    pub content_hash: String,
    pub duration_seconds: f64,
    pub notes_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcToMidiResponse {
    pub artifact_id: String,
    pub content_hash: String,
}

// =============================================================================
// Audio Conversion Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiToWavResponse {
    pub artifact_id: String,
    pub content_hash: String,
    pub sample_rate: u32,
    pub duration_secs: Option<f64>,
}

// =============================================================================
// SoundFont Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundfontInfoResponse {
    pub name: String,
    pub presets: Vec<SoundfontPreset>,
    pub preset_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundfontPreset {
    pub bank: u16,
    pub program: u16,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundfontPresetInfoResponse {
    pub bank: u16,
    pub program: u16,
    pub name: String,
    pub regions: Vec<SoundfontRegion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundfontRegion {
    pub key_low: u8,
    pub key_high: u8,
    pub velocity_low: u8,
    pub velocity_high: u8,
    pub sample_name: Option<String>,
}

// =============================================================================
// Orpheus Responses
// =============================================================================

/// Response from Orpheus MIDI generation (sample, extend, bridge)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrpheusGeneratedResponse {
    /// CAS hashes of generated MIDI files
    pub output_hashes: Vec<String>,
    /// Artifact IDs in the store
    pub artifact_ids: Vec<String>,
    /// Tokens used per variation
    pub tokens_per_variation: Vec<u64>,
    /// Total tokens consumed
    pub total_tokens: u64,
    /// Variation set grouping (if multiple variations)
    pub variation_set_id: Option<String>,
    /// Human-readable summary
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrpheusClassifiedResponse {
    pub classifications: Vec<MidiClassification>,
}

// =============================================================================
// Audio Generation Responses
// =============================================================================

/// Response from audio generation (MusicGen, YuE)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioGeneratedResponse {
    pub artifact_id: String,
    pub content_hash: String,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub format: AudioFormat,
    /// Optional genre (for YuE)
    pub genre: Option<String>,
}

/// Audio file format
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Wav,
    Mp3,
    Flac,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiClassification {
    pub label: String,
    pub confidence: f32,
}

// =============================================================================
// Audio Analysis Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatsAnalyzedResponse {
    pub beats: Vec<f64>,
    pub downbeats: Vec<f64>,
    pub estimated_bpm: f64,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClapAnalyzedResponse {
    pub embeddings: Option<Vec<f32>>,
    pub genre: Option<Vec<ClapClassification>>,
    pub mood: Option<Vec<ClapClassification>>,
    pub zero_shot: Option<Vec<ClapClassification>>,
    pub similarity: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClapClassification {
    pub label: String,
    pub score: f32,
}

// =============================================================================
// Garden/Transport Responses
// =============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransportState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenStatusResponse {
    pub state: TransportState,
    pub position_beats: f64,
    pub tempo_bpm: f64,
    pub region_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenRegionInfo {
    pub region_id: String,
    pub position: f64,
    pub duration: f64,
    pub behavior_type: String,
    pub content_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenRegionsResponse {
    pub regions: Vec<GardenRegionInfo>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenRegionCreatedResponse {
    pub region_id: String,
    pub position: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenQueryResultResponse {
    pub results: Vec<serde_json::Value>, // Trustfall results can be complex
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenAudioStatusResponse {
    pub attached: bool,
    pub device_name: Option<String>,
    pub sample_rate: Option<u32>,
    pub latency_frames: Option<u32>,
    pub callbacks: u64,
    pub samples_written: u64,
    pub underruns: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenInputStatusResponse {
    pub attached: bool,
    pub device_name: Option<String>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub monitor_enabled: bool,
    pub monitor_gain: f32,
    pub callbacks: u64,
    pub samples_captured: u64,
    pub overruns: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenMonitorStatusResponse {
    pub enabled: bool,
    pub gain: f64,
}

// =============================================================================
// Graph Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphIdentityResponse {
    pub id: String,
    pub name: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphIdentityInfo {
    pub id: String,
    pub name: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphIdentitiesResponse {
    pub identities: Vec<GraphIdentityInfo>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphConnectionResponse {
    pub connection_id: String,
    pub from_identity: String,
    pub from_port: String,
    pub to_identity: String,
    pub to_port: String,
    pub transport: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTagInfo {
    pub namespace: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTagsResponse {
    pub identity_id: String,
    pub tags: Vec<GraphTagInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphContextResponse {
    pub context: String,
    pub artifact_count: usize,
    pub identity_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQueryResultResponse {
    pub results: Vec<serde_json::Value>, // Trustfall results
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphBindResponse {
    pub identity_id: String,
    pub name: String,
    pub hints_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTagResponse {
    pub identity_id: String,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphConnectResponse {
    pub from_identity: String,
    pub from_port: String,
    pub to_identity: String,
    pub to_port: String,
}

// =============================================================================
// Config Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigValueResponse {
    pub section: Option<String>,
    pub key: Option<String>,
    pub value: ConfigValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Object(std::collections::HashMap<String, ConfigValue>),
    Array(Vec<ConfigValue>),
    Null,
}

// =============================================================================
// Admin Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolsListResponse {
    pub tools: Vec<crate::ToolInfo>,
    pub count: usize,
}

// =============================================================================
// Simple Responses
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AckResponse {
    pub message: String,
}

impl AckResponse {
    pub fn ok() -> Self {
        Self {
            message: "ok".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationAddedResponse {
    pub artifact_id: String,
    pub annotation_id: String,
}

// =============================================================================
// Vibeweaver Responses
// =============================================================================

/// Type of Python output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaveOutputType {
    /// Expression evaluation - returns repr() of result
    Expression,
    /// Statement execution - captures stdout/stderr
    Statement,
}

/// Result of evaluating Python code
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaveEvalResponse {
    pub output_type: WeaveOutputType,
    /// Expression result (repr string), None for statements
    pub result: Option<String>,
    /// Captured stdout, None for expressions
    pub stdout: Option<String>,
    /// Captured stderr
    pub stderr: Option<String>,
}

/// Session info for active vibeweaver session
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaveSessionInfo {
    pub id: String,
    pub name: String,
    pub vibe: Option<String>,
}

/// Current session state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaveSessionResponse {
    pub session: Option<WeaveSessionInfo>,
    pub message: Option<String>,
}

/// Kernel reset confirmation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaveResetResponse {
    pub reset: bool,
    pub message: String,
}

/// Help documentation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaveHelpResponse {
    pub help: String,
    pub topic: Option<String>,
}

// =============================================================================
// Timeline Scheduling
// =============================================================================

/// Response from scheduling content on the timeline
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduledResponse {
    pub success: bool,
    pub message: String,
    pub region_id: String,
    pub position: f64,
    pub duration: f64,
    pub artifact_id: String,
}

// =============================================================================
// Native Tool Responses
// =============================================================================

/// Response from the analyze tool with multiple analysis results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeResultResponse {
    /// Content hash of the analyzed content
    pub content_hash: String,
    /// Analysis results keyed by task type
    pub results: serde_json::Value,
    /// Human-readable summary of results
    pub summary: String,
    /// Optional artifact ID if content is stored
    pub artifact_id: Option<String>,
}

/// Response from the project tool for format conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectResultResponse {
    /// Artifact ID of the projected content
    pub artifact_id: String,
    /// Content hash of the projected content
    pub content_hash: String,
    /// Type of projection performed (e.g., "midi_to_audio", "abc_to_midi")
    pub projection_type: String,
    /// Optional duration in seconds (for audio projections)
    pub duration_seconds: Option<f64>,
    /// Optional sample rate (for audio projections)
    pub sample_rate: Option<u32>,
}

// =============================================================================
// Conversion to JSON (for gateway edge)
// =============================================================================

impl ToolResponse {
    /// Convert to JSON for gateway responses.
    ///
    /// This is the ONLY place JSON conversion should happen for responses.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|e| {
            serde_json::json!({
                "error": "serialization_failed",
                "message": e.to_string()
            })
        })
    }

    /// Create a simple ack response.
    pub fn ack(message: impl Into<String>) -> Self {
        ToolResponse::Ack(AckResponse {
            message: message.into(),
        })
    }

    /// Create a job started response.
    pub fn job_started(job_id: impl Into<String>, tool: impl Into<String>) -> Self {
        ToolResponse::JobStarted(JobStartedResponse {
            job_id: job_id.into(),
            tool: tool.into(),
        })
    }
}
