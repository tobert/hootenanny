//! Model-native API types for unified generative operations.
//!
//! This module provides the foundation types for the model-native API transformation,
//! enabling tools like `sample()`, `project()`, `schedule()`, and `analyze()` to work
//! across different generative spaces (Orpheus, MusicGen, YuE, etc.) with a consistent
//! interface.

use serde::{Deserialize, Serialize};

/// Return type for Orpheus model parameters: (variant, temperature, top_p, max_tokens)
pub type OrpheusParams = (Option<String>, Option<f32>, Option<f32>, Option<u32>);

/// Return type for MusicGen parameters: (temperature, top_p, top_k, guidance_scale, duration_seconds)
pub type MusicGenParams = (
    Option<f32>,
    Option<f32>,
    Option<u32>,
    Option<f32>,
    Option<f32>,
);

/// Generative domain spaces available in Hootenanny.
///
/// Each space represents a different generative model or approach, with its own
/// characteristics, output types, and capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(description = "Generative domain space (model variant)")]
pub enum Space {
    /// Orpheus base model - general MIDI generation
    Orpheus,

    /// Orpheus children's music model
    OrpheusChildren,

    /// Orpheus monophonic melodies model
    OrpheusMonoMelodies,

    /// Orpheus loopable patterns model
    OrpheusLoops,

    /// Orpheus bridge/transition generation model
    OrpheusBridge,

    /// MusicGen audio generation model
    MusicGen,

    /// YuE lyrics-to-song model
    Yue,

    /// ABC notation symbolic music
    Abc,
}

impl Space {
    /// Returns the output type produced by this space.
    pub fn output_type(&self) -> OutputType {
        match self {
            Space::Orpheus
            | Space::OrpheusChildren
            | Space::OrpheusMonoMelodies
            | Space::OrpheusLoops
            | Space::OrpheusBridge => OutputType::Midi,
            Space::MusicGen | Space::Yue => OutputType::Audio,
            Space::Abc => OutputType::Symbolic,
        }
    }

    /// Returns true if this space supports continuation/extension operations.
    pub fn supports_continuation(&self) -> bool {
        match self {
            Space::Orpheus
            | Space::OrpheusChildren
            | Space::OrpheusMonoMelodies
            | Space::OrpheusLoops => true,
            Space::OrpheusBridge | Space::MusicGen | Space::Yue | Space::Abc => false,
        }
    }

    /// Returns the underlying model variant string used by the generative backend.
    pub fn model_variant(&self) -> Option<&str> {
        match self {
            Space::Orpheus => Some("base"),
            Space::OrpheusChildren => Some("children"),
            Space::OrpheusMonoMelodies => Some("mono_melodies"),
            Space::OrpheusLoops => None, // Uses dedicated loops endpoint
            Space::OrpheusBridge => Some("bridge"),
            Space::MusicGen => None, // MusicGen has its own model selection
            Space::Yue => None,      // YuE doesn't expose model variants
            Space::Abc => None,      // ABC is symbolic, not model-based
        }
    }
}

/// Output format type produced by generative operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(description = "Output format type")]
pub enum OutputType {
    /// MIDI format (symbolic music events)
    Midi,

    /// Audio format (PCM waveform)
    Audio,

    /// Symbolic notation (ABC, MusicXML, etc.)
    Symbolic,
}

/// Content reference encoding - union type for different ways to reference content.
///
/// This enum provides a flexible way to specify input content, whether it's an
/// artifact in the system, raw ABC notation, or a CAS hash reference.
#[derive(Debug, Clone)]
pub enum Encoding {
    /// MIDI content via artifact ID
    Midi { artifact_id: String },

    /// Audio content via artifact ID
    Audio { artifact_id: String },

    /// ABC notation as raw string
    Abc { notation: String },

    /// Raw content via CAS hash
    Hash { content_hash: String, format: String },
}

// Custom JsonSchema that matches the internally-tagged serde format
impl schemars::JsonSchema for Encoding {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Encoding")
    }

    fn json_schema(_gen: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Content reference encoding",
            "oneOf": [
                {
                    "type": "object",
                    "description": "MIDI content via artifact ID",
                    "properties": {
                        "type": { "const": "midi" },
                        "artifact_id": { "type": "string", "description": "Artifact ID of MIDI content" }
                    },
                    "required": ["type", "artifact_id"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "description": "Audio content via artifact ID",
                    "properties": {
                        "type": { "const": "audio" },
                        "artifact_id": { "type": "string", "description": "Artifact ID of audio content" }
                    },
                    "required": ["type", "artifact_id"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "description": "ABC notation as raw string",
                    "properties": {
                        "type": { "const": "abc" },
                        "notation": { "type": "string", "description": "ABC notation string" }
                    },
                    "required": ["type", "notation"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "description": "Raw content via CAS hash",
                    "properties": {
                        "type": { "const": "hash" },
                        "content_hash": { "type": "string", "description": "Content-addressable storage hash" },
                        "format": { "type": "string", "description": "Format identifier (e.g., 'audio/midi', 'audio/wav')" }
                    },
                    "required": ["type", "content_hash", "format"],
                    "additionalProperties": false
                }
            ]
        })
    }
}

impl Encoding {
    /// Returns the output type of this encoding.
    pub fn output_type(&self) -> OutputType {
        match self {
            Encoding::Midi { .. } => OutputType::Midi,
            Encoding::Audio { .. } => OutputType::Audio,
            Encoding::Abc { .. } => OutputType::Symbolic,
            Encoding::Hash { format, .. } => {
                if format.contains("midi") {
                    OutputType::Midi
                } else if format.contains("audio") || format.contains("wav") {
                    OutputType::Audio
                } else {
                    OutputType::Symbolic
                }
            }
        }
    }

    /// Returns the artifact ID if this encoding references one.
    pub fn artifact_id(&self) -> Option<&str> {
        match self {
            Encoding::Midi { artifact_id } | Encoding::Audio { artifact_id } => {
                Some(artifact_id.as_str())
            }
            Encoding::Abc { .. } | Encoding::Hash { .. } => None,
        }
    }

    /// Returns the content hash if this encoding is a hash reference.
    pub fn content_hash(&self) -> Option<&str> {
        match self {
            Encoding::Hash { content_hash, .. } => Some(content_hash.as_str()),
            _ => None,
        }
    }
}

// Custom deserializer that handles both proper objects AND JSON strings (MCP workaround)
impl<'de> Deserialize<'de> for Encoding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        // First try to deserialize as a Value
        let value = serde_json::Value::deserialize(deserializer)?;

        // If it's a string, try to parse it as JSON
        let value = if let serde_json::Value::String(s) = &value {
            serde_json::from_str(s).map_err(D::Error::custom)?
        } else {
            value
        };

        // Now deserialize the tagged enum from the value
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum EncodingHelper {
            Midi { artifact_id: String },
            Audio { artifact_id: String },
            Abc { notation: String },
            Hash { content_hash: String, format: String },
        }

        let helper: EncodingHelper =
            serde_json::from_value(value).map_err(D::Error::custom)?;

        Ok(match helper {
            EncodingHelper::Midi { artifact_id } => Encoding::Midi { artifact_id },
            EncodingHelper::Audio { artifact_id } => Encoding::Audio { artifact_id },
            EncodingHelper::Abc { notation } => Encoding::Abc { notation },
            EncodingHelper::Hash { content_hash, format } => {
                Encoding::Hash { content_hash, format }
            }
        })
    }
}

// Custom serializer that produces internally-tagged JSON with "type" field
impl Serialize for Encoding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        match self {
            Encoding::Midi { artifact_id } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "midi")?;
                map.serialize_entry("artifact_id", artifact_id)?;
                map.end()
            }
            Encoding::Audio { artifact_id } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "audio")?;
                map.serialize_entry("artifact_id", artifact_id)?;
                map.end()
            }
            Encoding::Abc { notation } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "abc")?;
                map.serialize_entry("notation", notation)?;
                map.end()
            }
            Encoding::Hash {
                content_hash,
                format,
            } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "hash")?;
                map.serialize_entry("content_hash", content_hash)?;
                map.serialize_entry("format", format)?;
                map.end()
            }
        }
    }
}

/// Projection target for the `project()` tool.
///
/// Specifies how content should be transformed or rendered, such as converting
/// MIDI to audio via a SoundFont or adjusting MIDI parameters.
#[derive(Debug, Clone)]
pub enum ProjectionTarget {
    /// Render to audio using a SoundFont
    Audio {
        soundfont_hash: String,
        sample_rate: Option<u32>,
    },

    /// Project to MIDI with specific parameters
    Midi {
        channel: Option<u8>,
        velocity: Option<u8>,
    },
}

// Custom JsonSchema that matches the internally-tagged serde format
impl schemars::JsonSchema for ProjectionTarget {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ProjectionTarget")
    }

    fn json_schema(_gen: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Projection target for content transformation",
            "oneOf": [
                {
                    "type": "object",
                    "description": "Render to audio using a SoundFont",
                    "properties": {
                        "type": { "const": "audio" },
                        "soundfont_hash": { "type": "string", "description": "CAS hash of SoundFont file" },
                        "sample_rate": { "type": "integer", "description": "Sample rate in Hz (default: 44100)" }
                    },
                    "required": ["type", "soundfont_hash"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "description": "Project to MIDI with specific parameters",
                    "properties": {
                        "type": { "const": "midi" },
                        "channel": { "type": "integer", "description": "MIDI channel (0-15)" },
                        "velocity": { "type": "integer", "description": "MIDI velocity (1-127)" }
                    },
                    "required": ["type"],
                    "additionalProperties": false
                }
            ]
        })
    }
}

// Custom deserializer that handles both proper objects AND JSON strings (MCP workaround)
impl<'de> Deserialize<'de> for ProjectionTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = serde_json::Value::deserialize(deserializer)?;

        // If it's a string, try to parse it as JSON
        let value = if let serde_json::Value::String(s) = &value {
            serde_json::from_str(s).map_err(D::Error::custom)?
        } else {
            value
        };

        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum Helper {
            Audio {
                soundfont_hash: String,
                sample_rate: Option<u32>,
            },
            Midi {
                channel: Option<u8>,
                velocity: Option<u8>,
            },
        }

        let helper: Helper = serde_json::from_value(value).map_err(D::Error::custom)?;

        Ok(match helper {
            Helper::Audio {
                soundfont_hash,
                sample_rate,
            } => ProjectionTarget::Audio {
                soundfont_hash,
                sample_rate,
            },
            Helper::Midi { channel, velocity } => ProjectionTarget::Midi { channel, velocity },
        })
    }
}

// Custom serializer that produces internally-tagged JSON with "type" field
impl Serialize for ProjectionTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        match self {
            ProjectionTarget::Audio {
                soundfont_hash,
                sample_rate,
            } => {
                let len = if sample_rate.is_some() { 3 } else { 2 };
                let mut map = serializer.serialize_map(Some(len))?;
                map.serialize_entry("type", "audio")?;
                map.serialize_entry("soundfont_hash", soundfont_hash)?;
                if let Some(rate) = sample_rate {
                    map.serialize_entry("sample_rate", rate)?;
                }
                map.end()
            }
            ProjectionTarget::Midi { channel, velocity } => {
                let mut len = 1;
                if channel.is_some() {
                    len += 1;
                }
                if velocity.is_some() {
                    len += 1;
                }
                let mut map = serializer.serialize_map(Some(len))?;
                map.serialize_entry("type", "midi")?;
                if let Some(ch) = channel {
                    map.serialize_entry("channel", ch)?;
                }
                if let Some(vel) = velocity {
                    map.serialize_entry("velocity", vel)?;
                }
                map.end()
            }
        }
    }
}

/// Inference parameters for generative sampling
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InferenceContext {
    #[schemars(description = "Sampling temperature 0.0-2.0 (higher = more random)")]
    pub temperature: Option<f32>,

    #[schemars(description = "Nucleus sampling 0.0-1.0 (lower = more focused)")]
    pub top_p: Option<f32>,

    #[schemars(description = "Top-k filtering (0 = disabled)")]
    pub top_k: Option<u32>,

    #[schemars(description = "Random seed for reproducibility")]
    pub seed: Option<u64>,

    #[schemars(description = "Max tokens to generate (space-dependent)")]
    pub max_tokens: Option<u32>,

    #[schemars(description = "Duration in seconds (for audio spaces)")]
    pub duration_seconds: Option<f32>,

    #[schemars(description = "Guidance scale for CFG (higher = stronger conditioning)")]
    pub guidance_scale: Option<f32>,

    #[schemars(description = "Model variant within space")]
    pub variant: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

impl InferenceContext {
    /// Validate parameter ranges
    pub fn validate(&self) -> Result<(), ValidationError> {
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                return Err(ValidationError {
                    field: "temperature".to_string(),
                    message: format!("must be between 0.0 and 2.0, got {}", temp),
                });
            }
        }

        if let Some(top_p) = self.top_p {
            if !(0.0..=1.0).contains(&top_p) {
                return Err(ValidationError {
                    field: "top_p".to_string(),
                    message: format!("must be between 0.0 and 1.0, got {}", top_p),
                });
            }
        }

        if let Some(duration) = self.duration_seconds {
            if duration <= 0.0 {
                return Err(ValidationError {
                    field: "duration_seconds".to_string(),
                    message: format!("must be greater than 0.0, got {}", duration),
                });
            }
        }

        if let Some(guidance) = self.guidance_scale {
            if guidance < 0.0 {
                return Err(ValidationError {
                    field: "guidance_scale".to_string(),
                    message: format!("must be non-negative, got {}", guidance),
                });
            }
        }

        Ok(())
    }

    /// Merge with defaults for orpheus models
    fn with_orpheus_defaults(&self) -> Self {
        Self {
            temperature: self.temperature.or(Some(1.0)),
            top_p: self.top_p.or(Some(0.95)),
            top_k: self.top_k,
            seed: self.seed,
            max_tokens: self.max_tokens.or(Some(1024)),
            duration_seconds: self.duration_seconds,
            guidance_scale: self.guidance_scale,
            variant: self.variant.clone(),
        }
    }

    /// Merge with defaults for musicgen
    fn with_musicgen_defaults(&self) -> Self {
        Self {
            temperature: self.temperature.or(Some(1.0)),
            top_p: self.top_p.or(Some(0.9)),
            top_k: self.top_k.or(Some(250)),
            seed: self.seed,
            max_tokens: self.max_tokens,
            duration_seconds: self.duration_seconds.or(Some(10.0)),
            guidance_scale: self.guidance_scale.or(Some(3.0)),
            variant: self.variant.clone(),
        }
    }

    /// Merge with defaults for a given space
    ///
    /// Note: This is a placeholder implementation. Once the Space type is defined,
    /// this should be updated to match against space variants.
    pub fn with_defaults_for(&self, _space: &str) -> Self {
        // TODO: Match against Space enum variants once Space type is implemented
        // For now, using conservative defaults
        Self {
            temperature: self.temperature.or(Some(1.0)),
            top_p: self.top_p.or(Some(0.95)),
            top_k: self.top_k,
            seed: self.seed,
            max_tokens: self.max_tokens.or(Some(1024)),
            duration_seconds: self.duration_seconds,
            guidance_scale: self.guidance_scale,
            variant: self.variant.clone(),
        }
    }

    /// Convert to parameters for orpheus tools
    ///
    /// Returns: (variant, temperature, top_p, max_tokens)
    pub fn to_orpheus_params(&self) -> OrpheusParams {
        let defaults = self.with_orpheus_defaults();
        (
            defaults.variant,
            defaults.temperature,
            defaults.top_p,
            defaults.max_tokens,
        )
    }

    /// Convert to parameters for musicgen
    ///
    /// Returns: (temperature, top_p, top_k, guidance_scale, duration_seconds)
    pub fn to_musicgen_params(&self) -> MusicGenParams {
        let defaults = self.with_musicgen_defaults();
        (
            defaults.temperature,
            defaults.top_p,
            defaults.top_k,
            defaults.guidance_scale,
            defaults.duration_seconds,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_space_output_types() {
        assert_eq!(Space::Orpheus.output_type(), OutputType::Midi);
        assert_eq!(Space::OrpheusChildren.output_type(), OutputType::Midi);
        assert_eq!(Space::OrpheusMonoMelodies.output_type(), OutputType::Midi);
        assert_eq!(Space::OrpheusLoops.output_type(), OutputType::Midi);
        assert_eq!(Space::OrpheusBridge.output_type(), OutputType::Midi);
        assert_eq!(Space::MusicGen.output_type(), OutputType::Audio);
        assert_eq!(Space::Yue.output_type(), OutputType::Audio);
        assert_eq!(Space::Abc.output_type(), OutputType::Symbolic);
    }

    #[test]
    fn test_space_continuation_support() {
        assert!(Space::Orpheus.supports_continuation());
        assert!(Space::OrpheusChildren.supports_continuation());
        assert!(Space::OrpheusMonoMelodies.supports_continuation());
        assert!(Space::OrpheusLoops.supports_continuation());
        assert!(!Space::OrpheusBridge.supports_continuation());
        assert!(!Space::MusicGen.supports_continuation());
        assert!(!Space::Yue.supports_continuation());
        assert!(!Space::Abc.supports_continuation());
    }

    #[test]
    fn test_space_model_variants() {
        assert_eq!(Space::Orpheus.model_variant(), Some("base"));
        assert_eq!(Space::OrpheusChildren.model_variant(), Some("children"));
        assert_eq!(
            Space::OrpheusMonoMelodies.model_variant(),
            Some("mono_melodies")
        );
        assert_eq!(Space::OrpheusLoops.model_variant(), None);
        assert_eq!(Space::OrpheusBridge.model_variant(), Some("bridge"));
        assert_eq!(Space::MusicGen.model_variant(), None);
        assert_eq!(Space::Yue.model_variant(), None);
        assert_eq!(Space::Abc.model_variant(), None);
    }

    #[test]
    fn test_encoding_output_types() {
        let midi = Encoding::Midi {
            artifact_id: "artifact_123".to_string(),
        };
        assert_eq!(midi.output_type(), OutputType::Midi);

        let audio = Encoding::Audio {
            artifact_id: "artifact_456".to_string(),
        };
        assert_eq!(audio.output_type(), OutputType::Audio);

        let abc = Encoding::Abc {
            notation: "X:1\nK:C\nCDEF".to_string(),
        };
        assert_eq!(abc.output_type(), OutputType::Symbolic);

        let hash_midi = Encoding::Hash {
            content_hash: "sha256_abc".to_string(),
            format: "audio/midi".to_string(),
        };
        assert_eq!(hash_midi.output_type(), OutputType::Midi);

        let hash_audio = Encoding::Hash {
            content_hash: "sha256_def".to_string(),
            format: "audio/wav".to_string(),
        };
        assert_eq!(hash_audio.output_type(), OutputType::Audio);
    }

    #[test]
    fn test_encoding_artifact_id() {
        let midi = Encoding::Midi {
            artifact_id: "artifact_123".to_string(),
        };
        assert_eq!(midi.artifact_id(), Some("artifact_123"));

        let audio = Encoding::Audio {
            artifact_id: "artifact_456".to_string(),
        };
        assert_eq!(audio.artifact_id(), Some("artifact_456"));

        let abc = Encoding::Abc {
            notation: "X:1\nK:C\nCDEF".to_string(),
        };
        assert_eq!(abc.artifact_id(), None);

        let hash = Encoding::Hash {
            content_hash: "sha256_abc".to_string(),
            format: "audio/midi".to_string(),
        };
        assert_eq!(hash.artifact_id(), None);
    }

    #[test]
    fn test_encoding_content_hash() {
        let midi = Encoding::Midi {
            artifact_id: "artifact_123".to_string(),
        };
        assert_eq!(midi.content_hash(), None);

        let hash = Encoding::Hash {
            content_hash: "sha256_abc".to_string(),
            format: "audio/midi".to_string(),
        };
        assert_eq!(hash.content_hash(), Some("sha256_abc"));
    }

    #[test]
    fn test_serde_space() {
        let space = Space::Orpheus;
        let json = serde_json::to_string(&space).unwrap();
        assert_eq!(json, r#""orpheus""#);

        let deserialized: Space = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, space);
    }

    #[test]
    fn test_serde_encoding() {
        let encoding = Encoding::Midi {
            artifact_id: "artifact_123".to_string(),
        };
        let json = serde_json::to_string(&encoding).unwrap();
        let deserialized: Encoding = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.artifact_id(), Some("artifact_123"));
        assert_eq!(deserialized.output_type(), OutputType::Midi);
    }

    #[test]
    fn test_serde_projection_target() {
        let target = ProjectionTarget::Audio {
            soundfont_hash: "sha256_soundfont".to_string(),
            sample_rate: Some(48000),
        };
        let json = serde_json::to_string(&target).unwrap();
        let deserialized: ProjectionTarget = serde_json::from_str(&json).unwrap();

        match deserialized {
            ProjectionTarget::Audio {
                soundfont_hash,
                sample_rate,
            } => {
                assert_eq!(soundfont_hash, "sha256_soundfont");
                assert_eq!(sample_rate, Some(48000));
            }
            _ => panic!("Expected Audio variant"),
        }
    }

    #[test]
    fn test_validate_temperature_range() {
        let invalid = InferenceContext {
            temperature: Some(2.5),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        let valid = InferenceContext {
            temperature: Some(1.0),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_validate_top_p_range() {
        let invalid = InferenceContext {
            top_p: Some(1.5),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        let valid = InferenceContext {
            top_p: Some(0.95),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_validate_duration() {
        let invalid = InferenceContext {
            duration_seconds: Some(-1.0),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());

        let valid = InferenceContext {
            duration_seconds: Some(10.0),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn test_orpheus_params_defaults() {
        let ctx = InferenceContext::default();
        let (variant, temp, top_p, max_tokens) = ctx.to_orpheus_params();

        assert_eq!(variant, None);
        assert_eq!(temp, Some(1.0));
        assert_eq!(top_p, Some(0.95));
        assert_eq!(max_tokens, Some(1024));
    }

    #[test]
    fn test_musicgen_params_defaults() {
        let ctx = InferenceContext::default();
        let (temp, top_p, top_k, guidance, duration) = ctx.to_musicgen_params();

        assert_eq!(temp, Some(1.0));
        assert_eq!(top_p, Some(0.9));
        assert_eq!(top_k, Some(250));
        assert_eq!(guidance, Some(3.0));
        assert_eq!(duration, Some(10.0));
    }

    #[test]
    fn test_orpheus_params_with_overrides() {
        let ctx = InferenceContext {
            temperature: Some(1.5),
            top_p: Some(0.8),
            max_tokens: Some(512),
            variant: Some("bridge".to_string()),
            ..Default::default()
        };
        let (variant, temp, top_p, max_tokens) = ctx.to_orpheus_params();

        assert_eq!(variant, Some("bridge".to_string()));
        assert_eq!(temp, Some(1.5));
        assert_eq!(top_p, Some(0.8));
        assert_eq!(max_tokens, Some(512));
    }

    #[test]
    fn test_musicgen_params_with_overrides() {
        let ctx = InferenceContext {
            temperature: Some(0.8),
            top_p: Some(0.85),
            top_k: Some(100),
            guidance_scale: Some(5.0),
            duration_seconds: Some(15.0),
            ..Default::default()
        };
        let (temp, top_p, top_k, guidance, duration) = ctx.to_musicgen_params();

        assert_eq!(temp, Some(0.8));
        assert_eq!(top_p, Some(0.85));
        assert_eq!(top_k, Some(100));
        assert_eq!(guidance, Some(5.0));
        assert_eq!(duration, Some(15.0));
    }
}
