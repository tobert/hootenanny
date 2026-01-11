@0xb55d10399d53fbb9;

using Common = import "common.capnp";

# Beat position in musical time
struct Beat {
  value @0 :Float64;
}

# === Garden Commands ===

struct Seek {
  beat @0 :Float64;
}

struct SetTempo {
  bpm @0 :Float64;
}

struct Query {
  query @0 :Text;
  variables @1 :Text;  # JSON
}

struct CreateRegion {
  position @0 :Float64;
  duration @1 :Float64;
  behaviorType @2 :Text;
  contentId @3 :Text;
}

struct DeleteRegion {
  regionId @0 :Text;
}

struct MoveRegion {
  regionId @0 :Text;
  newPosition @1 :Float64;
}

struct GetRegions {
  start @0 :Float64;
  end @1 :Float64;
}

# === Transport Commands ===

struct TransportSeek {
  positionBeats @0 :Float64;
}

# === Timeline Commands ===

struct TimelineQuery {
  fromBeats @0 :Float64;
  toBeats @1 :Float64;
}

struct TimelineAddMarker {
  positionBeats @0 :Float64;
  markerType @1 :Text;
  metadata @2 :Text;  # JSON
}

struct TimelineEvent {
  eventType @0 :Common.TimelineEventType;
  positionBeats @1 :Float64;
  tempo @2 :Float64;
  metadata @3 :Text;  # JSON
}

# === Region Behavior ===

struct Behavior {
  union {
    playContent @0 :Text;  # artifact_id
    latent @1 :Text;       # job_id
  }
}

# === Position Updates (for IOPub) ===

struct PositionUpdate {
  timestamp @0 :Common.Timestamp;
  beat @1 :Beat;
  sampleFrame @2 :UInt64;
}

# === Transport State ===

struct TransportState {
  playing @0 :Bool;
  position @1 :Beat;
  tempo @2 :Float64;
}

# === Region Summary ===

struct RegionSummary {
  regionId @0 :Text;
  position @1 :Beat;
  duration @2 :Beat;
  isLatent @3 :Bool;
  artifactId @4 :Text;
}

# === Audio Output Attachment ===

struct AttachAudio {
  deviceName @0 :Text;
  sampleRate @1 :UInt32;
  latencyFrames @2 :UInt32;
}

struct AudioStatus {
  attached @0 :Bool;
  deviceName @1 :Text;
  sampleRate @2 :UInt32;
  latencyFrames @3 :UInt32;
  callbacks @4 :UInt64;
  samplesWritten @5 :UInt64;
  underruns @6 :UInt64;
}

# === Monitor Input Attachment ===

struct AttachInput {
  deviceName @0 :Text;
  sampleRate @1 :UInt32;
}

struct InputStatus {
  attached @0 :Bool;
  deviceName @1 :Text;
  sampleRate @2 :UInt32;
  channels @3 :UInt32;
  monitorEnabled @4 :Bool;
  monitorGain @5 :Float32;
  callbacks @6 :UInt64;
  samplesCaptured @7 :UInt64;
  overruns @8 :UInt64;
}

struct SetMonitor {
  enabled @0 :Bool;
  enabledSet @1 :Bool;  # true if enabled was explicitly set
  gain @2 :Float32;
  gainSet @3 :Bool;     # true if gain was explicitly set
}

# === State Snapshot Types ===
# These enable hootenanny to fetch chaosgarden state for local query evaluation,
# keeping allocation-heavy Trustfall/GraphQL processing out of the RT process.

# Request for full state snapshot
struct GetSnapshotRequest {}

# Full garden state - everything needed for Trustfall queries
struct GardenSnapshot {
  version @0 :UInt64;              # Monotonic version for cache invalidation
  transport @1 :TransportState;    # Current playback state
  regions @2 :List(RegionSnapshot);
  nodes @3 :List(GraphNode);
  edges @4 :List(GraphEdge);
  latentJobs @5 :List(LatentJob);
  pendingApprovals @6 :List(ApprovalInfo);
  outputs @7 :List(AudioOutput);
  inputs @8 :List(AudioInput);
  midiDevices @9 :List(MidiDeviceInfo);
  tempoMap @10 :TempoMapSnapshot;
}

# Region with all queryable fields
struct RegionSnapshot {
  id @0 :Text;                     # UUID
  position @1 :Float64;            # Beat position
  duration @2 :Float64;            # Duration in beats
  behaviorType @3 :BehaviorType;
  name @4 :Text;                   # Optional name
  tags @5 :List(Text);

  # For PlayContent behavior
  contentHash @6 :Text;
  contentType @7 :ContentTypeEnum;

  # For Latent behavior
  latentStatus @8 :LatentStatusEnum;
  latentProgress @9 :Float32;
  jobId @10 :Text;
  generationTool @11 :Text;

  # Computed/lifecycle flags
  isResolved @12 :Bool;
  isApproved @13 :Bool;
  isPlayable @14 :Bool;
  isAlive @15 :Bool;
  isTombstoned @16 :Bool;
}

enum BehaviorType {
  playContent @0;
  latent @1;
  applyProcessing @2;
  emitTrigger @3;
  custom @4;
}

enum ContentTypeEnum {
  audio @0;
  midi @1;
  control @2;
}

enum LatentStatusEnum {
  none @0;        # Not a latent region
  pending @1;
  running @2;
  resolved @3;
  approved @4;
  rejected @5;
  failed @6;
}

# Graph node with ports and capabilities
struct GraphNode {
  id @0 :Text;                     # UUID
  name @1 :Text;
  typeId @2 :Text;
  inputs @3 :List(Port);
  outputs @4 :List(Port);
  latencySamples @5 :UInt32;
  canRealtime @6 :Bool;
  canOffline @7 :Bool;
}

struct Port {
  name @0 :Text;
  signalType @1 :SignalTypeEnum;
}

enum SignalTypeEnum {
  audio @0;
  midi @1;
  control @2;
  trigger @3;
}

# Graph edge (connection between nodes)
struct GraphEdge {
  sourceId @0 :Text;               # UUID
  sourcePort @1 :Text;
  destId @2 :Text;                 # UUID
  destPort @3 :Text;
}

# Running latent job
struct LatentJob {
  id @0 :Text;                     # Job ID
  regionId @1 :Text;               # UUID
  tool @2 :Text;
  progress @3 :Float32;
}

# Pending approval
struct ApprovalInfo {
  regionId @0 :Text;               # UUID
  contentHash @1 :Text;
  contentType @2 :ContentTypeEnum;
}

# Audio output device
struct AudioOutput {
  id @0 :Text;                     # UUID
  name @1 :Text;
  channels @2 :UInt8;
  pwNodeId @3 :UInt32;             # 0 = not connected
  hasPwNodeId @4 :Bool;
}

# Audio input device
struct AudioInput {
  id @0 :Text;                     # UUID
  name @1 :Text;
  channels @2 :UInt8;
  portPattern @3 :Text;
  pwNodeId @4 :UInt32;             # 0 = not connected
  hasPwNodeId @5 :Bool;
}

# MIDI device
struct MidiDeviceInfo {
  id @0 :Text;                     # UUID
  name @1 :Text;
  direction @2 :MidiDirection;
  pwNodeId @3 :UInt32;             # 0 = not connected
  hasPwNodeId @4 :Bool;
}

enum MidiDirection {
  input @0;
  output @1;
}

# Tempo map for time conversions
struct TempoMapSnapshot {
  defaultTempo @0 :Float64;        # BPM
  ticksPerBeat @1 :UInt32;
  changes @2 :List(TempoChange);
}

struct TempoChange {
  tick @0 :Int64;
  tempo @1 :Float64;               # BPM
}

# === IOPub Events (Cap'n Proto version) ===
# Replaces JSON IOPubEvent for efficient notification

struct IOPubMessage {
  version @0 :UInt64;              # State version after this event
  timestamp @1 :UInt64;            # Unix millis
  event @2 :IOPubEventUnion;
}

struct IOPubEventUnion {
  union {
    # State change (generic - invalidates cache)
    stateChanged @0 :Void;

    # Transport
    playbackStarted @1 :Void;
    playbackStopped @2 :Void;
    playbackPosition @3 :PlaybackPositionEvent;

    # Regions
    regionCreated @4 :Text;        # region_id
    regionDeleted @5 :Text;        # region_id
    regionMoved @6 :RegionMovedEvent;

    # Latent lifecycle
    latentStarted @7 :LatentStartedEvent;
    latentProgress @8 :LatentProgressEvent;
    latentResolved @9 :LatentResolvedEvent;
    latentFailed @10 :LatentFailedEvent;
    latentApproved @11 :Text;      # region_id
    latentRejected @12 :LatentRejectedEvent;

    # Graph changes
    nodeAdded @13 :NodeAddedEvent;
    nodeRemoved @14 :Text;         # node_id
    connectionMade @15 :ConnectionEvent;
    connectionBroken @16 :ConnectionEvent;

    # Audio I/O
    audioAttached @17 :AudioAttachedEvent;
    audioDetached @18 :Void;
    audioUnderrun @19 :UInt64;     # count

    # Errors
    error @20 :ErrorEvent;
    warning @21 :Text;             # message
  }
}

struct PlaybackPositionEvent {
  beat @0 :Float64;
  second @1 :Float64;
}

struct RegionMovedEvent {
  regionId @0 :Text;
  newPosition @1 :Float64;
}

struct LatentStartedEvent {
  regionId @0 :Text;
  jobId @1 :Text;
}

struct LatentProgressEvent {
  regionId @0 :Text;
  progress @1 :Float32;
}

struct LatentResolvedEvent {
  regionId @0 :Text;
  artifactId @1 :Text;
  contentHash @2 :Text;
}

struct LatentFailedEvent {
  regionId @0 :Text;
  error @1 :Text;
}

struct LatentRejectedEvent {
  regionId @0 :Text;
  reason @1 :Text;
}

struct NodeAddedEvent {
  nodeId @0 :Text;
  name @1 :Text;
}

struct ConnectionEvent {
  sourceId @0 :Text;
  sourcePort @1 :Text;
  destId @2 :Text;
  destPort @3 :Text;
}

struct AudioAttachedEvent {
  deviceName @0 :Text;
  sampleRate @1 :UInt32;
  latencyFrames @2 :UInt32;
}

struct ErrorEvent {
  error @0 :Text;
  context @1 :Text;
}
