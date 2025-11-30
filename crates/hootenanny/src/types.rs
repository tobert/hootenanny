//! Domain types for Hootenanny
//!
//! Rich types to avoid primitive obsession. A `ContentHash` is not just a `String`,
//! it's a BLAKE3 hash that addresses content in the CAS. An `ArtifactId` identifies
//! a creative artifact with context and history.

use serde::{Deserialize, Serialize};
use std::fmt;

/// CAS content hash (BLAKE3, 128-bit, 32 hex chars)
///
/// This is the address of content in the Content Addressable Storage.
/// Truncated to 128 bits (32 hex chars) for practical use while maintaining
/// collision resistance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ContentHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Artifact identifier
///
/// Identifies a creative artifact in the system. Artifacts have context:
/// who created them, when, what variation set they belong to, their lineage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactId(String);

impl ArtifactId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Create an artifact ID from a content hash prefix
    ///
    /// Uses the first 12 characters of the hash to create a recognizable ID.
    pub fn from_hash_prefix(hash: &ContentHash) -> Self {
        Self(format!("artifact_{}", &hash.as_str()[..12]))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ArtifactId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Variation set identifier
///
/// Groups related artifacts together. When generating multiple variations
/// of a musical idea, they share a variation set ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VariationSetId(String);

impl VariationSetId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VariationSetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for VariationSetId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_serde_roundtrip() {
        let hash = ContentHash::new("abc123def456abc123def456abc123de");
        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(json, "\"abc123def456abc123def456abc123de\"");

        let parsed: ContentHash = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, hash);
    }

    #[test]
    fn artifact_id_from_hash() {
        let hash = ContentHash::new("abc123def456abc123def456abc123de");
        let id = ArtifactId::from_hash_prefix(&hash);
        assert_eq!(id.as_str(), "artifact_abc123def456");
    }

    #[test]
    fn artifact_id_serde_roundtrip() {
        let id = ArtifactId::new("artifact_test123");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"artifact_test123\"");

        let parsed: ArtifactId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn variation_set_id_display() {
        let id = VariationSetId::new("vset_exploration_001");
        assert_eq!(format!("{}", id), "vset_exploration_001");
    }
}
