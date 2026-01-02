@0xc4f8e2a1b3d5f0e7;

using Common = import "common.capnp";

# Unified response type for all tools
struct ToolResponse {
  union {
    # CAS Operations
    casStored @0 :CasStoredResponse;
    casContent @1 :CasContentResponse;
    casInspected @2 :CasInspectedResponse;

    # Artifacts
    artifactCreated @3 :ArtifactCreatedResponse;
    artifactInfo @4 :ArtifactInfoResponse;
    artifactList @5 :ArtifactListResponse;

    # Jobs
    jobStarted @6 :JobStartedResponse;
    jobStatus @7 :JobStatusResponse;
    jobList @8 :JobListResponse;
    jobPollResult @9 :JobPollResultResponse;

    # ABC Notation
    abcParsed @10 :AbcParsedResponse;
    abcValidated @11 :AbcValidatedResponse;
    abcTransposed @12 :AbcTransposedResponse;
    abcConverted @13 :AbcConvertedResponse;

    # SoundFont
    soundfontInfo @14 :SoundfontInfoResponse;
    soundfontPresetInfo @15 :SoundfontPresetInfoResponse;

    # Orpheus MIDI Generation
    orpheusGenerated @16 :OrpheusGeneratedResponse;
    orpheusClassified @17 :OrpheusClassifiedResponse;

    # Audio Generation
    audioGenerated @18 :AudioGeneratedResponse;

    # Audio Analysis
    beatsAnalyzed @19 :BeatsAnalyzedResponse;
    clapAnalyzed @20 :ClapAnalyzedResponse;

    # Garden/Transport
    gardenStatus @21 :GardenStatusResponse;
    gardenRegions @22 :GardenRegionsResponse;
    gardenRegionCreated @23 :GardenRegionCreatedResponse;
    gardenQueryResult @24 :GardenQueryResultResponse;

    # Graph
    graphIdentity @25 :GraphIdentityResponse;
    graphIdentities @26 :GraphIdentitiesResponse;
    graphConnection @27 :GraphConnectionResponse;
    graphTags @28 :GraphTagsResponse;
    graphContext @29 :GraphContextResponse;
    graphQueryResult @30 :GraphQueryResultResponse;

    # Config
    configValue @31 :ConfigValueResponse;

    # Admin
    toolsList @32 :ToolsListResponse;

    # Simple Acknowledgements
    ack @33 :AckResponse;

    # Annotations
    annotationAdded @34 :AnnotationAddedResponse;

    # Vibeweaver (Python kernel)
    weaveEval @35 :WeaveEvalResponse;
    weaveSession @36 :WeaveSessionResponse;
    weaveReset @37 :WeaveResetResponse;
    weaveHelp @38 :WeaveHelpResponse;

    # Audio Device Status
    gardenAudioStatus @39 :GardenAudioStatusResponse;
    gardenInputStatus @40 :GardenInputStatusResponse;
    gardenMonitorStatus @41 :GardenMonitorStatusResponse;

    # Tool Help
    toolHelp @42 :ToolHelpResponse;

    # Schedule Result
    scheduleResult @43 :ScheduleResultResponse;

    # Analysis Result
    analyzeResult @44 :AnalyzeResultResponse;

    # CAS Stats
    casStats @45 :CasStatsResponse;

    # Project Result
    projectResult @46 :ProjectResultResponse;

    # Graph Results
    graphBind @47 :GraphBindResponse;
    graphTag @48 :GraphTagResponse;
    graphConnect @49 :GraphConnectResponse;

    # Job Extended (with full details)
    jobPoll @50 :JobPollResponse;
    jobCancel @51 :JobCancelResponse;
    jobSleep @52 :JobSleepResponse;

    # Audio Conversion
    abcToMidi @53 :AbcToMidiResponse;
    midiToWav @54 :MidiToWavResponse;
  }
}

# =============================================================================
# CAS Responses
# =============================================================================

struct CasStoredResponse {
  hash @0 :Text;
  size @1 :UInt64;
  mimeType @2 :Text;
}

struct CasContentResponse {
  hash @0 :Text;
  size @1 :UInt64;
  data @2 :Data;
}

struct CasInspectedResponse {
  hash @0 :Text;
  exists @1 :Bool;
  size @2 :UInt64;      # 0 if not exists
  preview @3 :Text;     # empty if not exists
}

struct CasStatsResponse {
  totalItems @0 :UInt64;
  totalBytes @1 :UInt64;
  casDir @2 :Text;
}

# =============================================================================
# Artifact Responses
# =============================================================================

struct ArtifactCreatedResponse {
  artifactId @0 :Text;
  contentHash @1 :Text;
  tags @2 :List(Text);
  creator @3 :Text;
}

struct ArtifactInfoResponse {
  id @0 :Text;
  contentHash @1 :Text;
  mimeType @2 :Text;
  tags @3 :List(Text);
  creator @4 :Text;
  createdAt @5 :UInt64;
  parentId @6 :Text;          # empty if none
  variationSetId @7 :Text;    # empty if none
  metadata @8 :ArtifactMetadata;
}

struct ArtifactMetadata {
  durationSeconds @0 :Float64;
  sampleRate @1 :UInt32;
  channels @2 :UInt8;
  midiInfo @3 :MidiMetadata;
}

struct MidiMetadata {
  tracks @0 :UInt16;
  ticksPerQuarter @1 :UInt16;
  durationTicks @2 :UInt64;
}

struct ArtifactListResponse {
  artifacts @0 :List(ArtifactInfoResponse);
  count @1 :UInt64;
}

# =============================================================================
# Job Responses
# =============================================================================

struct JobStartedResponse {
  jobId @0 :Text;
  tool @1 :Text;
}

enum JobState {
  pending @0;
  running @1;
  complete @2;
  failed @3;
  cancelled @4;
}

struct JobStatusResponse {
  jobId @0 :Text;
  status @1 :JobState;
  source @2 :Text;
  result @3 :ToolResponse;    # Only set if complete
  error @4 :Text;             # Only set if failed
  createdAt @5 :UInt64;
  startedAt @6 :UInt64;       # 0 if not started
  completedAt @7 :UInt64;     # 0 if not completed
}

struct JobListResponse {
  jobs @0 :List(JobStatusResponse);
  total @1 :UInt64;
  byStatus @2 :JobCounts;
}

struct JobCounts {
  pending @0 :UInt64;
  running @1 :UInt64;
  complete @2 :UInt64;
  failed @3 :UInt64;
  cancelled @4 :UInt64;
}

struct JobPollResultResponse {
  completed @0 :List(Text);
  failed @1 :List(Text);
  pending @2 :List(Text);
  timedOut @3 :Bool;
}

struct JobPollResponse {
  completed @0 :List(Text);
  failed @1 :List(Text);
  pending @2 :List(Text);
  reason @3 :Text;
  elapsedMs @4 :UInt64;
}

struct JobCancelResponse {
  jobId @0 :Text;
  cancelled @1 :Bool;
}

struct JobSleepResponse {
  sleptMs @0 :UInt64;
}

# =============================================================================
# ABC Notation Responses
# =============================================================================

struct AbcParsedResponse {
  valid @0 :Bool;
  title @1 :Text;
  key @2 :Text;
  meter @3 :Text;
  tempo @4 :UInt16;
  notesCount @5 :UInt64;
}

struct AbcValidatedResponse {
  valid @0 :Bool;
  errors @1 :List(AbcValidationError);
  warnings @2 :List(Text);
}

struct AbcValidationError {
  line @0 :UInt64;
  column @1 :UInt64;
  message @2 :Text;
}

struct AbcTransposedResponse {
  abc @0 :Text;
  originalKey @1 :Text;
  newKey @2 :Text;
  semitones @3 :Int8;
}

struct AbcConvertedResponse {
  artifactId @0 :Text;
  contentHash @1 :Text;
  durationSeconds @2 :Float64;
  notesCount @3 :UInt64;
}

struct AbcToMidiResponse {
  artifactId @0 :Text;
  contentHash @1 :Text;
}

# =============================================================================
# Audio Conversion Responses
# =============================================================================

struct MidiToWavResponse {
  artifactId @0 :Text;
  contentHash @1 :Text;
  sampleRate @2 :UInt32;
  durationSecs @3 :Float64;  # 0.0 if not known
}

# =============================================================================
# SoundFont Responses
# =============================================================================

struct SoundfontInfoResponse {
  name @0 :Text;
  presets @1 :List(SoundfontPreset);
  presetCount @2 :UInt64;
}

struct SoundfontPreset {
  bank @0 :UInt16;
  program @1 :UInt16;
  name @2 :Text;
}

struct SoundfontPresetInfoResponse {
  bank @0 :UInt16;
  program @1 :UInt16;
  name @2 :Text;
  regions @3 :List(SoundfontRegion);
}

struct SoundfontRegion {
  keyLow @0 :UInt8;
  keyHigh @1 :UInt8;
  velocityLow @2 :UInt8;
  velocityHigh @3 :UInt8;
  sampleName @4 :Text;
}

# =============================================================================
# Orpheus Responses
# =============================================================================

struct OrpheusGeneratedResponse {
  outputHashes @0 :List(Text);
  artifactIds @1 :List(Text);
  tokensPerVariation @2 :List(UInt64);
  totalTokens @3 :UInt64;
  variationSetId @4 :Text;    # empty if none
  summary @5 :Text;
}

struct OrpheusClassifiedResponse {
  classifications @0 :List(MidiClassification);
}

struct MidiClassification {
  label @0 :Text;
  confidence @1 :Float32;
}

# =============================================================================
# Audio Generation Responses
# =============================================================================

struct AudioGeneratedResponse {
  artifactId @0 :Text;
  contentHash @1 :Text;
  durationSeconds @2 :Float64;
  sampleRate @3 :UInt32;
  format @4 :AudioFormat;
  genre @5 :Text;             # empty if none
}

enum AudioFormat {
  wav @0;
  mp3 @1;
  flac @2;
}

# =============================================================================
# Audio Analysis Responses
# =============================================================================

struct BeatsAnalyzedResponse {
  beats @0 :List(Float64);
  downbeats @1 :List(Float64);
  estimatedBpm @2 :Float64;
  confidence @3 :Float32;
}

struct ClapAnalyzedResponse {
  embeddings @0 :List(Float32);
  genre @1 :List(ClapClassification);
  mood @2 :List(ClapClassification);
  zeroShot @3 :List(ClapClassification);
  similarity @4 :Float32;
}

struct ClapClassification {
  label @0 :Text;
  score @1 :Float32;
}

# =============================================================================
# Garden/Transport Responses
# =============================================================================

enum TransportState {
  stopped @0;
  playing @1;
  paused @2;
}

struct GardenStatusResponse {
  state @0 :TransportState;
  positionBeats @1 :Float64;
  tempoBpm @2 :Float64;
  regionCount @3 :UInt64;
}

struct GardenRegionInfo {
  regionId @0 :Text;
  position @1 :Float64;
  duration @2 :Float64;
  behaviorType @3 :Text;
  contentId @4 :Text;
}

struct GardenRegionsResponse {
  regions @0 :List(GardenRegionInfo);
  count @1 :UInt64;
}

struct GardenRegionCreatedResponse {
  regionId @0 :Text;
  position @1 :Float64;
  duration @2 :Float64;
}

struct GardenQueryResultResponse {
  results @0 :Text;           # JSON array - Trustfall results are dynamic
  count @1 :UInt64;
}

struct GardenAudioStatusResponse {
  attached @0 :Bool;
  deviceName @1 :Text;
  sampleRate @2 :UInt32;
  latencyFrames @3 :UInt32;
  bufferUnderruns @4 :UInt64;
  callbacks @5 :UInt64;
  samplesWritten @6 :UInt64;
}

struct GardenInputStatusResponse {
  attached @0 :Bool;
  deviceName @1 :Text;
  sampleRate @2 :UInt32;
}

struct GardenMonitorStatusResponse {
  enabled @0 :Bool;
  gain @1 :Float64;
}

# =============================================================================
# Graph Responses
# =============================================================================

struct GraphIdentityResponse {
  id @0 :Text;
  name @1 :Text;
  createdAt @2 :UInt64;
}

struct GraphIdentityInfo {
  id @0 :Text;
  name @1 :Text;
  tags @2 :List(Text);
}

struct GraphIdentitiesResponse {
  identities @0 :List(GraphIdentityInfo);
  count @1 :UInt64;
}

struct GraphConnectionResponse {
  connectionId @0 :Text;
  fromIdentity @1 :Text;
  fromPort @2 :Text;
  toIdentity @3 :Text;
  toPort @4 :Text;
  transport @5 :Text;         # empty if none
}

struct GraphTagInfo {
  namespace @0 :Text;
  value @1 :Text;
}

struct GraphTagsResponse {
  identityId @0 :Text;
  tags @1 :List(GraphTagInfo);
}

struct GraphContextResponse {
  context @0 :Text;
  artifactCount @1 :UInt64;
  identityCount @2 :UInt64;
}

struct GraphQueryResultResponse {
  results @0 :Text;           # JSON array - Trustfall results are dynamic
  count @1 :UInt64;
}

# =============================================================================
# Config Responses
# =============================================================================

struct ConfigValueResponse {
  section @0 :Text;
  key @1 :Text;
  value @2 :Text;             # JSON-encoded value (preserves nested structure)
}

# =============================================================================
# Admin Responses
# =============================================================================

struct ToolInfo {
  name @0 :Text;
  description @1 :Text;
  inputSchema @2 :Text;       # JSON Schema
}

struct ToolsListResponse {
  tools @0 :List(ToolInfo);
  count @1 :UInt64;
}

# =============================================================================
# Simple Responses
# =============================================================================

struct AckResponse {
  message @0 :Text;
}

struct AnnotationAddedResponse {
  artifactId @0 :Text;
  annotationId @1 :Text;
}

# =============================================================================
# Vibeweaver Responses
# =============================================================================

enum WeaveOutputType {
  expression @0;
  statement @1;
}

struct WeaveEvalResponse {
  outputType @0 :WeaveOutputType;
  result @1 :Text;            # empty if statement
  stdout @2 :Text;            # empty if expression
  stderr @3 :Text;
}

struct WeaveSessionInfo {
  id @0 :Text;
  name @1 :Text;
  vibe @2 :Text;              # empty if none
}

struct WeaveSessionResponse {
  session @0 :WeaveSessionInfo;
  message @1 :Text;
}

struct WeaveResetResponse {
  reset @0 :Bool;
  message @1 :Text;
}

struct WeaveHelpResponse {
  help @0 :Text;
  topic @1 :Text;
}

# =============================================================================
# Tool Help Response
# =============================================================================

struct ToolHelpResponse {
  help @0 :Text;
  topic @1 :Text;
}

# =============================================================================
# Schedule Response
# =============================================================================

struct ScheduleResultResponse {
  success @0 :Bool;
  message @1 :Text;
  regionId @2 :Text;
  position @3 :Float64;
  duration @4 :Float64;
  artifactId @5 :Text;
}

# =============================================================================
# Analyze Response
# =============================================================================

struct AnalyzeResultResponse {
  contentHash @0 :Text;
  results @1 :Text;           # JSON - analysis results vary by task
  summary @2 :Text;
  artifactId @3 :Text;        # Empty if not stored
}

# =============================================================================
# Project Response
# =============================================================================

struct ProjectResultResponse {
  artifactId @0 :Text;
  contentHash @1 :Text;
  projectionType @2 :Text;
  durationSeconds @3 :Float64;  # 0.0 if not audio
  sampleRate @4 :UInt32;        # 0 if not audio
}

# =============================================================================
# Graph Responses
# =============================================================================

struct GraphBindResponse {
  identityId @0 :Text;
  name @1 :Text;
  hintsCount @2 :UInt32;
}

struct GraphTagResponse {
  identityId @0 :Text;
  tag @1 :Text;
}

struct GraphConnectResponse {
  fromIdentity @0 :Text;
  fromPort @1 :Text;
  toIdentity @2 :Text;
  toPort @3 :Text;
}
