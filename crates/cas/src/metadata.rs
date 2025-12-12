//! Metadata types for CAS objects.
//!
//! Each object in the CAS can have associated metadata stored in a JSON sidecar file.
//! This allows quick lookup of MIME type and size without reading the actual content.

use crate::hash::ContentHash;
use serde::{Deserialize, Serialize};

/// Metadata stored alongside CAS objects.
///
/// Stored as JSON in the metadata directory with the same prefix/remainder structure
/// as the object itself, but with a `.json` extension.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CasMetadata {
    /// MIME type of the content (e.g., "audio/wav", "audio/midi").
    pub mime_type: String,

    /// Size of the content in bytes.
    pub size: u64,
}

/// Reference to content in the CAS, combining hash with metadata.
///
/// This is what gets returned from `inspect()` - everything you need to know
/// about a stored object without reading its content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasReference {
    /// The content hash identifying this object.
    pub hash: ContentHash,

    /// MIME type of the content.
    pub mime_type: String,

    /// Size in bytes.
    pub size_bytes: u64,

    /// Local filesystem path to the content (if available).
    /// May be `None` for remote CAS or if path shouldn't be exposed.
    pub local_path: Option<String>,
}

impl CasReference {
    /// Create a new CAS reference.
    pub fn new(hash: ContentHash, mime_type: impl Into<String>, size_bytes: u64) -> Self {
        Self {
            hash,
            mime_type: mime_type.into(),
            size_bytes,
            local_path: None,
        }
    }

    /// Add a local path to this reference.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.local_path = Some(path.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cas_metadata_serde() {
        let meta = CasMetadata {
            mime_type: "audio/wav".to_string(),
            size: 48000,
        };

        let json = serde_json::to_string(&meta).unwrap();
        let restored: CasMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta, restored);
    }

    #[test]
    fn test_cas_reference_new() {
        let hash = ContentHash::from_data(b"test");
        let reference = CasReference::new(hash.clone(), "text/plain", 4);

        assert_eq!(reference.hash, hash);
        assert_eq!(reference.mime_type, "text/plain");
        assert_eq!(reference.size_bytes, 4);
        assert!(reference.local_path.is_none());
    }

    #[test]
    fn test_cas_reference_with_path() {
        let hash = ContentHash::from_data(b"test");
        let reference = CasReference::new(hash, "text/plain", 4).with_path("/tmp/cas/ab/cdef");

        assert_eq!(reference.local_path, Some("/tmp/cas/ab/cdef".to_string()));
    }

    #[test]
    fn test_cas_reference_serde() {
        let hash = ContentHash::from_data(b"serde test");
        let reference = CasReference::new(hash, "application/json", 100).with_path("/path/to/file");

        let json = serde_json::to_string(&reference).unwrap();
        let restored: CasReference = serde_json::from_str(&json).unwrap();

        assert_eq!(reference.hash, restored.hash);
        assert_eq!(reference.mime_type, restored.mime_type);
        assert_eq!(reference.size_bytes, restored.size_bytes);
        assert_eq!(reference.local_path, restored.local_path);
    }
}
