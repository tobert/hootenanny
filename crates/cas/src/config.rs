//! CAS configuration with environment variable and file-based loading.
//!
//! Environment variables:
//! - `HOOTENANNY_CAS_PATH`: Base path for CAS storage
//! - `HOOTENANNY_CAS_READONLY`: Set to "true" for read-only mode
//!
//! Default path: `~/.hootenanny/cas`

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};

/// Configuration for Content Addressable Storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CasConfig {
    /// Base path for CAS storage.
    /// Objects stored in `{base_path}/objects/`, metadata in `{base_path}/metadata/`.
    pub base_path: PathBuf,

    /// Whether to write metadata JSON alongside objects.
    /// Set to false for faster writes when metadata isn't needed.
    #[serde(default = "default_true")]
    pub store_metadata: bool,

    /// Read-only mode - prevents any writes.
    /// Useful for chaosgarden which only reads content.
    #[serde(default)]
    pub read_only: bool,
}

fn default_true() -> bool {
    true
}

impl Default for CasConfig {
    fn default() -> Self {
        Self {
            base_path: default_cas_path(),
            store_metadata: true,
            read_only: false,
        }
    }
}

/// Get the default CAS path (~/.hootenanny/cas).
fn default_cas_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".hootenanny").join("cas"))
        .unwrap_or_else(|| PathBuf::from(".hootenanny/cas"))
}

impl CasConfig {
    /// Load configuration from environment variables, falling back to defaults.
    ///
    /// Environment variables:
    /// - `HOOTENANNY_CAS_PATH`: Override the base path
    /// - `HOOTENANNY_CAS_READONLY`: Set to "true" for read-only mode
    pub fn from_env() -> Result<Self> {
        let base_path = env::var("HOOTENANNY_CAS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_cas_path());

        let read_only = env::var("HOOTENANNY_CAS_READONLY")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(false);

        Ok(Self {
            base_path,
            store_metadata: true,
            read_only,
        })
    }

    /// Load configuration from a TOML file, falling back to environment.
    ///
    /// The file should contain a `[cas]` section:
    /// ```toml
    /// [cas]
    /// base_path = "/tank/hootenanny/cas"
    /// store_metadata = true
    /// read_only = false
    /// ```
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;

        // Parse as TOML table, look for [cas] section
        let table: toml::Table = contents
            .parse()
            .with_context(|| format!("failed to parse TOML: {}", path.display()))?;

        if let Some(cas_section) = table.get("cas") {
            let config: CasConfig = cas_section
                .clone()
                .try_into()
                .context("failed to parse [cas] section")?;
            Ok(config)
        } else {
            // No [cas] section, fall back to env
            Self::from_env()
        }
    }

    /// Create a config with a specific base path.
    pub fn with_base_path(path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: path.into(),
            store_metadata: true,
            read_only: false,
        }
    }

    /// Create a read-only config with a specific base path.
    pub fn read_only(path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: path.into(),
            store_metadata: false,
            read_only: true,
        }
    }

    /// Get the objects directory path.
    pub fn objects_dir(&self) -> PathBuf {
        self.base_path.join("objects")
    }

    /// Get the metadata directory path.
    pub fn metadata_dir(&self) -> PathBuf {
        self.base_path.join("metadata")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CasConfig::default();
        assert!(config.base_path.to_string_lossy().contains(".hootenanny"));
        assert!(config.store_metadata);
        assert!(!config.read_only);
    }

    #[test]
    fn test_with_base_path() {
        let config = CasConfig::with_base_path("/custom/path");
        assert_eq!(config.base_path, PathBuf::from("/custom/path"));
        assert!(config.store_metadata);
        assert!(!config.read_only);
    }

    #[test]
    fn test_read_only_config() {
        let config = CasConfig::read_only("/tank/cas");
        assert_eq!(config.base_path, PathBuf::from("/tank/cas"));
        assert!(!config.store_metadata);
        assert!(config.read_only);
    }

    #[test]
    fn test_objects_and_metadata_dirs() {
        let config = CasConfig::with_base_path("/test/cas");
        assert_eq!(config.objects_dir(), PathBuf::from("/test/cas/objects"));
        assert_eq!(config.metadata_dir(), PathBuf::from("/test/cas/metadata"));
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = CasConfig {
            base_path: PathBuf::from("/custom/cas"),
            store_metadata: false,
            read_only: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: CasConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.base_path, restored.base_path);
        assert_eq!(config.store_metadata, restored.store_metadata);
        assert_eq!(config.read_only, restored.read_only);
    }

    #[test]
    fn test_from_env_uses_defaults() {
        // Clear any existing env vars for predictable test
        env::remove_var("HOOTENANNY_CAS_PATH");
        env::remove_var("HOOTENANNY_CAS_READONLY");

        let config = CasConfig::from_env().unwrap();
        assert!(config.base_path.to_string_lossy().contains(".hootenanny"));
        assert!(!config.read_only);
    }
}
