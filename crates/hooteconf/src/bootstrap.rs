//! Bootstrap configuration - seeds runtime state, then runtime owns it.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Model service endpoints.
///
/// Maps model name to URL. Runtime may discover additional models
/// or lose connectivity to these - this just seeds the initial state.
pub type ModelsConfig = HashMap<String, String>;

/// Default model endpoints for bootstrap.
pub fn default_models() -> ModelsConfig {
    let mut models = HashMap::new();
    models.insert("orpheus".to_string(), "http://127.0.0.1:2000".to_string());
    models.insert("orpheus_classifier".to_string(), "http://127.0.0.1:2001".to_string());
    models.insert("orpheus_bridge".to_string(), "http://127.0.0.1:2002".to_string());
    models.insert("orpheus_loops".to_string(), "http://127.0.0.1:2003".to_string());
    models.insert("musicgen".to_string(), "http://127.0.0.1:2006".to_string());
    models.insert("clap".to_string(), "http://127.0.0.1:2007".to_string());
    models.insert("yue".to_string(), "http://127.0.0.1:2008".to_string());
    models.insert("beatthis".to_string(), "http://127.0.0.1:2012".to_string());
    models.insert("gpu_observer".to_string(), "http://127.0.0.1:2099".to_string());
    models
}

/// Connection endpoints for other Hootenanny services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionsConfig {
    /// Chaosgarden endpoint: "local" for IPC, or "tcp://host:port"
    #[serde(default = "ConnectionsConfig::default_chaosgarden")]
    pub chaosgarden: String,

    /// Vibeweaver ZMQ endpoint (Python kernel for AI music agents)
    #[serde(default = "ConnectionsConfig::default_vibeweaver")]
    pub vibeweaver: String,

    /// RAVE ZMQ endpoint (RAVE audio codec service via hootpy)
    #[serde(default = "ConnectionsConfig::default_rave")]
    pub rave: String,
}

impl ConnectionsConfig {
    fn default_chaosgarden() -> String {
        "local".to_string()
    }

    fn default_vibeweaver() -> String {
        "tcp://localhost:5575".to_string()
    }

    fn default_rave() -> String {
        // Empty string means disabled (RAVE is optional)
        String::new()
    }
}

impl Default for ConnectionsConfig {
    fn default() -> Self {
        Self {
            chaosgarden: Self::default_chaosgarden(),
            vibeweaver: Self::default_vibeweaver(),
            rave: Self::default_rave(),
        }
    }
}

/// Media directories for SoundFonts and samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaConfig {
    /// Directories to search for SoundFonts (.sf2, .sf3)
    #[serde(default = "MediaConfig::default_soundfont_dirs")]
    pub soundfont_dirs: Vec<PathBuf>,

    /// Directories to search for samples (.wav, .flac, etc)
    #[serde(default = "MediaConfig::default_sample_dirs")]
    pub sample_dirs: Vec<PathBuf>,
}

impl MediaConfig {
    fn default_soundfont_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // User's custom directory
        if let Some(base) = directories::BaseDirs::new() {
            dirs.push(base.home_dir().join("midi/SF2"));
        }

        // System SoundFonts
        dirs.push(PathBuf::from("/usr/share/sounds/sf2"));

        dirs
    }

    fn default_sample_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        if let Some(base) = directories::BaseDirs::new() {
            dirs.push(base.home_dir().join("samples"));
        }

        dirs
    }
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            soundfont_dirs: Self::default_soundfont_dirs(),
            sample_dirs: Self::default_sample_dirs(),
        }
    }
}

/// Default runtime policies and limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    /// Lua script execution timeout
    #[serde(default = "DefaultsConfig::default_lua_timeout")]
    pub lua_timeout: String,

    /// Session expiration time
    #[serde(default = "DefaultsConfig::default_session_expiration")]
    pub session_expiration: String,

    /// Maximum concurrent background jobs
    #[serde(default = "DefaultsConfig::default_max_concurrent_jobs")]
    pub max_concurrent_jobs: u32,
}

impl DefaultsConfig {
    fn default_lua_timeout() -> String {
        "30s".to_string()
    }

    fn default_session_expiration() -> String {
        "5m".to_string()
    }

    fn default_max_concurrent_jobs() -> u32 {
        4
    }
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            lua_timeout: Self::default_lua_timeout(),
            session_expiration: Self::default_session_expiration(),
            max_concurrent_jobs: Self::default_max_concurrent_jobs(),
        }
    }
}

/// Bootstrap configuration - seeds runtime, then runtime owns it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// Model service endpoints
    #[serde(default = "default_models")]
    pub models: ModelsConfig,

    /// Connection endpoints
    #[serde(default)]
    pub connections: ConnectionsConfig,

    /// Media directories
    #[serde(default)]
    pub media: MediaConfig,

    /// Default policies
    #[serde(default)]
    pub defaults: DefaultsConfig,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            models: default_models(),
            connections: ConnectionsConfig::default(),
            media: MediaConfig::default(),
            defaults: DefaultsConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_models() {
        let models = default_models();
        assert_eq!(models.get("orpheus"), Some(&"http://127.0.0.1:2000".to_string()));
        assert_eq!(models.get("beatthis"), Some(&"http://127.0.0.1:2012".to_string()));
        assert_eq!(models.len(), 9);
    }

    #[test]
    fn test_connections_defaults() {
        let conn = ConnectionsConfig::default();
        assert_eq!(conn.chaosgarden, "local");
        assert_eq!(conn.vibeweaver, "tcp://localhost:5575");
    }

    #[test]
    fn test_media_defaults() {
        let media = MediaConfig::default();
        assert!(!media.soundfont_dirs.is_empty());
        assert!(media.soundfont_dirs.iter().any(|p| p.to_string_lossy().contains("SF2") || p.to_string_lossy().contains("sf2")));
    }

    #[test]
    fn test_defaults_config() {
        let defaults = DefaultsConfig::default();
        assert_eq!(defaults.lua_timeout, "30s");
        assert_eq!(defaults.session_expiration, "5m");
        assert_eq!(defaults.max_concurrent_jobs, 4);
    }
}
