@0xf3d2f032f8bfe15a;

using Common = import "common.capnp";
using Jobs = import "jobs.capnp";
using Garden = import "garden.capnp";

# All tool requests in a single union for typed dispatch
struct ToolRequest {
  union {
    # === CAS Tools ===
    casStore @0 :CasStore;
    casInspect @1 :CasInspect;
    casGet @2 :CasGet;
    casUploadFile @3 :CasUploadFile;
    casStats @71 :Void;

    # === Orpheus Tools ===
    orpheusGenerate @4 :OrpheusGenerate;
    orpheusGenerateSeeded @5 :OrpheusGenerateSeeded;
    orpheusContinue @6 :OrpheusContinue;
    orpheusBridge @7 :OrpheusBridge;
    orpheusLoops @8 :OrpheusLoops;
    orpheusClassify @9 :OrpheusClassify;

    # === ABC Notation Tools ===
    abcParse @10 :AbcParse;
    abcToMidi @11 :AbcToMidi;
    abcValidate @12 :AbcValidate;
    abcTranspose @13 :AbcTranspose;

    # === MIDI/Audio Tools ===
    convertMidiToWav @14 :ConvertMidiToWav;
    soundfontInspect @15 :SoundfontInspect;
    soundfontPresetInspect @16 :SoundfontPresetInspect;

    # === Analysis Tools ===
    beatthisAnalyze @17 :BeatthisAnalyze;
    clapAnalyze @18 :ClapAnalyze;
    midiInfo @72 :MidiInfo;
    audioInfo @89 :AudioInfo;

    # === Generation Tools ===
    musicgenGenerate @19 :MusicgenGenerate;
    yueGenerate @20 :YueGenerate;

    # === Artifact Tools ===
    artifactUpload @21 :ArtifactUpload;
    artifactGet @22 :ArtifactGet;
    artifactList @23 :ArtifactList;
    artifactCreate @24 :ArtifactCreate;

    # === Graph Tools ===
    graphQuery @25 :GraphQuery;
    graphBind @26 :GraphBind;
    graphTag @27 :GraphTag;
    graphConnect @28 :GraphConnect;
    graphFind @29 :GraphFind;
    graphContext @30 :GraphContext;
    addAnnotation @31 :AddAnnotation;

    # === Config Tools ===
    configGet @32 :ConfigGet;

    # === Lua Tools (Deprecated) ===
    luaEval @33 :LuaEval;
    luaDescribe @34 :LuaDescribe;
    scriptStore @35 :ScriptStore;
    scriptSearch @36 :ScriptSearch;

    # === Job Tools ===
    jobExecute @37 :Jobs.JobExecute;
    jobStatus @38 :Jobs.JobStatusRequest;
    jobPoll @39 :Jobs.JobPoll;
    jobCancel @40 :Jobs.JobCancel;
    jobList @41 :Jobs.JobList;

    # === Resource Tools ===
    readResource @42 :ReadResource;
    listResources @43 :Void;

    # === Completion Tools ===
    complete @44 :Complete;

    # === Misc Tools ===
    sampleLlm @45 :SampleLlm;

    # === Vibeweaver Tools ===
    weaveEval @46 :WeaveEval;
    weaveSession @47 :Void;
    weaveReset @48 :WeaveReset;
    weaveHelp @49 :WeaveHelp;

    # === Garden Tools ===
    gardenStatus @50 :Void;
    gardenPlay @51 :Void;
    gardenPause @52 :Void;
    gardenStop @53 :Void;
    gardenSeek @54 :Garden.Seek;
    gardenSetTempo @55 :Garden.SetTempo;
    gardenQuery @56 :Garden.Query;
    gardenEmergencyPause @57 :Void;
    gardenCreateRegion @58 :Garden.CreateRegion;
    gardenDeleteRegion @59 :Garden.DeleteRegion;
    gardenMoveRegion @60 :Garden.MoveRegion;
    gardenGetRegions @61 :Garden.GetRegions;
    gardenAttachAudio @62 :Garden.AttachAudio;
    gardenDetachAudio @63 :Void;
    gardenAudioStatus @64 :Void;
    gardenAttachInput @65 :Garden.AttachInput;
    gardenDetachInput @66 :Void;
    gardenInputStatus @67 :Void;
    gardenSetMonitor @68 :Garden.SetMonitor;
    gardenGetAudioSnapshot @80 :Garden.GetAudioSnapshot;
    audioCapture @81 :AudioCapture;
    gardenClearRegions @69 :Void;

    # === Help ===
    getToolHelp @70 :GetToolHelp;

    # === RAVE Tools ===
    raveEncode @73 :RaveEncode;
    raveDecode @74 :RaveDecode;
    raveReconstruct @75 :RaveReconstruct;
    raveGenerate @76 :RaveGenerate;
    raveStreamStart @77 :RaveStreamStart;
    raveStreamStop @78 :RaveStreamStop;
    raveStreamStatus @79 :RaveStreamStatus;

    # === MIDI I/O Tools (routed to chaosgarden) ===
    midiListPorts @82 :Void;
    midiInputAttach @83 :MidiAttach;
    midiInputDetach @84 :MidiDetach;
    midiOutputAttach @85 :MidiAttach;
    midiOutputDetach @86 :MidiDetach;
    midiSend @87 :MidiSend;
    midiStatus @88 :Void;

    # === AudioLDM2 ===
    audioldm2Generate @90 :Audioldm2Generate;

    # === Anticipatory Music Transformer ===
    anticipatoryGenerate @91 :AnticipatoryGenerate;
    anticipatoryContinue @92 :AnticipatoryContinue;
    anticipatoryEmbed @93 :AnticipatoryEmbed;

    # === Demucs ===
    demucsSeparate @94 :DemucsSeparate;

    # === MIDI Analysis / Voice Separation ===
    midiAnalyze @95 :MidiAnalyze;
    midiVoiceSeparate @96 :MidiVoiceSeparate;
    midiStemsExport @97 :MidiStemsExport;
  }
}

# === MIDI I/O Types ===
struct MidiAttach {
  portPattern @0 :Text;
}

struct MidiDetach {
  portPattern @0 :Text;
}

struct MidiSend {
  portPattern @0 :Text;  # Optional, empty = all outputs
  messageType @1 :Text;  # note_on, note_off, control_change, etc.
  channel @2 :UInt8;
  pitch @3 :UInt8;
  velocity @4 :UInt8;
  controller @5 :UInt8;
  value @6 :UInt8;
  program @7 :UInt8;
  bendValue @8 :Int16;
  rawBytes @9 :Data;
}

# === CAS Types ===
struct CasStore {
  data @0 :Data;
  mimeType @1 :Text;
}

struct CasInspect {
  hash @0 :Text;
}

struct CasGet {
  hash @0 :Text;
}

struct CasUploadFile {
  filePath @0 :Text;
  mimeType @1 :Text;
}

# === Orpheus Types ===
struct OrpheusGenerate {
  model @0 :Text;
  temperature @1 :Float32;
  topP @2 :Float32;
  maxTokens @3 :UInt32;
  numVariations @4 :UInt32;
  metadata @5 :Common.ArtifactMetadata;
}

struct OrpheusGenerateSeeded {
  seedHash @0 :Text;
  model @1 :Text;
  temperature @2 :Float32;
  topP @3 :Float32;
  maxTokens @4 :UInt32;
  numVariations @5 :UInt32;
  metadata @6 :Common.ArtifactMetadata;
}

struct OrpheusContinue {
  inputHash @0 :Text;
  model @1 :Text;
  temperature @2 :Float32;
  topP @3 :Float32;
  maxTokens @4 :UInt32;
  numVariations @5 :UInt32;
  metadata @6 :Common.ArtifactMetadata;
}

struct OrpheusBridge {
  sectionAHash @0 :Text;
  sectionBHash @1 :Text;
  model @2 :Text;
  temperature @3 :Float32;
  topP @4 :Float32;
  maxTokens @5 :UInt32;
  metadata @6 :Common.ArtifactMetadata;
}

struct OrpheusLoops {
  temperature @0 :Float32;
  topP @1 :Float32;
  maxTokens @2 :UInt32;
  numVariations @3 :UInt32;
  seedHash @4 :Text;
  metadata @5 :Common.ArtifactMetadata;
}

struct OrpheusClassify {
  midiHash @0 :Text;
}

# === ABC Types ===
struct AbcParse {
  abc @0 :Text;
}

struct AbcToMidi {
  abc @0 :Text;
  tempoOverride @1 :UInt16;
  transpose @2 :Int8;
  velocity @3 :UInt8;
  channel @4 :UInt8;
  metadata @5 :Common.ArtifactMetadata;
}

struct AbcValidate {
  abc @0 :Text;
}

struct AbcTranspose {
  abc @0 :Text;
  semitones @1 :Int8;
  targetKey @2 :Text;
}

# === MIDI/Audio Types ===
struct ConvertMidiToWav {
  inputHash @0 :Text;
  soundfontHash @1 :Text;
  sampleRate @2 :UInt32;
  metadata @3 :Common.ArtifactMetadata;
}

struct SoundfontInspect {
  soundfontHash @0 :Text;
  includeDrumMap @1 :Bool;
}

struct SoundfontPresetInspect {
  soundfontHash @0 :Text;
  bank @1 :Int32;
  program @2 :Int32;
}

# === Analysis Types ===
struct BeatthisAnalyze {
  audioPath @0 :Text;
  audioHash @1 :Text;
  includeFrames @2 :Bool;
}

struct ClapAnalyze {
  audioHash @0 :Text;
  tasks @1 :List(Text);
  audioBHash @2 :Text;
  textCandidates @3 :List(Text);
  parentId @4 :Text;
  creator @5 :Text;
}

struct MidiInfo {
  artifactId @0 :Text;
  hash @1 :Text;
}

struct AudioInfo {
  artifactId @0 :Text;
  hash @1 :Text;
}

# === Generation Types ===
struct MusicgenGenerate {
  prompt @0 :Text;
  duration @1 :Float32;
  temperature @2 :Float32;
  topK @3 :UInt32;
  topP @4 :Float32;
  guidanceScale @5 :Float32;
  doSample @6 :Bool;
  metadata @7 :Common.ArtifactMetadata;
}

struct YueGenerate {
  lyrics @0 :Text;
  genre @1 :Text;
  maxNewTokens @2 :UInt32;
  runNSegments @3 :UInt32;
  seed @4 :UInt64;
  metadata @5 :Common.ArtifactMetadata;
}

# === Artifact Types ===
struct ArtifactUpload {
  filePath @0 :Text;
  mimeType @1 :Text;
  metadata @2 :Common.ArtifactMetadata;
}

struct ArtifactGet {
  id @0 :Text;
}

struct ArtifactList {
  tag @0 :Text;
  creator @1 :Text;
}

struct ArtifactCreate {
  casHash @0 :Text;
  tags @1 :List(Text);
  creator @2 :Text;
  metadata @3 :Text;  # JSON
}

# === Graph Types ===
struct GraphQuery {
  query @0 :Text;
  variables @1 :Text;  # JSON
  limit @2 :UInt32;
}

struct GraphBind {
  id @0 :Text;
  name @1 :Text;
  hints @2 :List(Common.GraphHint);
}

struct GraphTag {
  identityId @0 :Text;
  namespace @1 :Text;
  value @2 :Text;
}

struct GraphConnect {
  fromIdentity @0 :Text;
  fromPort @1 :Text;
  toIdentity @2 :Text;
  toPort @3 :Text;
  transport @4 :Text;
}

struct GraphFind {
  name @0 :Text;
  tagNamespace @1 :Text;
  tagValue @2 :Text;
}

struct GraphContext {
  tag @0 :Text;
  vibeSearch @1 :Text;
  creator @2 :Text;
  limit @3 :UInt32;
  includeMetadata @4 :Bool;
  includeAnnotations @5 :Bool;
}

struct AddAnnotation {
  artifactId @0 :Text;
  message @1 :Text;
  vibe @2 :Text;
  source @3 :Text;
}

# === Config Types ===
struct ConfigGet {
  section @0 :Text;
  key @1 :Text;
}

# === Lua Types ===
struct LuaEval {
  code @0 :Text;
  params @1 :Text;  # JSON
}

struct LuaDescribe {
  scriptHash @0 :Text;
}

struct ScriptStore {
  content @0 :Text;
  tags @1 :List(Text);
  creator @2 :Text;
}

struct ScriptSearch {
  tag @0 :Text;
  creator @1 :Text;
  vibe @2 :Text;
}

# === Resource Types ===
struct ReadResource {
  uri @0 :Text;
}

# === Completion Types ===
struct Complete {
  context @0 :Text;
  partial @1 :Text;
}

# === Misc Types ===
struct SampleLlm {
  prompt @0 :Text;
  maxTokens @1 :UInt32;
  temperature @2 :Float64;
  systemPrompt @3 :Text;
}

# === Vibeweaver Types ===
struct WeaveEval {
  code @0 :Text;
}

struct WeaveReset {
  clearSession @0 :Bool;
}

struct WeaveHelp {
  topic @0 :Text;  # Empty for general help
}

# === Help Types ===
struct GetToolHelp {
  topic @0 :Text;
}

# === RAVE Types ===
# RAVE (Realtime Audio Variational autoEncoder) audio codec tools

struct RaveEncode {
  audioHash @0 :Text;        # CAS hash of input audio (WAV)
  model @1 :Text;            # Model name (e.g., "vintage", "percussion")
  metadata @2 :Common.ArtifactMetadata;
}

struct RaveDecode {
  latentHash @0 :Text;       # CAS hash of latent codes
  latentShape @1 :List(UInt32);  # Shape of latent tensor
  model @2 :Text;
  metadata @3 :Common.ArtifactMetadata;
}

struct RaveReconstruct {
  audioHash @0 :Text;        # CAS hash of input audio
  model @1 :Text;
  metadata @2 :Common.ArtifactMetadata;
}

struct RaveGenerate {
  model @0 :Text;
  durationSeconds @1 :Float32;
  temperature @2 :Float32;   # Sampling temperature for prior (default 1.0)
  metadata @3 :Common.ArtifactMetadata;
}

struct RaveStreamStart {
  model @0 :Text;
  inputIdentity @1 :Text;    # Graph identity for audio input source
  outputIdentity @2 :Text;   # Graph identity for audio output sink
  bufferSize @3 :UInt32;     # Samples per buffer (default 2048)
}

struct RaveStreamStop {
  streamId @0 :Text;
}

struct RaveStreamStatus {
  streamId @0 :Text;
}

# === Audio Capture ===
struct AudioCapture {
  durationSeconds @0 :Float32;
  source @1 :Text;  # "monitor", "timeline", "mix"
  metadata @2 :Common.ArtifactMetadata;
}

# === AudioLDM2 Types ===
struct Audioldm2Generate {
  prompt @0 :Text;
  negativePrompt @1 :Text;
  duration @2 :Float32;
  numInferenceSteps @3 :UInt32;
  guidanceScale @4 :Float32;
  seed @5 :UInt64;
  metadata @6 :Common.ArtifactMetadata;
}

# === Anticipatory Music Transformer Types ===
struct AnticipatoryGenerate {
  lengthSeconds @0 :Float32;
  topP @1 :Float32;
  modelSize @2 :Text;          # "small", "medium", "large"
  metadata @3 :Common.ArtifactMetadata;
}

struct AnticipatoryContinue {
  inputHash @0 :Text;           # CAS hash of input MIDI
  lengthSeconds @1 :Float32;
  primeSeconds @2 :Float32;
  topP @3 :Float32;
  modelSize @4 :Text;
  metadata @5 :Common.ArtifactMetadata;
}

struct AnticipatoryEmbed {
  inputHash @0 :Text;           # CAS hash of input MIDI
  modelSize @1 :Text;
  embedLayer @2 :Int32;         # Which transformer layer (-3 default)
}

# === Demucs Types ===
struct DemucsSeparate {
  audioHash @0 :Text;           # CAS hash of input audio
  model @1 :Text;               # "htdemucs", "htdemucs_ft", "htdemucs_6s"
  stems @2 :List(Text);         # Filter to specific stems
  twoStems @3 :Text;            # Karaoke mode: "vocals" or "drums" etc.
  metadata @4 :Common.ArtifactMetadata;
}

# === MIDI Analysis / Voice Separation Types ===
struct MidiAnalyze {
  artifactId @0 :Text;
  hash @1 :Text;
  polyphonyThreshold @2 :Float64;   # Default 0.3
  densityWindowBeats @3 :Float64;   # Default 4.0
}

struct MidiVoiceSeparate {
  artifactId @0 :Text;
  hash @1 :Text;
  method @2 :Text;                  # "auto", "channel_split", "pitch_contiguity", "skyline", "bassline"
  maxPitchJump @3 :UInt8;           # Default 12 semitones
  maxGapBeats @4 :Float64;          # Default 4.0
  maxVoices @5 :UInt8;              # Default 8
  trackIndices @6 :List(UInt16);    # Which tracks to separate (empty = all flagged)
}

struct MidiStemsExport {
  artifactId @0 :Text;
  hash @1 :Text;
  voiceData @2 :Text;              # JSON-serialized voice separation results
  combinedFile @3 :Bool;           # Also export combined multi-track file
  metadata @4 :Common.ArtifactMetadata;
}
