@0xf8721cef9bc45e2a;

# Common types shared across all schemas

struct Timestamp {
  nanos @0 :UInt64;  # Nanoseconds since UNIX epoch
}

struct Uuid {
  low @0 :UInt64;
  high @1 :UInt64;
}

struct Error {
  code @0 :Text;
  message @1 :Text;
  details @2 :Text;  # JSON string for flexibility
}

enum JobStatus {
  pending @0;
  running @1;
  complete @2;
  failed @3;
  cancelled @4;
}

enum WorkerType {
  hootenanny @0;
  chaosgarden @1;
  vibeweaver @2;
}

enum PollMode {
  any @0;
  all @1;
}

enum TimelineEventType {
  sectionChange @0;
  beatMarker @1;
  cuePoint @2;
  generateTransition @3;
}

struct ArtifactMetadata {
  variationSetId @0 :Text;
  parentId @1 :Text;
  tags @2 :List(Text);
  creator @3 :Text;
}

struct GraphHint {
  kind @0 :Text;
  value @1 :Text;
  confidence @2 :Float64;
}

struct ToolInfo {
  name @0 :Text;
  description @1 :Text;
  inputSchema @2 :Text;  # JSON string
}

struct WorkerRegistration {
  workerId @0 :Uuid;
  workerType @1 :WorkerType;
  workerName @2 :Text;
  capabilities @3 :List(Text);
  hostname @4 :Text;
  version @5 :Text;
}

struct Encoding {
  union {
    midi @0 :Text; # artifact_id
    audio @1 :Text; # artifact_id
    abc @2 :Text; # notation
    hash :group {
      contentHash @3 :Text;
      format @4 :Text;
    }
  }
}

enum AnalysisTask {
  classify @0;
  beats @1;
  embeddings @2;
  genre @3;
  mood @4;
  zeroShot @5;
}

# Generative spaces for sample/extend/bridge tools
enum Space {
  orpheus @0;
  orpheusChildren @1;
  orpheusMonoMelodies @2;
  orpheusLoops @3;
  orpheusBridge @4;
  musicGen @5;
  yue @6;
  abc @7;
}

# Inference parameters for generative models
struct InferenceContext {
  temperature @0 :Float32;
  topP @1 :Float32;
  topK @2 :UInt32;
  seed @3 :UInt64;
  maxTokens @4 :UInt32;
  durationSeconds @5 :Float32;
  guidanceScale @6 :Float32;
  variant @7 :Text;
}

# Target format for content projection
struct ProjectionTarget {
  union {
    audio :group {
      soundfontHash @0 :Text;
      sampleRate @1 :UInt32;
    }
    midi :group {
      channel @2 :UInt8;
      velocity @3 :UInt8;
      program @4 :UInt8;
    }
  }
}
