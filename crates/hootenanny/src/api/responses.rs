//! Response types for MCP tool output schemas
//!
//! These types define the structured content returned by tools per MCP 2025-06-18 spec.
//! Each implements JsonSchema for output schema generation.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// Job Management Responses
// ============================================================================

/// Response from tools that spawn async jobs
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobSpawnResponse {
    /// Unique job identifier
    pub job_id: String,

    /// Current job status
    pub status: JobStatus,

    /// Artifact ID if job completed (job_status only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,

    /// Content hash if job completed (job_status only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,

    /// Human-readable message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Job status enum
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Response from job_status tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobStatusResponse {
    pub job_id: String,
    pub status: JobStatus,
    pub tool_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

/// Response from job_list tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobListResponse {
    pub jobs: Vec<JobSummary>,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobSummary {
    pub job_id: String,
    pub tool_name: String,
    pub status: JobStatus,
    pub created_at: i64,
}

/// Response from job_poll tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobPollResponse {
    pub completed: Vec<String>,
    pub failed: Vec<String>,
    pub pending: Vec<String>,
    pub reason: String,
    pub elapsed_ms: u64,
}

/// Response from job_cancel tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobCancelResponse {
    pub job_id: String,
    pub cancelled: bool,
    pub message: String,
}

/// Response from job_sleep tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct JobSleepResponse {
    pub slept_ms: u64,
    pub completed_at: i64,
}

// ============================================================================
// CAS (Content Addressable Storage) Responses
// ============================================================================

/// Response from cas_store tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CasStoreResponse {
    pub hash: String,
    pub size_bytes: u64,
    pub mime_type: String,
}

/// Response from cas_inspect tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CasInspectResponse {
    pub hash: String,
    pub size_bytes: u64,
    pub mime_type: String,
    pub exists: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
}

/// Response from cas_upload_file tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CasUploadResponse {
    pub hash: String,
    pub size_bytes: u64,
    pub mime_type: String,
    pub source_path: String,
}

// ============================================================================
// Graph Responses
// ============================================================================

/// Response from graph_bind tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphBindResponse {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

/// Response from graph_tag tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphTagResponse {
    pub identity_id: String,
    pub tags: Vec<TagInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagInfo {
    pub namespace: String,
    pub value: String,
}

/// Response from graph_connect tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphConnectResponse {
    pub connection_id: String,
    pub from: String,
    pub to: String,
    pub transport: String,
}

/// Response from graph_find tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphFindResponse {
    pub identities: Vec<IdentitySummary>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IdentitySummary {
    pub id: String,
    pub name: String,
    pub tags: Vec<String>,
}

/// Response from graph_context tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphContextResponse {
    pub artifacts: Vec<serde_json::Value>,
    pub summary: ContextSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextSummary {
    pub total: usize,
    pub by_type: std::collections::HashMap<String, usize>,
}

/// Response from graph_query tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphQueryResponse {
    pub results: Vec<serde_json::Value>,
    pub count: usize,
}

// ============================================================================
// ABC Notation Responses
// ============================================================================

/// Response from abc_parse tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AbcParseResponse {
    pub success: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ast: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

/// Response from abc_to_midi tool (spawns job)
pub type AbcToMidiResponse = JobSpawnResponse;

/// Response from abc_validate tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AbcValidateResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Response from abc_transpose tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AbcTransposeResponse {
    pub abc: String,
    pub transposed_by: i8,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_key: Option<String>,
}

// ============================================================================
// SoundFont Responses
// ============================================================================

/// Response from soundfont_inspect tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SoundfontInspectResponse {
    pub soundfont_hash: String,
    pub presets: Vec<PresetInfo>,
    pub has_drum_presets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PresetInfo {
    pub bank: i32,
    pub program: i32,
    pub name: String,
}

/// Response from soundfont_preset_inspect tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SoundfontPresetResponse {
    pub soundfont_hash: String,
    pub bank: i32,
    pub program: i32,
    pub preset_name: String,
    pub instruments: Vec<InstrumentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstrumentInfo {
    pub name: String,
    pub key_range: Option<(u8, u8)>,
    pub velocity_range: Option<(u8, u8)>,
}

// ============================================================================
// Conversion Tool Responses
// ============================================================================

/// Response from convert_midi_to_wav tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MidiToWavResponse {
    pub artifact_id: String,
    pub content_hash: String,
    pub size_bytes: usize,
    pub duration_secs: f64,
    pub sample_rate: u32,
}

// ============================================================================
// Analysis Tool Responses
// ============================================================================

/// Response from beatthis_analyze tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BeatthisAnalyzeResponse {
    pub beats: Vec<f64>,
    pub downbeats: Vec<f64>,
    pub estimated_bpm: f64,
    pub confidence: f64,
}

/// Response from add_annotation tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AddAnnotationResponse {
    pub artifact_id: String,
    pub annotation_id: String,
    pub success: bool,
}
