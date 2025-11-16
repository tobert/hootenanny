@0xcf0d1acdd0bf24b4;

struct EmotionalVector {
  valence @0 :Float32;
  arousal @1 :Float32;
  agency @2 :Float32;
}

struct Event {
  timestamp @0 :UInt64; # Unix timestamp in nanoseconds
  union {
    sessionStarted @1 :Void;
    emotionalStateUpdated @2 :EmotionalVector;
    # Add other events here in the future
  }
}
