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
