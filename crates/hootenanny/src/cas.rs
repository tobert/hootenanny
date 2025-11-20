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

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Content Addressable Storage manager.
#[derive(Debug, Clone)]
pub struct Cas {
    root: PathBuf,
}

impl Cas {
    /// Create a new CAS interface rooted at the given directory.
    ///
    /// The actual objects will be stored in `root/objects/`.
    pub fn new(root: &Path) -> Result<Self> {
        let objects_dir = root.join("objects");
        fs::create_dir_all(&objects_dir).context("Failed to create CAS objects directory")?;
        Ok(Self { root: root.to_path_buf() })
    }

    /// Initialize a CAS at the default location for the project.
    ///
    /// Usually `.hootenanny/cas`.
    pub fn default_at(project_root: &Path) -> Result<Self> {
        let cas_root = project_root.join(".hootenanny").join("cas");
        Self::new(&cas_root)
    }

    /// Write data to the store.
    ///
    /// Returns the truncated BLAKE3 hash of the data (32 hex chars).
    /// If the data already exists, it returns the hash without writing.
    pub fn write(&self, data: &[u8]) -> Result<String> {
        let hash_bytes = blake3::hash(data);
        let hash_hex = hex::encode(&hash_bytes.as_bytes()[..16]); // Truncate to 16 bytes (128 bits)

        let (dir, file) = self.hash_to_path(&hash_hex);
        
        if !dir.exists() {
            fs::create_dir_all(&dir).context("Failed to create object subdirectory")?;
        }

        let path = dir.join(file);
        if !path.exists() {
            // Use atomic write ideally, but for now direct write is fine for prototype
            fs::write(&path, data).context("Failed to write object file")?;
        }

        Ok(hash_hex)
    }

    /// Read data from the store given its hash.
    pub fn read(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        // Validate hash format (32 chars hex)
        if hash.len() != 32 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("Invalid hash format (expected 32 hex chars)");
        }

        let (dir, file) = self.hash_to_path(hash);
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
         if hash.len() != 32 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("Invalid hash format (expected 32 hex chars)");
        }

        let (dir, file) = self.hash_to_path(hash);
        let path = dir.join(file);

        if path.exists() {
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    /// Helper to split hash into dir (first 2 chars) and filename (rest).
    fn hash_to_path(&self, hash: &str) -> (PathBuf, String) {
        let prefix = &hash[0..2];
        let remainder = &hash[2..];
        (self.root.join("objects").join(prefix), remainder.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_write_read() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cas = Cas::new(temp_dir.path())?;

        let data = b"Hello, World!";
        let hash = cas.write(data)?;

        // Verify length (32 hex chars = 16 bytes = 128 bits)
        assert_eq!(hash.len(), 32);

        let read_back = cas.read(&hash)?.expect("Should exist");
        assert_eq!(read_back, data);

        Ok(())
    }

    #[test]
    fn test_deduplication() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cas = Cas::new(temp_dir.path())?;

        let data = b"Duplicate Me";
        let hash1 = cas.write(data)?;
        let hash2 = cas.write(data)?;

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
        // Correct BLAKE3 hash (truncated to 16 bytes/32 hex)
        let expected_hash = "5c735d76fe3537a0f35cf4a4eb14a532";
        
        let mut handles = vec![];

        for _ in 0..10 {
            let cas_clone = cas.clone();
            let handle = thread::spawn(move || {
                cas_clone.write(data).expect("Write failed")
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

        let start = std::time::Instant::now();
        for _ in 0..10 {
            cas.write(&data)?;
        }
        let duration = start.elapsed();

        println!("Wrote 10MB in {:?}", duration);
        assert!(duration.as_secs() < 5, "Should write 10MB in under 5 seconds");
        Ok(())
    }
}
