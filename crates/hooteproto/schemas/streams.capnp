@0xf6c97ca6e0cdcd69;

using Common = import "common.capnp";

# Stream definition stored in CAS
struct StreamDefinition {
  uri @0 :Text;
  deviceIdentity @1 :Text;
  format @2 :StreamFormat;
  chunkSizeBytes @3 :UInt64;
}

struct StreamFormat {
  union {
    audio @0 :AudioFormat;
    midi @1 :Void;
  }
}

struct AudioFormat {
  sampleRate @0 :UInt32;
  channels @1 :UInt8;
  sampleFormat @2 :SampleFormat;
}

enum SampleFormat {
  f32 @0;
  i16 @1;
  i24 @2;
}

# Stream commands (hootenanny → chaosgarden)
struct StreamStart {
  uri @0 :Text;
  definition @1 :StreamDefinition;
  chunkPath @2 :Text;
}

struct StreamSwitchChunk {
  uri @0 :Text;
  newChunkPath @1 :Text;
}

struct StreamStop {
  uri @0 :Text;
}

# Stream events (chaosgarden → hootenanny via Broadcast)
struct StreamHeadPosition {
  streamUri @0 :Text;
  samplePosition @1 :UInt64;
  bytePosition @2 :UInt64;
  wallClock @3 :Common.Timestamp;
}

struct StreamChunkFull {
  streamUri @0 :Text;
  path @1 :Text;
  bytesWritten @2 :UInt64;
  samplesWritten @3 :UInt64;  # 0 for MIDI
  wallClock @4 :Common.Timestamp;
}

struct StreamError {
  streamUri @0 :Text;
  error @1 :Text;
  recoverable @2 :Bool;
}
