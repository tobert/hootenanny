//! Staging: Mutable content that can be sealed into immutable CAS.
//!
//! Staging files are used for in-progress writes like audio/MIDI recording.
//! They have a random ID (not content-based) and can be written to incrementally.
//! When complete, they are sealed: content is hashed and moved to the objects directory.
//!
//! Layout:
//! ```text
//! {base_path}/
//! ├── objects/
//! │   └── ab/cde123...     # Sealed content
//! ├── staging/
//! │   └── ef/gh5678...     # In-progress content
//! └── metadata/
//!     └── ...
//! ```

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::hash::ContentHash;

/// A staging ID - same format as ContentHash but generated from random data.
///
/// This allows staging files to be addressed before their content hash is known.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StagingId(String);

impl StagingId {
    /// Generate a new random staging ID.
    pub fn new() -> Self {
        let uuid = Uuid::new_v4();
        let hash_bytes = blake3::hash(uuid.as_bytes());
        let hash_hex = hex::encode(&hash_bytes.as_bytes()[..16]);
        Self(hash_hex)
    }

    /// Get the first 2 characters (used for directory sharding).
    pub fn prefix(&self) -> &str {
        &self.0[0..2]
    }

    /// Get the remainder after the prefix (used as filename).
    pub fn remainder(&self) -> &str {
        &self.0[2..]
    }

    /// Get the full ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for StagingId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for StagingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A handle to a staging file.
///
/// The file is created when the chunk is created and can be written to
/// incrementally. Call `seal()` when done to move it to the objects directory.
#[derive(Debug)]
pub struct StagingChunk {
    /// The staging ID (random, not content-based).
    pub id: StagingId,
    /// Path to the staging file.
    pub path: PathBuf,
    /// Open file handle for writing.
    file: Option<File>,
    /// Bytes written so far.
    bytes_written: u64,
}

impl StagingChunk {
    /// Create a new staging chunk at the given path.
    pub(crate) fn create(id: StagingId, path: PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("failed to create staging prefix directory")?;
        }

        let file = File::create(&path).context("failed to create staging file")?;

        Ok(Self {
            id,
            path,
            file: Some(file),
            bytes_written: 0,
        })
    }

    /// Get the staging ID.
    pub fn id(&self) -> &StagingId {
        &self.id
    }

    /// Get the path to the staging file.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get bytes written so far.
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Write data to the staging file.
    pub fn write(&mut self, data: &[u8]) -> io::Result<()> {
        if let Some(ref mut file) = self.file {
            file.write_all(data)?;
            self.bytes_written += data.len() as u64;
            Ok(())
        } else {
            Err(io::Error::other("staging file already closed"))
        }
    }

    /// Flush any buffered data to disk.
    pub fn flush(&mut self) -> io::Result<()> {
        if let Some(ref mut file) = self.file {
            file.flush()
        } else {
            Ok(())
        }
    }

    /// Sync data to disk (fsync).
    pub fn sync(&mut self) -> io::Result<()> {
        if let Some(ref mut file) = self.file {
            file.sync_all()
        } else {
            Ok(())
        }
    }

    /// Close the file handle without sealing.
    ///
    /// Use this when handing off to another process (e.g., chaosgarden)
    /// that will write via mmap.
    pub fn close(&mut self) {
        self.file = None;
    }

    /// Check if the file handle is open.
    pub fn is_open(&self) -> bool {
        self.file.is_some()
    }
}

/// Result of sealing a staging chunk.
#[derive(Debug, Clone)]
pub struct SealResult {
    /// The content hash of the sealed data.
    pub content_hash: ContentHash,
    /// Final path in the objects directory.
    pub content_path: PathBuf,
    /// Size in bytes.
    pub size_bytes: u64,
}

/// Address that can refer to either sealed content or staging.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "id")]
pub enum CasAddress {
    /// Sealed, immutable content addressed by hash.
    Content(ContentHash),
    /// Staging content addressed by random ID.
    Staging(StagingId),
}

impl CasAddress {
    /// Get the prefix for directory sharding.
    pub fn prefix(&self) -> &str {
        match self {
            CasAddress::Content(hash) => hash.prefix(),
            CasAddress::Staging(id) => id.prefix(),
        }
    }

    /// Get the remainder for filename.
    pub fn remainder(&self) -> &str {
        match self {
            CasAddress::Content(hash) => hash.remainder(),
            CasAddress::Staging(id) => id.remainder(),
        }
    }

    /// Check if this is a content (sealed) address.
    pub fn is_content(&self) -> bool {
        matches!(self, CasAddress::Content(_))
    }

    /// Check if this is a staging address.
    pub fn is_staging(&self) -> bool {
        matches!(self, CasAddress::Staging(_))
    }
}

impl std::fmt::Display for CasAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CasAddress::Content(hash) => write!(f, "content:{}", hash),
            CasAddress::Staging(id) => write!(f, "staging:{}", id),
        }
    }
}

impl From<ContentHash> for CasAddress {
    fn from(hash: ContentHash) -> Self {
        CasAddress::Content(hash)
    }
}

impl From<StagingId> for CasAddress {
    fn from(id: StagingId) -> Self {
        CasAddress::Staging(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_staging_id_format() {
        let id = StagingId::new();
        assert_eq!(id.as_str().len(), 32);
        assert!(id.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_staging_id_uniqueness() {
        let id1 = StagingId::new();
        let id2 = StagingId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_staging_id_prefix_remainder() {
        let id = StagingId::new();
        assert_eq!(id.prefix().len(), 2);
        assert_eq!(id.remainder().len(), 30);
        assert_eq!(
            format!("{}{}", id.prefix(), id.remainder()),
            id.as_str()
        );
    }

    #[test]
    fn test_cas_address_display() {
        let content = CasAddress::Content(ContentHash::from_data(b"test"));
        let staging = CasAddress::Staging(StagingId::new());

        assert!(content.to_string().starts_with("content:"));
        assert!(staging.to_string().starts_with("staging:"));
    }

    #[test]
    fn test_cas_address_is_methods() {
        let content = CasAddress::Content(ContentHash::from_data(b"test"));
        let staging = CasAddress::Staging(StagingId::new());

        assert!(content.is_content());
        assert!(!content.is_staging());
        assert!(!staging.is_content());
        assert!(staging.is_staging());
    }
}
