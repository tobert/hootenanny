@0xbcdc9a739d8e7582;

using Common = import "common.capnp";

# Vibeweaver kernel state for session persistence and context restoration

struct SessionId {
  value @0 :Text;
}

struct RuleId {
  value @0 :Text;
}

struct MarkerId {
  value @0 :Text;
}

struct Session {
  id @0 :SessionId;
  name @1 :Text;
  vibe @2 :Text;  # Optional, empty if not set
  tempoBpm @3 :Float64;
  createdAt @4 :Common.Timestamp;
  updatedAt @5 :Common.Timestamp;
}

enum TransportState {
  stopped @0;
  playing @1;
  paused @2;
}

struct BeatState {
  current @0 :Float64;
  tempoBpm @1 :Float64;
}

enum JobState {
  pending @0;
  running @1;
  complete @2;
  failed @3;
}

struct JobEntry {
  jobId @0 :Text;
  state @1 :JobState;
  artifactId @2 :Text;  # Optional, empty if not set
}

struct ArtifactRef {
  id @0 :Text;
  contentHash @1 :Text;
  tags @2 :List(Text);
  createdAt @3 :Common.Timestamp;
}

struct KernelState {
  sessionId @0 :SessionId;
  sessionName @1 :Text;
  sessionVibe @2 :Text;  # Optional
  tempoBpm @3 :Float64;
  transport @4 :TransportState;
  beat @5 :BeatState;
  jobs @6 :List(JobEntry);
  recentArtifacts @7 :List(ArtifactRef);
  capturedAtNanos @8 :UInt64;
}

enum TriggerType {
  beat @0;
  marker @1;
  deadline @2;
  artifact @3;
  jobComplete @4;
  transport @5;
}

enum Priority {
  critical @0;
  high @1;
  normal @2;
  low @3;
  idle @4;
}

struct Trigger {
  union {
    beat @0 :BeatTrigger;
    marker @1 :MarkerTrigger;
    deadline @2 :DeadlineTrigger;
    artifact @3 :ArtifactTrigger;
    jobComplete @4 :JobCompleteTrigger;
    transport @5 :TransportTrigger;
  }
}

struct BeatTrigger {
  divisor @0 :UInt32;
}

struct MarkerTrigger {
  name @0 :Text;
}

struct DeadlineTrigger {
  beat @0 :Float64;
}

struct ArtifactTrigger {
  tag @0 :Text;  # Optional, empty for any artifact
}

struct JobCompleteTrigger {
  jobId @0 :Text;
}

struct TransportTrigger {
  state @0 :Text;
}

struct Action {
  union {
    sample @0 :SampleAction;
    schedule @1 :ScheduleAction;
    sampleAndSchedule @2 :SampleAndScheduleAction;
    play @3 :Void;
    pause @4 :Void;
    stop @5 :Void;
    seek @6 :SeekAction;
    audition @7 :AuditionAction;
    notify @8 :NotifyAction;
  }
}

struct SampleAction {
  space @0 :Text;
  prompt @1 :Text;
  inferenceJson @2 :Text;  # JSON params
}

struct ScheduleAction {
  contentHash @0 :Text;
  at @1 :Float64;
  duration @2 :Float64;  # 0 for none
  gain @3 :Float64;
}

struct SampleAndScheduleAction {
  space @0 :Text;
  prompt @1 :Text;
  at @2 :Float64;
}

struct SeekAction {
  beat @0 :Float64;
}

struct AuditionAction {
  contentHash @0 :Text;
  duration @1 :Float64;
}

struct NotifyAction {
  message @0 :Text;
}

struct Rule {
  id @0 :RuleId;
  sessionId @1 :SessionId;
  trigger @2 :Trigger;
  action @3 :Action;
  priority @4 :Priority;
  enabled @5 :Bool;
  oneShot @6 :Bool;
  firedCount @7 :UInt64;
  lastFiredAt @8 :Common.Timestamp;
  createdAt @9 :Common.Timestamp;
}

struct Marker {
  id @0 :MarkerId;
  sessionId @1 :SessionId;
  beat @2 :Float64;
  name @3 :Text;
  metadataJson @4 :Text;  # Optional JSON
  createdAt @5 :Common.Timestamp;
}

struct HistoryEntry {
  id @0 :Int64;
  sessionId @1 :SessionId;
  action @2 :Text;
  paramsJson @3 :Text;
  resultJson @4 :Text;
  success @5 :Bool;
  createdAt @6 :Common.Timestamp;
}
