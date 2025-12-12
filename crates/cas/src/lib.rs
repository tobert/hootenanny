//! Content Addressable Storage (CAS) for halfremembered.
//!
//! A shared crate providing content-addressed storage that can be used by:
//! - **hootenanny** (control plane): writes artifacts, stores metadata
//! - **chaosgarden** (RT audio): reads content by hash
//! - **workers**: stores generated content
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use cas::{FileStore, ContentStore, CasConfig};
//!
//! // Create from environment (reads HALFREMEMBERED_CAS_PATH)
//! let config = CasConfig::from_env().unwrap();
//! let store = FileStore::new(config).unwrap();
//!
//! // Or at a specific path
//! let store = FileStore::at_path("/tank/halfremembered/cas").unwrap();
//!
//! // Store content
//! let hash = store.store(b"Hello, World!", "text/plain").unwrap();
//! println!("Stored as: {}", hash);
//!
//! // Retrieve content
//! if let Some(data) = store.retrieve(&hash).unwrap() {
//!     println!("Got {} bytes", data.len());
//! }
//!
//! // Get filesystem path (for external tools)
//! if let Some(path) = store.path(&hash) {
//!     println!("File at: {}", path.display());
//! }
//! ```
//!
//! # Configuration
//!
//! Environment variables:
//! - `HALFREMEMBERED_CAS_PATH`: Base path for storage (default: `~/.halfremembered/cas`)
//! - `HALFREMEMBERED_CAS_READONLY`: Set to "true" for read-only mode
//!
//! # NFS / Shared Storage
//!
//! CAS is designed for shared filesystems:
//! - Content is write-once (content-addressed = no conflicts)
//! - Writers (hootenanny, workers) create content
//! - Readers (chaosgarden) only need read access
//! - No locking required

pub mod config;
pub mod hash;
pub mod metadata;
pub mod store;

// Re-exports for convenience
pub use config::CasConfig;
pub use hash::{ContentHash, HashError};
pub use metadata::{CasMetadata, CasReference};
pub use store::{ContentStore, FileStore};
