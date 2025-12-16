//! FileStore: Filesystem-based Content Addressable Storage.
//!
//! Implements the ContentStore trait using a local filesystem with directory sharding.
//!
//! Layout:
//! ```text
//! {base_path}/
//! ├── objects/
//! │   ├── ab/
//! │   │   └── cde123...  # Content file (remainder of hash)
//! │   └── 12/
//! │       └── 3456789...
//! └── metadata/
//!     ├── ab/
//!     │   └── cde123....json  # {mime_type, size}
//!     └── 12/
//!         └── 3456789....json
//! ```

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::CasConfig;
use crate::hash::ContentHash;
use crate::metadata::{CasMetadata, CasReference};
use crate::staging::{CasAddress, SealResult, StagingChunk, StagingId};

/// Trait for content storage backends.
///
/// This allows for alternative implementations (e.g., in-memory for testing,
/// remote storage, caching layers).
pub trait ContentStore: Send + Sync {
    /// Store data with associated MIME type, returning the content hash.
    ///
    /// If the data already exists, returns the hash without writing.
    fn store(&self, data: &[u8], mime_type: &str) -> Result<ContentHash>;

    /// Retrieve data by its content hash.
    ///
    /// Returns `Ok(None)` if the hash doesn't exist.
    fn retrieve(&self, hash: &ContentHash) -> Result<Option<Vec<u8>>>;

    /// Check if content exists without retrieving it.
    fn exists(&self, hash: &ContentHash) -> bool;

    /// Get the filesystem path for content (if available).
    ///
    /// Returns `None` for remote storage or if content doesn't exist.
    fn path(&self, hash: &ContentHash) -> Option<PathBuf>;

    /// Get full metadata about stored content.
    ///
    /// Returns `Ok(None)` if the hash doesn't exist or metadata is unavailable.
    fn inspect(&self, hash: &ContentHash) -> Result<Option<CasReference>>;
}

/// Filesystem-based content store.
#[derive(Debug, Clone)]
pub struct FileStore {
    config: CasConfig,
}

impl FileStore {
    /// Create a new FileStore with the given configuration.
    ///
    /// Creates the objects and metadata directories if they don't exist
    /// (unless in read-only mode).
    pub fn new(config: CasConfig) -> Result<Self> {
        if !config.read_only {
            fs::create_dir_all(config.objects_dir())
                .context("failed to create CAS objects directory")?;
            fs::create_dir_all(config.metadata_dir())
                .context("failed to create CAS metadata directory")?;
        }

        Ok(Self { config })
    }

    /// Create a FileStore at a specific path.
    pub fn at_path(path: impl Into<PathBuf>) -> Result<Self> {
        Self::new(CasConfig::with_base_path(path))
    }

    /// Create a read-only FileStore at a specific path.
    ///
    /// Useful for chaosgarden which only needs to read content.
    pub fn read_only_at(path: impl Into<PathBuf>) -> Result<Self> {
        Self::new(CasConfig::read_only(path))
    }

    /// Get the configuration.
    pub fn config(&self) -> &CasConfig {
        &self.config
    }

    /// Get the path where an object would be stored.
    fn object_path(&self, hash: &ContentHash) -> PathBuf {
        self.config
            .objects_dir()
            .join(hash.prefix())
            .join(hash.remainder())
    }

    /// Get the path where metadata would be stored.
    fn metadata_path(&self, hash: &ContentHash) -> PathBuf {
        self.config
            .metadata_dir()
            .join(hash.prefix())
            .join(format!("{}.json", hash.remainder()))
    }

    /// Get the path where a staging file would be stored.
    fn staging_path(&self, id: &StagingId) -> PathBuf {
        self.config
            .staging_dir()
            .join(id.prefix())
            .join(id.remainder())
    }

    /// Create a new staging chunk for incremental writes.
    ///
    /// Returns a handle that can be written to. Call `seal()` when done
    /// to compute the content hash and move to the objects directory.
    pub fn create_staging(&self) -> Result<StagingChunk> {
        if self.config.read_only {
            anyhow::bail!("CAS is in read-only mode");
        }

        let id = StagingId::new();
        let path = self.staging_path(&id);
        StagingChunk::create(id, path)
    }

    /// Create a staging chunk with a specific ID.
    ///
    /// Useful when the ID needs to be known before creation (e.g., for coordination).
    pub fn create_staging_with_id(&self, id: StagingId) -> Result<StagingChunk> {
        if self.config.read_only {
            anyhow::bail!("CAS is in read-only mode");
        }

        let path = self.staging_path(&id);
        StagingChunk::create(id, path)
    }

    /// Get the path for a staging file by ID.
    pub fn staging_path_for(&self, id: &StagingId) -> PathBuf {
        self.staging_path(id)
    }

    /// Seal a staging chunk: compute hash and move to objects directory.
    ///
    /// This attempts a rename() first (O(1) on same filesystem).
    /// Falls back to copy+delete if cross-filesystem.
    pub fn seal(&self, chunk: &StagingChunk, mime_type: &str) -> Result<SealResult> {
        self.seal_path(&chunk.path, mime_type)
    }

    /// Seal a staging file by path.
    ///
    /// Use this when the staging file was written by another process (e.g., chaosgarden).
    pub fn seal_path(&self, staging_path: &PathBuf, mime_type: &str) -> Result<SealResult> {
        if self.config.read_only {
            anyhow::bail!("CAS is in read-only mode");
        }

        // Read and hash the content
        let data = fs::read(staging_path).context("failed to read staging file")?;
        let content_hash = ContentHash::from_data(&data);
        let size_bytes = data.len() as u64;

        let obj_path = self.object_path(&content_hash);

        // Create prefix directory if needed
        if let Some(parent) = obj_path.parent() {
            fs::create_dir_all(parent).context("failed to create object prefix directory")?;
        }

        // Try rename first (O(1) on same filesystem)
        if !obj_path.exists() {
            match fs::rename(staging_path, &obj_path) {
                Ok(()) => {}
                Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
                    // Cross-filesystem: fall back to copy + delete
                    fs::copy(staging_path, &obj_path).context("failed to copy staging file")?;
                    fs::remove_file(staging_path).context("failed to remove staging file")?;
                }
                Err(e) => {
                    return Err(e).context("failed to rename staging file");
                }
            }
        } else {
            // Content already exists (dedup), just remove staging
            fs::remove_file(staging_path).context("failed to remove staging file")?;
        }

        // Write metadata if configured
        if self.config.store_metadata {
            let meta_path = self.metadata_path(&content_hash);
            if let Some(parent) = meta_path.parent() {
                fs::create_dir_all(parent).context("failed to create metadata prefix directory")?;
            }

            if !meta_path.exists() {
                let metadata = CasMetadata {
                    mime_type: mime_type.to_string(),
                    size: size_bytes,
                };
                let json = serde_json::to_string(&metadata).context("failed to serialize metadata")?;
                fs::write(&meta_path, json).context("failed to write metadata file")?;
            }
        }

        Ok(SealResult {
            content_hash,
            content_path: obj_path,
            size_bytes,
        })
    }

    /// Check if a staging file exists.
    pub fn staging_exists(&self, id: &StagingId) -> bool {
        self.staging_path(id).exists()
    }

    /// Get the path for an address (content or staging).
    pub fn address_path(&self, address: &CasAddress) -> Option<PathBuf> {
        match address {
            CasAddress::Content(hash) => self.path(hash),
            CasAddress::Staging(id) => {
                let path = self.staging_path(id);
                if path.exists() {
                    Some(path)
                } else {
                    None
                }
            }
        }
    }

    /// Remove a staging file (cleanup).
    pub fn remove_staging(&self, id: &StagingId) -> Result<()> {
        let path = self.staging_path(id);
        if path.exists() {
            fs::remove_file(&path).context("failed to remove staging file")?;
        }
        Ok(())
    }
}

impl ContentStore for FileStore {
    fn store(&self, data: &[u8], mime_type: &str) -> Result<ContentHash> {
        if self.config.read_only {
            anyhow::bail!("CAS is in read-only mode");
        }

        let hash = ContentHash::from_data(data);
        let obj_path = self.object_path(&hash);
        let meta_path = self.metadata_path(&hash);

        // Create prefix directories if needed
        if let Some(parent) = obj_path.parent() {
            fs::create_dir_all(parent).context("failed to create object prefix directory")?;
        }

        // Write object (skip if exists - content-addressed = idempotent)
        if !obj_path.exists() {
            fs::write(&obj_path, data).context("failed to write object file")?;
        }

        // Write metadata if configured
        if self.config.store_metadata {
            if let Some(parent) = meta_path.parent() {
                fs::create_dir_all(parent).context("failed to create metadata prefix directory")?;
            }

            if !meta_path.exists() {
                let metadata = CasMetadata {
                    mime_type: mime_type.to_string(),
                    size: data.len() as u64,
                };
                let json = serde_json::to_string(&metadata).context("failed to serialize metadata")?;
                fs::write(&meta_path, json).context("failed to write metadata file")?;
            }
        }

        Ok(hash)
    }

    fn retrieve(&self, hash: &ContentHash) -> Result<Option<Vec<u8>>> {
        let path = self.object_path(hash);

        if path.exists() {
            let data = fs::read(&path).context("failed to read object file")?;
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    fn exists(&self, hash: &ContentHash) -> bool {
        self.object_path(hash).exists()
    }

    fn path(&self, hash: &ContentHash) -> Option<PathBuf> {
        let path = self.object_path(hash);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    fn inspect(&self, hash: &ContentHash) -> Result<Option<CasReference>> {
        let obj_path = self.object_path(hash);
        let meta_path = self.metadata_path(hash);

        if !obj_path.exists() {
            return Ok(None);
        }

        // Try to read metadata
        if meta_path.exists() {
            let json = fs::read_to_string(&meta_path).context("failed to read metadata file")?;
            let metadata: CasMetadata =
                serde_json::from_str(&json).context("failed to parse metadata")?;

            Ok(Some(
                CasReference::new(hash.clone(), metadata.mime_type, metadata.size)
                    .with_path(obj_path.to_string_lossy()),
            ))
        } else {
            // No metadata - infer size from file, use generic mime type
            let file_size = fs::metadata(&obj_path)
                .context("failed to stat object file")?
                .len();

            Ok(Some(
                CasReference::new(hash.clone(), "application/octet-stream", file_size)
                    .with_path(obj_path.to_string_lossy()),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use tempfile::TempDir;

    #[test]
    fn test_store_and_retrieve() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let data = b"Hello, World!";
        let mime = "text/plain";
        let hash = store.store(data, mime)?;

        // Verify hash format
        assert_eq!(hash.as_str().len(), 32);

        // Retrieve
        let retrieved = store.retrieve(&hash)?.expect("should exist");
        assert_eq!(retrieved, data);

        Ok(())
    }

    #[test]
    fn test_inspect() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let data = b"Inspectable content";
        let mime = "application/json";
        let hash = store.store(data, mime)?;

        let reference = store.inspect(&hash)?.expect("should be inspectable");
        assert_eq!(reference.hash, hash);
        assert_eq!(reference.mime_type, mime);
        assert_eq!(reference.size_bytes, data.len() as u64);
        assert!(reference.local_path.is_some());

        Ok(())
    }

    #[test]
    fn test_deduplication() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let data = b"Duplicate Me";
        let hash1 = store.store(data, "text/plain")?;
        let hash2 = store.store(data, "text/plain")?;

        assert_eq!(hash1, hash2);
        Ok(())
    }

    #[test]
    fn test_exists() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let hash = store.store(b"existence test", "text/plain")?;
        assert!(store.exists(&hash));

        let missing_hash: ContentHash = "00000000000000000000000000000000".parse()?;
        assert!(!store.exists(&missing_hash));

        Ok(())
    }

    #[test]
    fn test_path() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let hash = store.store(b"path test", "text/plain")?;
        let path = store.path(&hash).expect("should have path");

        // Path should contain the hash prefix and remainder
        let path_str = path.to_string_lossy();
        assert!(path_str.contains(hash.prefix()));
        assert!(path_str.contains(hash.remainder()));

        Ok(())
    }

    #[test]
    fn test_read_only_prevents_writes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::read_only_at(temp_dir.path())?;

        let result = store.store(b"should fail", "text/plain");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("read-only"));

        Ok(())
    }

    #[test]
    fn test_read_only_allows_reads() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // First write with a writable store
        let writable = FileStore::at_path(temp_dir.path())?;
        let hash = writable.store(b"readable content", "text/plain")?;

        // Then read with a read-only store
        let readonly = FileStore::read_only_at(temp_dir.path())?;
        let data = readonly.retrieve(&hash)?.expect("should be readable");
        assert_eq!(data, b"readable content");

        Ok(())
    }

    #[test]
    fn test_concurrent_writes() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = Arc::new(FileStore::at_path(temp_dir.path())?);

        let data = b"Concurrent Data";
        let mime = "application/octet-stream";
        let expected_hash: ContentHash = "5c735d76fe3537a0f35cf4a4eb14a532".parse()?;

        let mut handles = vec![];

        for _ in 0..10 {
            let store_clone = store.clone();
            let handle = thread::spawn(move || store_clone.store(data, mime).expect("write failed"));
            handles.push(handle);
        }

        for handle in handles {
            let hash = handle.join().unwrap();
            assert_eq!(hash, expected_hash);
        }

        // Verify content
        let retrieved = store.retrieve(&expected_hash)?.expect("should exist");
        assert_eq!(retrieved, data);

        Ok(())
    }

    #[test]
    fn test_inspect_without_metadata() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create store that doesn't write metadata
        let config = CasConfig {
            base_path: temp_dir.path().to_path_buf(),
            store_metadata: false,
            read_only: false,
        };
        let store = FileStore::new(config)?;

        let hash = store.store(b"no metadata", "text/plain")?;

        // Inspect should still work, but with generic mime type
        let reference = store.inspect(&hash)?.expect("should exist");
        assert_eq!(reference.hash, hash);
        assert_eq!(reference.mime_type, "application/octet-stream");
        assert_eq!(reference.size_bytes, 11);

        Ok(())
    }

    #[test]
    fn test_throughput() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let data = vec![0u8; 1024 * 1024]; // 1MB
        let mime = "application/octet-stream";

        let start = std::time::Instant::now();
        for _ in 0..10 {
            store.store(&data, mime)?;
        }
        let duration = start.elapsed();

        println!("Wrote 10MB in {:?}", duration);
        assert!(
            duration.as_secs() < 5,
            "should write 10MB in under 5 seconds"
        );

        Ok(())
    }

    #[test]
    fn test_staging_create_and_write() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let mut chunk = store.create_staging()?;

        // Write some data
        chunk.write(b"Hello, ")?;
        chunk.write(b"World!")?;
        chunk.flush()?;

        assert_eq!(chunk.bytes_written(), 13);
        assert!(chunk.path().exists());
        assert!(store.staging_exists(chunk.id()));

        Ok(())
    }

    #[test]
    fn test_staging_seal() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let mut chunk = store.create_staging()?;
        chunk.write(b"Seal me!")?;
        chunk.flush()?;

        let staging_id = chunk.id().clone();
        let staging_path = chunk.path().clone();

        // Seal it
        let result = store.seal(&chunk, "text/plain")?;

        // Staging file should be gone
        assert!(!staging_path.exists());
        assert!(!store.staging_exists(&staging_id));

        // Content should exist
        assert!(store.exists(&result.content_hash));
        let data = store.retrieve(&result.content_hash)?.expect("should exist");
        assert_eq!(data, b"Seal me!");
        assert_eq!(result.size_bytes, 8);

        Ok(())
    }

    #[test]
    fn test_staging_seal_dedup() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let data = b"Duplicate staging content";

        // Store via normal path first
        let hash1 = store.store(data, "text/plain")?;

        // Now stage the same content
        let mut chunk = store.create_staging()?;
        chunk.write(data)?;
        chunk.flush()?;

        // Seal should recognize duplicate and clean up staging
        let result = store.seal(&chunk, "text/plain")?;

        assert_eq!(result.content_hash, hash1);
        assert!(!chunk.path().exists()); // Staging cleaned up

        Ok(())
    }

    #[test]
    fn test_staging_with_id() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        // Create ID ahead of time
        let id = StagingId::new();
        let expected_path = store.staging_path_for(&id);

        let chunk = store.create_staging_with_id(id.clone())?;

        assert_eq!(chunk.id(), &id);
        assert_eq!(chunk.path(), &expected_path);

        Ok(())
    }

    #[test]
    fn test_staging_address_path() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        // Create some content
        let hash = store.store(b"content", "text/plain")?;

        // Create staging
        let mut chunk = store.create_staging()?;
        chunk.write(b"staging")?;
        chunk.flush()?;

        // Check address_path works for both
        let content_addr = CasAddress::Content(hash.clone());
        let staging_addr = CasAddress::Staging(chunk.id().clone());

        assert!(store.address_path(&content_addr).is_some());
        assert!(store.address_path(&staging_addr).is_some());

        Ok(())
    }

    #[test]
    fn test_staging_remove() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = FileStore::at_path(temp_dir.path())?;

        let mut chunk = store.create_staging()?;
        chunk.write(b"to be removed")?;
        chunk.flush()?;

        let id = chunk.id().clone();
        assert!(store.staging_exists(&id));

        store.remove_staging(&id)?;
        assert!(!store.staging_exists(&id));

        Ok(())
    }
}
