//! Domain types for stream capture.

use serde::{Deserialize, Serialize};
use std::fmt;

/// URI identifying a stream (e.g., "stream://eurorack-audio/main")
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamUri(pub String);

impl StreamUri {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StreamUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for StreamUri {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for StreamUri {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Sample format for audio streams
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleFormat {
    F32,
    I16,
    I24,
}

impl SampleFormat {
    pub fn bytes_per_sample(self) -> usize {
        match self {
            SampleFormat::F32 => 4,
            SampleFormat::I16 => 2,
            SampleFormat::I24 => 3,
        }
    }
}

/// Audio format specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u8,
    pub sample_format: SampleFormat,
}

/// Stream format (audio or MIDI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamFormat {
    Audio(AudioFormat),
    Midi,
}

/// Static definition of a stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamDefinition {
    pub uri: StreamUri,
    pub device_identity: String,
    pub format: StreamFormat,
    pub chunk_size_bytes: u64,
}

/// Current status of a stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamStatus {
    Recording,
    Stopped,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_uri() {
        let uri = StreamUri::from("stream://test/audio");
        assert_eq!(uri.as_str(), "stream://test/audio");
        assert_eq!(uri.to_string(), "stream://test/audio");
    }

    #[test]
    fn test_sample_format_bytes() {
        assert_eq!(SampleFormat::F32.bytes_per_sample(), 4);
        assert_eq!(SampleFormat::I16.bytes_per_sample(), 2);
        assert_eq!(SampleFormat::I24.bytes_per_sample(), 3);
    }

    #[test]
    fn test_stream_definition_serialization() {
        let def = StreamDefinition {
            uri: StreamUri::from("stream://test/audio"),
            device_identity: "test-device".to_string(),
            format: StreamFormat::Audio(AudioFormat {
                sample_rate: 48000,
                channels: 2,
                sample_format: SampleFormat::F32,
            }),
            chunk_size_bytes: 1024 * 1024,
        };

        let json = serde_json::to_string(&def).unwrap();
        let deserialized: StreamDefinition = serde_json::from_str(&json).unwrap();

        assert_eq!(def.uri, deserialized.uri);
        assert_eq!(def.device_identity, deserialized.device_identity);
        assert_eq!(def.chunk_size_bytes, deserialized.chunk_size_bytes);
    }
}
