//! Typed tool requests for the hooteproto protocol.
//!
//! Each variant represents a complete, typed request for a specific tool.
//! No JSON involved - these serialize directly to Cap'n Proto.

use crate::timing::ToolTiming;
use serde::{Deserialize, Serialize};

/// All tool requests. Discriminated by `tool` field in serialized form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum ToolRequest {
    // ==========================================================================
    // CAS Operations
    // ==========================================================================
    /// Store raw bytes in CAS
    CasStore(CasStoreRequest),
    /// Inspect content metadata without retrieving
    CasInspect(CasInspectRequest),
    /// Retrieve content from CAS
    CasGet(CasGetRequest),
    /// Upload file from filesystem to CAS
    CasUploadFile(CasUploadFileRequest),
    /// Get CAS storage statistics
    CasStats,

    // ==========================================================================
    // Artifacts
    // ==========================================================================
    /// Upload file and create artifact with metadata
    ArtifactUpload(ArtifactUploadRequest),
    /// Get artifact by ID
    ArtifactGet(ArtifactGetRequest),
    /// List artifacts with optional filters
    ArtifactList(ArtifactListRequest),
    /// Create artifact from CAS hash
    ArtifactCreate(ArtifactCreateRequest),

    // ==========================================================================
    // Orpheus MIDI Generation
    // ==========================================================================
    /// Generate MIDI from scratch
    OrpheusGenerate(OrpheusGenerateRequest),
    /// Generate MIDI from seed
    OrpheusGenerateSeeded(OrpheusGenerateSeededRequest),
    /// Continue existing MIDI
    OrpheusContinue(OrpheusContinueRequest),
    /// Create bridge between sections
    OrpheusBridge(OrpheusBridgeRequest),
    /// Generate loopable MIDI
    OrpheusLoops(OrpheusLoopsRequest),
    /// Classify MIDI content
    OrpheusClassify(OrpheusClassifyRequest),

    // ==========================================================================
    // Audio Conversion & Generation
    // ==========================================================================
    /// Render MIDI to WAV using SoundFont
    MidiToWav(MidiToWavRequest),
    /// Inspect SoundFont presets
    SoundfontInspect(SoundfontInspectRequest),
    /// Inspect specific SoundFont preset
    SoundfontPresetInspect(SoundfontPresetInspectRequest),
    /// Generate audio with MusicGen
    MusicgenGenerate(MusicgenGenerateRequest),
    /// Generate song with YuE
    YueGenerate(YueGenerateRequest),

    // ==========================================================================
    // Audio Analysis
    // ==========================================================================
    /// Analyze beats with BeatThis
    BeatthisAnalyze(BeatthisAnalyzeRequest),
    /// Analyze audio with CLAP
    ClapAnalyze(ClapAnalyzeRequest),

    // ==========================================================================
    // ABC Notation
    // ==========================================================================
    /// Parse ABC notation
    AbcParse(AbcParseRequest),
    /// Validate ABC notation
    AbcValidate(AbcValidateRequest),
    /// Transpose ABC notation
    AbcTranspose(AbcTransposeRequest),
    /// Convert ABC to MIDI
    AbcToMidi(AbcToMidiRequest),

    // ==========================================================================
    // Garden / Transport
    // ==========================================================================
    /// Get garden status
    GardenStatus,
    /// Start playback
    GardenPlay,
    /// Pause playback
    GardenPause,
    /// Stop playback
    GardenStop,
    /// Seek to position
    GardenSeek(GardenSeekRequest),
    /// Set tempo
    GardenSetTempo(GardenSetTempoRequest),
    /// Get regions in time range
    GardenGetRegions(GardenGetRegionsRequest),
    /// Create a region
    GardenCreateRegion(GardenCreateRegionRequest),
    /// Delete a region
    GardenDeleteRegion(GardenDeleteRegionRequest),
    /// Move a region
    GardenMoveRegion(GardenMoveRegionRequest),
    /// Query garden state with Trustfall
    GardenQuery(GardenQueryRequest),
    /// Emergency pause
    GardenEmergencyPause,
    /// Attach audio output
    GardenAttachAudio(GardenAttachAudioRequest),
    /// Detach audio output
    GardenDetachAudio,
    /// Get audio output status
    GardenAudioStatus,
    /// Attach audio input
    GardenAttachInput(GardenAttachInputRequest),
    /// Detach audio input
    GardenDetachInput,
    /// Get audio input status
    GardenInputStatus,
    /// Set monitor
    GardenSetMonitor(GardenSetMonitorRequest),
    /// Clear all regions
    GardenClearRegions,

    // ==========================================================================
    // Tool Help
    // ==========================================================================
    /// Get help for a tool
    GetToolHelp(GetToolHelpRequest),

    // ==========================================================================
    // Jobs
    // ==========================================================================
    /// Get job status
    JobStatus(JobStatusRequest),
    /// List jobs
    JobList(JobListRequest),
    /// Poll for job completion
    JobPoll(JobPollRequest),
    /// Cancel a job
    JobCancel(JobCancelRequest),
    /// Sleep for duration (utility)
    JobSleep(JobSleepRequest),
    /// Poll for buffered broadcast events
    EventPoll(EventPollRequest),

    // ==========================================================================
    // Graph
    // ==========================================================================
    /// Bind identity to device
    GraphBind(GraphBindRequest),
    /// Tag an identity
    GraphTag(GraphTagRequest),
    /// Connect two identities
    GraphConnect(GraphConnectRequest),
    /// Find identities
    GraphFind(GraphFindRequest),
    /// Execute Trustfall query
    GraphQuery(GraphQueryRequest),
    /// Get graph context for LLM
    GraphContext(GraphContextRequest),

    // ==========================================================================
    // Config
    // ==========================================================================
    /// Get configuration value
    ConfigGet(ConfigGetRequest),

    // ==========================================================================
    // Annotations
    // ==========================================================================
    /// Add annotation to artifact
    AddAnnotation(AddAnnotationRequest),

    // ==========================================================================
    // Vibeweaver (Python Kernel)
    // ==========================================================================
    /// Evaluate Python/Weave code
    WeaveEval(WeaveEvalRequest),
    /// Start a new session
    WeaveSession,
    /// Reset session state
    WeaveReset(WeaveResetRequest),
    /// Get help for Weave environment
    WeaveHelp(WeaveHelpRequest),

    // ==========================================================================
    // Resources & Completion
    // ==========================================================================
    /// Read a resource by URI
    ReadResource(ReadResourceRequest),
    /// List available resources
    ListResources,
    /// Get completion for prompt
    Complete(CompleteRequest),
    /// Sample LLM directly
    SampleLlm(SampleLlmRequest),

    // ==========================================================================
    // Model-Native API
    // ==========================================================================
    /// Schedule content on timeline
    Schedule(ScheduleRequest),
    /// Analyze content
    Analyze(AnalyzeRequest),
    /// Sample from a generative space
    Sample(SampleRequest),
    /// Extend existing content
    Extend(ExtendRequest),
    /// Create bridge transitions between sections
    Bridge(BridgeRequest),
    /// Project content to a different format
    Project(ProjectRequest),

    // ==========================================================================
    // Admin
    // ==========================================================================
    /// Ping for liveness
    Ping,
}

impl ToolRequest {
    /// Get the timing classification for this request
    pub fn timing(&self) -> ToolTiming {
        match self {
            // AsyncShort - fast operations, I/O bound, or ZMQ calls (~30s timeout)
            // Note: We don't use Sync because it creates footguns when tools are
            // misclassified. The async overhead is negligible for fast operations.
            Self::AbcParse(_) | Self::AbcValidate(_) | Self::AbcTranspose(_) => ToolTiming::AsyncShort,
            Self::OrpheusClassify(_) => ToolTiming::AsyncShort,
            Self::SoundfontInspect(_) | Self::SoundfontPresetInspect(_) => ToolTiming::AsyncShort,
            Self::GardenStatus | Self::GardenGetRegions(_) | Self::GardenQuery(_) => ToolTiming::AsyncShort,
            Self::JobStatus(_) | Self::JobList(_) => ToolTiming::AsyncShort,
            Self::ConfigGet(_) => ToolTiming::AsyncShort,
            Self::GraphFind(_) | Self::GraphContext(_) | Self::GraphQuery(_) => ToolTiming::AsyncShort,
            Self::ArtifactGet(_) | Self::ArtifactList(_) | Self::ArtifactCreate(_) => ToolTiming::AsyncShort,
            Self::CasInspect(_) => ToolTiming::AsyncShort,
            Self::Ping | Self::ListResources => ToolTiming::AsyncShort,
            Self::ReadResource(_) => ToolTiming::AsyncShort,
            Self::CasStore(_) | Self::CasGet(_) | Self::CasUploadFile(_) | Self::CasStats => ToolTiming::AsyncShort,
            Self::ArtifactUpload(_) => ToolTiming::AsyncShort,
            Self::AbcToMidi(_) => ToolTiming::AsyncShort,
            Self::GraphBind(_) | Self::GraphTag(_) | Self::GraphConnect(_) => ToolTiming::AsyncShort,
            Self::AddAnnotation(_) => ToolTiming::AsyncShort,
            Self::JobPoll(_) | Self::JobCancel(_) | Self::JobSleep(_) | Self::EventPoll(_) => ToolTiming::AsyncShort,
            Self::WeaveEval(_) | Self::WeaveSession | Self::WeaveReset(_) | Self::WeaveHelp(_) => ToolTiming::AsyncShort,
            Self::Complete(_) | Self::SampleLlm(_) => ToolTiming::AsyncShort,
            Self::Schedule(_) => ToolTiming::AsyncShort,
            Self::GardenAttachAudio(_) | Self::GardenDetachAudio | Self::GardenAudioStatus => ToolTiming::AsyncShort,
            Self::GardenAttachInput(_) | Self::GardenDetachInput | Self::GardenInputStatus => ToolTiming::AsyncShort,
            Self::GardenSetMonitor(_) => ToolTiming::AsyncShort,
            Self::GetToolHelp(_) => ToolTiming::AsyncShort,

            // AsyncMedium - GPU inference, ~120s
            Self::MidiToWav(_) => ToolTiming::AsyncMedium,
            Self::OrpheusGenerate(_)
            | Self::OrpheusGenerateSeeded(_)
            | Self::OrpheusContinue(_)
            | Self::OrpheusBridge(_)
            | Self::OrpheusLoops(_) => ToolTiming::AsyncMedium,
            Self::Analyze(_) => ToolTiming::AsyncMedium,
            Self::Sample(_) | Self::Extend(_) | Self::Bridge(_) => ToolTiming::AsyncMedium,
            Self::Project(_) => ToolTiming::AsyncMedium,

            // AsyncLong - long running, client manages
            Self::MusicgenGenerate(_) => ToolTiming::AsyncLong,
            Self::YueGenerate(_) => ToolTiming::AsyncLong,
            Self::ClapAnalyze(_) => ToolTiming::AsyncLong,
            Self::BeatthisAnalyze(_) => ToolTiming::AsyncLong,

            // FireAndForget - control commands
            Self::GardenPlay
            | Self::GardenPause
            | Self::GardenStop
            | Self::GardenSeek(_)
            | Self::GardenSetTempo(_)
            | Self::GardenEmergencyPause => ToolTiming::FireAndForget,
            Self::GardenCreateRegion(_)
            | Self::GardenDeleteRegion(_)
            | Self::GardenMoveRegion(_)
            | Self::GardenClearRegions => ToolTiming::FireAndForget,
        }
    }

    /// Get the tool name as a string
    pub fn name(&self) -> &'static str {
        match self {
            Self::CasStore(_) => "cas_store",
            Self::CasInspect(_) => "cas_inspect",
            Self::CasGet(_) => "cas_get",
            Self::CasUploadFile(_) => "cas_upload_file",
            Self::CasStats => "cas_stats",
            Self::ArtifactUpload(_) => "artifact_upload",
            Self::ArtifactGet(_) => "artifact_get",
            Self::ArtifactList(_) => "artifact_list",
            Self::ArtifactCreate(_) => "artifact_create",
            Self::OrpheusGenerate(_) => "orpheus_generate",
            Self::OrpheusGenerateSeeded(_) => "orpheus_generate_seeded",
            Self::OrpheusContinue(_) => "orpheus_continue",
            Self::OrpheusBridge(_) => "orpheus_bridge",
            Self::OrpheusLoops(_) => "orpheus_loops",
            Self::OrpheusClassify(_) => "orpheus_classify",
            Self::MidiToWav(_) => "convert_midi_to_wav",
            Self::SoundfontInspect(_) => "soundfont_inspect",
            Self::SoundfontPresetInspect(_) => "soundfont_preset_inspect",
            Self::MusicgenGenerate(_) => "musicgen_generate",
            Self::YueGenerate(_) => "yue_generate",
            Self::BeatthisAnalyze(_) => "beatthis_analyze",
            Self::ClapAnalyze(_) => "clap_analyze",
            Self::AbcParse(_) => "abc_parse",
            Self::AbcValidate(_) => "abc_validate",
            Self::AbcTranspose(_) => "abc_transpose",
            Self::AbcToMidi(_) => "abc_to_midi",
            Self::GardenStatus => "garden_status",
            Self::GardenPlay => "garden_play",
            Self::GardenPause => "garden_pause",
            Self::GardenStop => "garden_stop",
            Self::GardenSeek(_) => "garden_seek",
            Self::GardenSetTempo(_) => "garden_set_tempo",
            Self::GardenGetRegions(_) => "garden_get_regions",
            Self::GardenCreateRegion(_) => "garden_create_region",
            Self::GardenDeleteRegion(_) => "garden_delete_region",
            Self::GardenMoveRegion(_) => "garden_move_region",
            Self::GardenClearRegions => "garden_clear_regions",
            Self::GardenQuery(_) => "garden_query",
            Self::GardenEmergencyPause => "garden_emergency_pause",
            Self::GardenAttachAudio(_) => "garden_attach_audio",
            Self::GardenDetachAudio => "garden_detach_audio",
            Self::GardenAudioStatus => "garden_audio_status",
            Self::GardenAttachInput(_) => "garden_attach_input",
            Self::GardenDetachInput => "garden_detach_input",
            Self::GardenInputStatus => "garden_input_status",
            Self::GardenSetMonitor(_) => "garden_set_monitor",
            Self::GetToolHelp(_) => "get_tool_help",
            Self::JobStatus(_) => "job_status",
            Self::JobList(_) => "job_list",
            Self::JobPoll(_) => "job_poll",
            Self::JobCancel(_) => "job_cancel",
            Self::JobSleep(_) => "job_sleep",
            Self::EventPoll(_) => "event_poll",
            Self::GraphBind(_) => "graph_bind",
            Self::GraphTag(_) => "graph_tag",
            Self::GraphConnect(_) => "graph_connect",
            Self::GraphFind(_) => "graph_find",
            Self::GraphQuery(_) => "graph_query",
            Self::GraphContext(_) => "graph_context",
            Self::ConfigGet(_) => "config_get",
            Self::AddAnnotation(_) => "add_annotation",
            Self::WeaveEval(_) => "weave_eval",
            Self::WeaveSession => "weave_session",
            Self::WeaveReset(_) => "weave_reset",
            Self::WeaveHelp(_) => "weave_help",
            Self::ReadResource(_) => "read_resource",
            Self::ListResources => "list_resources",
            Self::Complete(_) => "complete",
            Self::SampleLlm(_) => "sample_llm",
            Self::Schedule(_) => "schedule",
            Self::Analyze(_) => "analyze",
            Self::Sample(_) => "sample",
            Self::Extend(_) => "extend",
            Self::Bridge(_) => "bridge",
            Self::Project(_) => "project",
            Self::Ping => "ping",
        }
    }
}

// =============================================================================
// CAS Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasStoreRequest {
    pub data: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasInspectRequest {
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasGetRequest {
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CasUploadFileRequest {
    pub file_path: String,
    pub mime_type: String,
}

// =============================================================================
// Artifact Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactUploadRequest {
    pub file_path: String,
    pub mime_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactGetRequest {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ArtifactListRequest {
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactCreateRequest {
    pub cas_hash: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub metadata: serde_json::Value,
}

// =============================================================================
// Orpheus Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrpheusGenerateRequest {
    pub max_tokens: Option<u32>,
    pub num_variations: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub model: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrpheusGenerateSeededRequest {
    pub seed_hash: String,
    pub max_tokens: Option<u32>,
    pub num_variations: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub model: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrpheusContinueRequest {
    pub input_hash: String,
    pub max_tokens: Option<u32>,
    pub num_variations: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub model: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrpheusBridgeRequest {
    pub section_a_hash: String,
    pub section_b_hash: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub model: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrpheusLoopsRequest {
    pub seed_hash: Option<String>,
    pub max_tokens: Option<u32>,
    pub num_variations: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrpheusClassifyRequest {
    pub midi_hash: String,
}

// =============================================================================
// Audio Conversion Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiToWavRequest {
    pub input_hash: String,
    pub soundfont_hash: String,
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundfontInspectRequest {
    pub soundfont_hash: String,
    #[serde(default)]
    pub include_drum_map: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoundfontPresetInspectRequest {
    pub soundfont_hash: String,
    pub bank: u16,
    pub program: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicgenGenerateRequest {
    pub prompt: Option<String>,
    pub duration: Option<f32>,
    pub temperature: Option<f32>,
    pub top_k: Option<u32>,
    pub top_p: Option<f32>,
    pub guidance_scale: Option<f32>,
    pub do_sample: Option<bool>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YueGenerateRequest {
    pub lyrics: String,
    pub genre: Option<String>,
    pub max_new_tokens: Option<u32>,
    pub run_n_segments: Option<u32>,
    pub seed: Option<u64>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

// =============================================================================
// Analysis Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatthisAnalyzeRequest {
    pub audio_hash: Option<String>,
    pub audio_path: Option<String>,
    #[serde(default)]
    pub include_frames: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClapAnalyzeRequest {
    pub audio_hash: String,
    pub audio_b_hash: Option<String>,
    #[serde(default = "default_clap_tasks")]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub text_candidates: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
}

fn default_clap_tasks() -> Vec<String> {
    vec!["embeddings".to_string()]
}

// =============================================================================
// ABC Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcParseRequest {
    pub abc: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcValidateRequest {
    pub abc: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcTransposeRequest {
    pub abc: String,
    pub semitones: Option<i8>,
    pub target_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbcToMidiRequest {
    pub abc: String,
    pub tempo_override: Option<u16>,
    pub transpose: Option<i8>,
    pub velocity: Option<u8>,
    pub channel: Option<u8>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub creator: Option<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
}

// =============================================================================
// Garden Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenSeekRequest {
    pub beat: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenSetTempoRequest {
    pub bpm: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GardenGetRegionsRequest {
    pub start: Option<f64>,
    pub end: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenCreateRegionRequest {
    pub position: f64,
    pub duration: f64,
    pub behavior_type: String,
    pub content_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenDeleteRegionRequest {
    pub region_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenMoveRegionRequest {
    pub region_id: String,
    pub new_position: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GardenQueryRequest {
    pub query: String,
    pub variables: Option<serde_json::Value>,
}

// =============================================================================
// Job Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobStatusRequest {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct JobListRequest {
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobPollRequest {
    #[serde(default)]
    pub job_ids: Vec<String>,
    pub timeout_ms: u64,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobCancelRequest {
    pub job_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSleepRequest {
    pub milliseconds: u64,
}

// =============================================================================
// Event Polling Request Types
// =============================================================================

/// Request to poll for buffered broadcast events
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EventPollRequest {
    /// Cursor from previous poll (None = get recent events)
    pub cursor: Option<u64>,
    /// Get events from the last N milliseconds (alternative to cursor)
    pub since_ms: Option<u64>,
    /// Event types to filter (None = all types)
    pub types: Option<Vec<String>>,
    /// How long to wait for events (ms). Default: 5000, max: 30000
    pub timeout_ms: Option<u64>,
    /// Max events to return. Default: 100, max: 1000
    pub limit: Option<usize>,
}

// =============================================================================
// Graph Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphBindRequest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub hints: Vec<GraphHint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphHint {
    pub kind: String,
    pub value: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTagRequest {
    pub identity_id: String,
    pub namespace: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphConnectRequest {
    pub from_identity: String,
    pub from_port: String,
    pub to_identity: String,
    pub to_port: String,
    pub transport: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GraphFindRequest {
    pub name: Option<String>,
    pub tag_namespace: Option<String>,
    pub tag_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphQueryRequest {
    pub query: String,
    pub limit: Option<usize>,
    pub variables: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphContextRequest {
    pub limit: Option<usize>,
    pub tag: Option<String>,
    pub creator: Option<String>,
    pub vibe_search: Option<String>,
    pub within_minutes: Option<i64>,
    #[serde(default = "default_true")]
    pub include_annotations: bool,
    #[serde(default)]
    pub include_metadata: bool,
}

fn default_true() -> bool {
    true
}

// =============================================================================
// Config Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ConfigGetRequest {
    pub section: Option<String>,
    pub key: Option<String>,
}

// =============================================================================
// Annotation Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddAnnotationRequest {
    pub artifact_id: String,
    pub message: String,
    pub source: Option<String>,
    pub vibe: Option<String>,
}

// =============================================================================
// Vibeweaver Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaveEvalRequest {
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeaveResetRequest {
    pub clear_session: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WeaveHelpRequest {
    pub topic: Option<String>,
}

// =============================================================================
// Resource Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadResourceRequest {
    pub uri: String,
}

// =============================================================================
// Completion Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompleteRequest {
    pub context: String,
    pub partial: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleLlmRequest {
    pub prompt: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub system_prompt: Option<String>,
}

// =============================================================================
// Model-Native Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleRequest {
    pub encoding: crate::Encoding,
    pub at: f64,
    pub duration: Option<f64>,
    pub gain: Option<f64>,
    pub rate: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeRequest {
    pub encoding: crate::Encoding,
    pub tasks: Vec<crate::AnalysisTask>,
}

fn default_one() -> Option<u32> {
    Some(1)
}

fn default_creator() -> Option<String> {
    Some("unknown".to_string())
}

/// Request to sample from a generative space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleRequest {
    /// Generative space to sample from
    pub space: crate::Space,

    /// Inference parameters
    #[serde(default)]
    pub inference: crate::InferenceContext,

    /// Number of variations to generate (default: 1)
    #[serde(default = "default_one")]
    pub num_variations: Option<u32>,

    /// Text prompt (for prompted spaces like musicgen, yue)
    pub prompt: Option<String>,

    /// Seed encoding to condition on
    pub seed: Option<crate::Encoding>,

    /// Generate as loopable pattern (orpheus only)
    #[serde(default)]
    pub as_loop: bool,

    /// Variation set ID for grouping
    pub variation_set_id: Option<String>,

    /// Parent artifact ID for refinements
    pub parent_id: Option<String>,

    /// Tags for organizing
    #[serde(default)]
    pub tags: Vec<String>,

    /// Creator identifier
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

/// Request to extend existing content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtendRequest {
    /// Content to continue from
    pub encoding: crate::Encoding,

    /// Space to use (inferred from encoding if omitted)
    pub space: Option<crate::Space>,

    /// Inference parameters
    #[serde(default)]
    pub inference: crate::InferenceContext,

    /// Number of variations to generate (default: 1)
    #[serde(default = "default_one")]
    pub num_variations: Option<u32>,

    /// Variation set ID for grouping
    pub variation_set_id: Option<String>,

    /// Parent artifact ID for refinements
    pub parent_id: Option<String>,

    /// Tags for organizing
    #[serde(default)]
    pub tags: Vec<String>,

    /// Creator identifier
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

/// Request to create a bridge transition between sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeRequest {
    /// Starting content (section A)
    pub from: crate::Encoding,

    /// Target content (section B) - optional for A->B bridging
    pub to: Option<crate::Encoding>,

    /// Inference parameters
    #[serde(default)]
    pub inference: crate::InferenceContext,

    /// Variation set ID for grouping
    pub variation_set_id: Option<String>,

    /// Parent artifact ID for refinements
    pub parent_id: Option<String>,

    /// Tags for organizing
    #[serde(default)]
    pub tags: Vec<String>,

    /// Creator identifier
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

/// Request to project content to a different format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectRequest {
    /// Source content to project
    pub encoding: crate::Encoding,

    /// Target format/space
    pub target: crate::ProjectionTarget,

    /// Variation set ID for grouping
    pub variation_set_id: Option<String>,

    /// Parent artifact ID
    pub parent_id: Option<String>,

    /// Tags for organizing
    #[serde(default)]
    pub tags: Vec<String>,

    /// Creator identifier
    #[serde(default = "default_creator")]
    pub creator: Option<String>,
}

// =============================================================================
// Garden Audio Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GardenAttachAudioRequest {
    pub device_name: Option<String>,
    pub sample_rate: Option<u32>,
    pub latency_frames: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GardenAttachInputRequest {
    pub device_name: Option<String>,
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GardenSetMonitorRequest {
    pub enabled: Option<bool>,
    pub gain: Option<f32>,
}

// =============================================================================
// Tool Help Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GetToolHelpRequest {
    pub topic: Option<String>,
}
