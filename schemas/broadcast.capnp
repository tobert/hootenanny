@0xab20a00d32798739;

using Common = import "common.capnp";
using Streams = import "streams.capnp";

# Broadcast messages via PUB/SUB
struct Broadcast {
  union {
    configUpdate @0 :ConfigUpdate;
    shutdown @1 :ShutdownBroadcast;
    scriptInvalidate @2 :ScriptInvalidate;
    jobStateChanged @3 :JobStateChanged;
    progress @4 :Progress;
    artifactCreated @5 :ArtifactCreated;
    transportStateChanged @6 :TransportStateChanged;
    markerReached @7 :MarkerReached;
    beatTick @8 :BeatTick;
    log @9 :Log;

    # === Stream Capture Events (Chaosgarden → Hootenanny) ===
    streamHeadPosition @10 :Streams.StreamHeadPosition;
    streamChunkFull @11 :Streams.StreamChunkFull;
    streamError @12 :Streams.StreamError;

    # === Device Hot-Plug Events ===
    deviceConnected @13 :DeviceConnected;
    deviceDisconnected @14 :DeviceDisconnected;

    # === Audio Output Attachment Events ===
    audioAttached @15 :AudioAttached;
    audioDetached @16 :AudioDetached;
    audioUnderrun @17 :AudioUnderrun;
  }
}

struct ConfigUpdate {
  key @0 :Text;
  value @1 :Text;  # JSON
}

struct ShutdownBroadcast {
  reason @0 :Text;
}

struct ScriptInvalidate {
  hash @0 :Text;
}

struct JobStateChanged {
  jobId @0 :Text;
  state @1 :Text;
  result @2 :Text;  # JSON, optional
}

struct Progress {
  jobId @0 :Text;
  percent @1 :Float32;
  message @2 :Text;
}

struct ArtifactCreated {
  artifactId @0 :Text;
  contentHash @1 :Text;
  tags @2 :List(Text);
  creator @3 :Text;
}

struct TransportStateChanged {
  state @0 :Text;
  positionBeats @1 :Float64;
  tempoBpm @2 :Float64;
}

struct MarkerReached {
  positionBeats @0 :Float64;
  markerType @1 :Text;
  metadata @2 :Text;  # JSON
}

struct BeatTick {
  timestamp @0 :Common.Timestamp;
  beat @1 :UInt64;
  positionBeats @2 :Float64;
  tempoBpm @3 :Float64;
}

struct Log {
  level @0 :Text;
  message @1 :Text;
  source @2 :Text;
}

# Note: Stream events added to Broadcast union above (indices 10-12)
# Note: Device events added at indices 13-14

struct DeviceConnected {
  pipewireId @0 :UInt32;
  name @1 :Text;
  mediaClass @2 :Text;      # Optional, empty string if not available
  identityId @3 :Text;      # Optional, empty string if no match
  identityName @4 :Text;    # Optional, empty string if no match
}

struct DeviceDisconnected {
  pipewireId @0 :UInt32;
  name @1 :Text;            # Optional, empty string if unknown
}

# === Audio Output Attachment Events ===

struct AudioAttached {
  deviceName @0 :Text;
  sampleRate @1 :UInt32;
  latencyFrames @2 :UInt32;
}

struct AudioDetached {
  # Empty - just signals detachment
}

struct AudioUnderrun {
  count @0 :UInt64;
}
