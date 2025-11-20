//! Content Addressable Storage (CAS) for Hootenanny.
//!
//! Implements a simple, Git-like object store where files are addressed
//! by the BLAKE3 hash of their content.
//!
//! We use BLAKE3 for its speed and the ability to safely use shorter hashes
//! (16 bytes / 32 hex chars) while maintaining collision resistance.
//!
//! Layout:
//! .hootenanny/cas/objects/
//!   ab/
//!     cde123... (remainder of hash)
//! .hootenanny/cas/metadata/
//!   ab/
//!     cde123... (remainder of hash).json (stores CasMetadata)

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use crate::domain::CasReference; // Assuming CasReference is defined in domain.rs

/// Metadata stored alongside CAS objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasMetadata {
    pub mime_type: String,
    pub size: u64,
}

/// Content Addressable Storage manager.
#[derive(Debug, Clone)]
pub struct Cas {
    root: PathBuf,
    objects_dir: PathBuf,
    metadata_dir: PathBuf,
}

impl Cas {
    /// Create a new CAS interface rooted at the given directory.
    ///
    /// The actual objects will be stored in `root/objects/` and metadata in `root/metadata/`
    pub fn new(root: &Path) -> Result<Self> {
        let objects_dir = root.join("objects");
        fs::create_dir_all(&objects_dir).context("Failed to create CAS objects directory")?;

        let metadata_dir = root.join("metadata");
        fs::create_dir_all(&metadata_dir).context("Failed to create CAS metadata directory")?;

        Ok(Self {
            root: root.to_path_buf(),
            objects_dir,
            metadata_dir,
        })
    }

    /// Initialize a CAS at the default location for the project.
    ///
    /// Usually `.hootenanny/cas`.
    pub fn default_at(project_root: &Path) -> Result<Self> {
        let cas_root = project_root.join(".hootenanny").join("cas");
        Self::new(&cas_root)
    }

    /// Write data to the store with associated MIME type.
    ///
    /// Returns the truncated BLAKE3 hash of the data (32 hex chars).
    /// If the data already exists, it returns the hash without writing.
    pub fn write(&self, data: &[u8], mime_type: &str) -> Result<String> {
        let hash_bytes = blake3::hash(data);
        let hash_hex = hex::encode(&hash_bytes.as_bytes()[..16]); // Truncate to 16 bytes (128 bits)

        let (obj_dir, obj_file) = self.hash_to_object_path(&hash_hex);
        let (meta_dir, meta_file) = self.hash_to_metadata_path(&hash_hex);

        if !obj_dir.exists() {
            fs::create_dir_all(&obj_dir).context("Failed to create object subdirectory")?;
        }
        if !meta_dir.exists() {
            fs::create_dir_all(&meta_dir).context("Failed to create metadata subdirectory")?;
        }

        let obj_path = obj_dir.join(obj_file);
        if !obj_path.exists() {
            fs::write(&obj_path, data).context("Failed to write object file")?;
        }

        let metadata_path = meta_dir.join(meta_file);
        if !metadata_path.exists() {
            let metadata = CasMetadata {
                mime_type: mime_type.to_string(),
                size: data.len() as u64,
            };
            let metadata_json = serde_json::to_string(&metadata)
                .context("Failed to serialize CAS metadata")?;
            fs::write(&metadata_path, metadata_json).context("Failed to write metadata file")?;
        }

        Ok(hash_hex)
    }

    /// Read data from the store given its hash.
    pub fn read(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        self.validate_hash(hash)?;

        let (dir, file) = self.hash_to_object_path(hash);
        let path = dir.join(file);

        if path.exists() {
            let data = fs::read(&path).context("Failed to read object file")?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    /// Get the file system path for a given hash.
    ///
    /// Useful for tools that can read files directly.
    pub fn get_path(&self, hash: &str) -> Result<Option<PathBuf>> {
        self.validate_hash(hash)?;

        let (dir, file) = self.hash_to_object_path(hash);
        let path = dir.join(file);

        if path.exists() {
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    /// Inspect a CAS object, returning its metadata and path.
    pub fn inspect(&self, hash: &str) -> Result<Option<CasReference>> {
        self.validate_hash(hash)?;

        let (obj_dir, obj_file) = self.hash_to_object_path(hash);
        let (meta_dir, meta_file) = self.hash_to_metadata_path(hash);

        let obj_path = obj_dir.join(obj_file);
        let metadata_path = meta_dir.join(meta_file);

        if obj_path.exists() && metadata_path.exists() {
            let metadata_json = fs::read_to_string(&metadata_path)
                .context("Failed to read CAS metadata file")?;
            let metadata: CasMetadata = serde_json::from_str(&metadata_json)
                .context("Failed to deserialize CAS metadata")?;

            Ok(Some(CasReference {
                hash: hash.to_string(),
                mime_type: metadata.mime_type,
                size_bytes: metadata.size,
                local_path: Some(obj_path.to_string_lossy().into_owned()),
            }))
        } else {
            Ok(None)
        }
    }

    /// Helper to split hash into dir (first 2 chars) and filename (rest) for object storage.
    fn hash_to_object_path(&self, hash: &str) -> (PathBuf, String) {
        let prefix = &hash[0..2];
        let remainder = &hash[2..];
        (self.objects_dir.join(prefix), remainder.to_string())
    }

    /// Helper to split hash into dir (first 2 chars) and filename (rest) for metadata storage.
    fn hash_to_metadata_path(&self, hash: &str) -> (PathBuf, String) {
        let prefix = &hash[0..2];
        let remainder = &hash[2..];
        (self.metadata_dir.join(prefix), format!("{}.json", remainder))
    }

    /// Validates the hash format.
    fn validate_hash(&self, hash: &str) -> Result<()> {
        if hash.len() != 32 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("Invalid hash format (expected 32 hex chars)");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_write_read_inspect() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cas = Cas::new(temp_dir.path())?;

        let data = b"Hello, World!";
        let mime = "text/plain";
        let hash = cas.write(data, mime)?;

        // Verify length (32 hex chars = 16 bytes = 128 bits)
        assert_eq!(hash.len(), 32);

        let read_back = cas.read(&hash)?.expect("Should exist");
        assert_eq!(read_back, data);

        let cas_ref = cas.inspect(&hash)?.expect("Should be inspectable");
        assert_eq!(cas_ref.hash, hash);
        assert_eq!(cas_ref.mime_type, mime);
        assert_eq!(cas_ref.size_bytes, data.len() as u64);
        let local_path = cas_ref.local_path.as_ref().expect("Should have local path");
        // Path structure is .../objects/ab/cdef123... where hash is abcdef123...
        assert!(local_path.contains(&hash[0..2])); // Check directory prefix
        assert!(local_path.contains(&hash[2..])); // Check file suffix

        Ok(())
    }

    #[test]
    fn test_deduplication() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cas = Cas::new(temp_dir.path())?;

        let data = b"Duplicate Me";
        let mime = "application/octet-stream";
        let hash1 = cas.write(data, mime)?;
        let hash2 = cas.write(data, mime)?;

        assert_eq!(hash1, hash2);
        Ok(())
    }

    #[test]
    fn test_invalid_hash() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cas = Cas::new(temp_dir.path())?;

        let result = cas.read("short");
        assert!(result.is_err());
        
        let result_long = cas.read("a".repeat(64).as_str()); // SHA-256 length
        assert!(result_long.is_err()); // Should fail as we expect 32 chars

        Ok(())
    }

    #[test]
    fn test_concurrent_writes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        // Create the directory first to avoid race condition in lazy initialization
        let cas = Arc::new(Cas::new(temp_dir.path())?);
        let data = b"Concurrent Data";
        let mime = "application/octet-stream";
        // Correct BLAKE3 hash (truncated to 16 bytes/32 hex)
        let expected_hash = "5c735d76fe3537a0f35cf4a4eb14a532";
        
        let mut handles = vec![];

        for _ in 0..10 {
            let cas_clone = cas.clone();
            let handle = thread::spawn(move || {
                cas_clone.write(data, mime).expect("Write failed")
            });
            handles.push(handle);
        }

        for handle in handles {
            let hash = handle.join().unwrap();
            assert_eq!(hash, expected_hash);
        }

        // Verify content matches
        let read_back = cas.read(expected_hash)?.expect("Should exist");
        assert_eq!(read_back, data);

        Ok(())
    }

    // Note: Proper benchmarking usually requires the 'criterion' crate and a separate bench target.
    // For this context, we'll add a simple throughput test to ensure it's not egregiously slow.
    #[test]
    fn test_simple_throughput() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cas = Cas::new(temp_dir.path())?;
        let data = vec![0u8; 1024 * 1024]; // 1MB
        let mime = "application/octet-stream";

        let start = std::time::Instant::now();
        for _ in 0..10 {
            cas.write(&data, mime)?;
        }
        let duration = start.elapsed();

        println!("Wrote 10MB in {:?}", duration);
        assert!(duration.as_secs() < 5, "Should write 10MB in under 5 seconds");
        Ok(())
    }
}