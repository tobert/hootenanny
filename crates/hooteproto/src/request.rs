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
    /// Extract MIDI file metadata (tempo, time signature, duration)
    MidiInfo(MidiInfoRequest),

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
    /// Get audio snapshot from streaming tap
    GardenGetAudioSnapshot(GardenGetAudioSnapshotRequest),
    /// Capture audio from monitor input to CAS
    AudioCapture(AudioCaptureRequest),

    // ==========================================================================
    // MIDI I/O (direct ALSA for low latency)
    // ==========================================================================
    /// List available MIDI ports
    MidiListPorts,
    /// Attach MIDI input
    MidiInputAttach(MidiAttachRequest),
    /// Detach MIDI input
    MidiInputDetach(MidiDetachRequest),
    /// Attach MIDI output
    MidiOutputAttach(MidiAttachRequest),
    /// Detach MIDI output
    MidiOutputDetach(MidiDetachRequest),
    /// Send MIDI message
    MidiSend(MidiSendRequest),
    /// Get MIDI status
    MidiStatus,

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
    // Admin
    // ==========================================================================
    /// Ping for liveness
    Ping,

    // ==========================================================================
    // RAVE Audio Codec
    // ==========================================================================
    /// Encode audio to latent codes
    RaveEncode(RaveEncodeRequest),
    /// Decode latent codes to audio
    RaveDecode(RaveDecodeRequest),
    /// Reconstruct audio (encode then decode)
    RaveReconstruct(RaveReconstructRequest),
    /// Generate audio by sampling from prior
    RaveGenerate(RaveGenerateRequest),
    /// Start a streaming session
    RaveStreamStart(RaveStreamStartRequest),
    /// Stop a streaming session
    RaveStreamStop(RaveStreamStopRequest),
    /// Get streaming session status
    RaveStreamStatus(RaveStreamStatusRequest),
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
            Self::MidiInfo(_) => ToolTiming::AsyncShort,
            Self::Ping | Self::ListResources => ToolTiming::AsyncShort,
            Self::ReadResource(_) => ToolTiming::AsyncShort,
            Self::CasStore(_) | Self::CasGet(_) | Self::CasUploadFile(_) | Self::CasStats => ToolTiming::AsyncShort,
            Self::ArtifactUpload(_) => ToolTiming::AsyncShort,
            Self::AbcToMidi(_) => ToolTiming::AsyncShort,
            Self::GraphBind(_) | Self::GraphTag(_) | Self::GraphConnect(_) => ToolTiming::AsyncShort,
            Self::AddAnnotation(_) => ToolTiming::AsyncShort,
            Self::JobPoll(_) | Self::JobCancel(_) | Self::EventPoll(_) => ToolTiming::AsyncShort,
            Self::WeaveEval(_) | Self::WeaveSession | Self::WeaveReset(_) | Self::WeaveHelp(_) => ToolTiming::AsyncShort,
            Self::Complete(_) | Self::SampleLlm(_) => ToolTiming::AsyncShort,
            Self::GardenAttachAudio(_) | Self::GardenDetachAudio | Self::GardenAudioStatus => ToolTiming::AsyncShort,
            Self::GardenAttachInput(_) | Self::GardenDetachInput | Self::GardenInputStatus => ToolTiming::AsyncShort,
            Self::GardenSetMonitor(_) | Self::GardenGetAudioSnapshot(_) => ToolTiming::AsyncShort,
            Self::MidiListPorts | Self::MidiStatus => ToolTiming::AsyncShort,
            Self::MidiInputAttach(_) | Self::MidiInputDetach(_) => ToolTiming::AsyncShort,
            Self::MidiOutputAttach(_) | Self::MidiOutputDetach(_) => ToolTiming::AsyncShort,
            Self::MidiSend(_) => ToolTiming::AsyncShort,
            Self::AudioCapture(_) => ToolTiming::AsyncShort,
            Self::GetToolHelp(_) => ToolTiming::AsyncShort,

            // AsyncMedium - GPU inference, ~120s
            Self::MidiToWav(_) => ToolTiming::AsyncMedium,
            Self::OrpheusGenerate(_)
            | Self::OrpheusGenerateSeeded(_)
            | Self::OrpheusContinue(_)
            | Self::OrpheusBridge(_)
            | Self::OrpheusLoops(_) => ToolTiming::AsyncMedium,

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

            // RAVE - AsyncMedium for batch, FireAndForget for stream control
            Self::RaveEncode(_)
            | Self::RaveDecode(_)
            | Self::RaveReconstruct(_)
            | Self::RaveGenerate(_) => ToolTiming::AsyncMedium,
            Self::RaveStreamStart(_) => ToolTiming::AsyncShort,
            Self::RaveStreamStop(_) | Self::RaveStreamStatus(_) => ToolTiming::AsyncShort,
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
            Self::MidiInfo(_) => "midi_info",
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
            Self::GardenGetAudioSnapshot(_) => "garden_get_audio_snapshot",
            Self::MidiListPorts => "midi_list_ports",
            Self::MidiInputAttach(_) => "midi_input_attach",
            Self::MidiInputDetach(_) => "midi_input_detach",
            Self::MidiOutputAttach(_) => "midi_output_attach",
            Self::MidiOutputDetach(_) => "midi_output_detach",
            Self::MidiSend(_) => "midi_send",
            Self::MidiStatus => "midi_status",
            Self::GetToolHelp(_) => "get_tool_help",
            Self::JobStatus(_) => "job_status",
            Self::JobList(_) => "job_list",
            Self::JobPoll(_) => "job_poll",
            Self::JobCancel(_) => "job_cancel",
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
            Self::Ping => "ping",
            Self::RaveEncode(_) => "rave_encode",
            Self::RaveDecode(_) => "rave_decode",
            Self::RaveReconstruct(_) => "rave_reconstruct",
            Self::RaveGenerate(_) => "rave_generate",
            Self::RaveStreamStart(_) => "rave_stream_start",
            Self::RaveStreamStop(_) => "rave_stream_stop",
            Self::RaveStreamStatus(_) => "rave_stream_status",
            Self::AudioCapture(_) => "audio_capture",
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiInfoRequest {
    /// Artifact ID or CAS hash of the MIDI file
    pub artifact_id: Option<String>,
    /// Direct CAS hash (alternative to artifact_id)
    pub hash: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GardenGetAudioSnapshotRequest {
    pub frames: u32,
}

/// Capture audio from monitor input and save to CAS
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AudioCaptureRequest {
    /// Duration to capture in seconds
    pub duration_seconds: f32,
    /// Source to capture from: "monitor" (default), "timeline", "mix"
    pub source: Option<String>,
    /// Tags for the resulting artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

// =============================================================================
// MIDI I/O Request Types
// =============================================================================

/// Request to attach a MIDI port (input or output)
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MidiAttachRequest {
    /// Port name pattern to match (e.g., "NiftyCASE", "BRAINS")
    pub port_pattern: String,
}

/// Request to detach a MIDI port
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MidiDetachRequest {
    /// Port name pattern to match
    pub port_pattern: String,
}

/// Request to send a MIDI message
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MidiSendRequest {
    /// Target port pattern (None = all outputs)
    pub port_pattern: Option<String>,
    /// MIDI message to send
    pub message: MidiMessageSpec,
}

/// MIDI message specification (matches garden::MidiMessageSpec)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MidiMessageSpec {
    NoteOff {
        #[serde(default)]
        channel: u8,
        #[serde(default)]
        pitch: u8,
    },
    NoteOn {
        #[serde(default)]
        channel: u8,
        #[serde(default)]
        pitch: u8,
        #[serde(default)]
        velocity: u8,
    },
    ControlChange {
        #[serde(default)]
        channel: u8,
        #[serde(default)]
        controller: u8,
        #[serde(default)]
        value: u8,
    },
    ProgramChange {
        #[serde(default)]
        channel: u8,
        #[serde(default)]
        program: u8,
    },
    PitchBend {
        #[serde(default)]
        channel: u8,
        #[serde(default)]
        value: i16,
    },
    Raw {
        #[serde(default)]
        bytes: Vec<u8>,
    },
}

impl Default for MidiMessageSpec {
    fn default() -> Self {
        Self::NoteOff { channel: 0, pitch: 0 }
    }
}

// =============================================================================
// Tool Help Request Types
// =============================================================================

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GetToolHelpRequest {
    pub topic: Option<String>,
}

// =============================================================================
// RAVE Request Types
// =============================================================================

/// Encode audio waveform to RAVE latent codes
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RaveEncodeRequest {
    /// CAS hash of input audio (WAV format)
    pub audio_hash: String,
    /// Model name (e.g., "vintage", "percussion", "darbouka")
    pub model: Option<String>,
    /// Tags for the resulting artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

/// Decode RAVE latent codes to audio waveform
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RaveDecodeRequest {
    /// CAS hash of latent codes
    pub latent_hash: String,
    /// Shape of latent tensor (for unpacking)
    #[serde(default)]
    pub latent_shape: Vec<u32>,
    /// Model name
    pub model: Option<String>,
    /// Tags for the resulting artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

/// Reconstruct audio through RAVE (encode then decode)
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RaveReconstructRequest {
    /// CAS hash of input audio
    pub audio_hash: String,
    /// Model name
    pub model: Option<String>,
    /// Tags for the resulting artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

/// Generate audio by sampling from RAVE prior
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RaveGenerateRequest {
    /// Model name
    pub model: Option<String>,
    /// Duration in seconds
    pub duration_seconds: Option<f32>,
    /// Sampling temperature (default 1.0)
    pub temperature: Option<f32>,
    /// Tags for the resulting artifact
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creator identifier
    pub creator: Option<String>,
}

/// Start a RAVE streaming session
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RaveStreamStartRequest {
    /// Model name
    pub model: Option<String>,
    /// Graph identity for audio input source
    pub input_identity: String,
    /// Graph identity for audio output sink
    pub output_identity: String,
    /// Buffer size in samples (default 2048)
    pub buffer_size: Option<u32>,
}

/// Stop a RAVE streaming session
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RaveStreamStopRequest {
    /// ID of the streaming session to stop
    pub stream_id: String,
}

/// Get status of a RAVE streaming session
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RaveStreamStatusRequest {
    /// ID of the streaming session
    pub stream_id: String,
}
