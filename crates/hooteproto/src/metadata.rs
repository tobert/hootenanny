//! Typed metadata types for artifacts and generation.
//!
//! These types replace loose `serde_json::Value` metadata with structured fields.
//! The pattern is: typed known fields + `extra` escape hatch for extensibility.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generation parameters for AI models (Orpheus, MusicGen, YuE, etc.)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GenerationParams {
    /// Model name/identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Sampling temperature (higher = more random)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Top-p (nucleus) sampling threshold
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Maximum tokens to generate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Random seed for reproducibility
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,

    /// Number of variations requested
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_variations: Option<u32>,
}

/// Metrics from generation or processing
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// Duration of audio/MIDI in milliseconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,

    /// Sample rate for audio (e.g., 44100, 48000)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,

    /// Number of audio channels
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<u8>,

    /// Number of MIDI tracks
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_tracks: Option<u16>,

    /// MIDI ticks per quarter note
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticks_per_quarter: Option<u16>,

    /// Processing time in milliseconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub processing_time_ms: Option<u64>,
}

/// Stored artifact metadata with typed fields + escape hatch.
///
/// Use the typed fields for known data, `extra` for truly dynamic content.
/// This is what gets persisted with artifacts in the artifact store.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StoredMetadata {
    /// Generation parameters if this artifact was AI-generated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<GenerationParams>,

    /// Metrics about the artifact content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,

    /// Source artifact hash if this was derived from another
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,

    /// Tool that created this artifact
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,

    /// Escape hatch for truly dynamic/unknown fields
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, serde_json::Value>,
}

impl StoredMetadata {
    /// Create empty metadata
    pub fn new() -> Self {
        Self::default()
    }

    /// Create metadata with generation params
    pub fn with_generation(mut self, params: GenerationParams) -> Self {
        self.generation = Some(params);
        self
    }

    /// Create metadata with metrics
    pub fn with_metrics(mut self, metrics: Metrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Set the source hash
    pub fn with_source(mut self, hash: impl Into<String>) -> Self {
        self.source_hash = Some(hash.into());
        self
    }

    /// Set the tool name
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    /// Add an extra field
    pub fn with_extra(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra.insert(key.into(), value);
        self
    }

    /// Check if metadata is empty
    pub fn is_empty(&self) -> bool {
        self.generation.is_none()
            && self.metrics.is_none()
            && self.source_hash.is_none()
            && self.tool.is_none()
            && self.extra.is_empty()
    }
}

impl GenerationParams {
    /// Create params for Orpheus generation
    pub fn orpheus(
        model: Option<String>,
        temperature: Option<f32>,
        top_p: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Self {
        Self {
            model,
            temperature,
            top_p,
            max_tokens,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_serialization() {
        let meta = StoredMetadata::new()
            .with_generation(GenerationParams {
                model: Some("orpheus-3b".into()),
                temperature: Some(0.9),
                max_tokens: Some(2048),
                ..Default::default()
            })
            .with_metrics(Metrics {
                duration_ms: Some(5000),
                sample_rate: Some(44100),
                ..Default::default()
            })
            .with_tool("orpheus_generate");

        let json = serde_json::to_string(&meta).unwrap();
        let loaded: StoredMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, loaded);
    }

    #[test]
    fn test_empty_metadata() {
        let meta = StoredMetadata::new();
        assert!(meta.is_empty());

        let json = serde_json::to_string(&meta).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_extra_fields() {
        let meta = StoredMetadata::new()
            .with_extra("custom_field", serde_json::json!("custom_value"));

        assert!(!meta.is_empty());
        assert_eq!(
            meta.extra.get("custom_field"),
            Some(&serde_json::json!("custom_value"))
        );
    }
}
