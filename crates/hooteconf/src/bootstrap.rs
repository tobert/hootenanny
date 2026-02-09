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
    // All model services now use ZMQ (see connections config).
    // Only gpu_observer remains HTTP — it's infrastructure, not a model path.
    models.insert("gpu_observer".to_string(), "http://127.0.0.1:2099".to_string());
    models
}

/// Connection endpoints for other Hootenanny services.
///
/// All Python model services use IPC sockets by default:
/// - Socket directory: `$XDG_RUNTIME_DIR/hootenanny/` or `~/.hootenanny/run/`
/// - Services create the directory on startup if needed
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

    /// RAVE streaming ZMQ endpoint (realtime audio processing)
    #[serde(default = "ConnectionsConfig::default_rave_streaming")]
    pub rave_streaming: String,

    /// Orpheus ZMQ endpoint (MIDI generation service via hootpy)
    #[serde(default = "ConnectionsConfig::default_orpheus")]
    pub orpheus: String,

    /// Beat-this ZMQ endpoint (beat/downbeat detection via hootpy)
    #[serde(default = "ConnectionsConfig::default_beatthis")]
    pub beatthis: String,

    /// MusicGen ZMQ endpoint (text-to-music generation via hootpy)
    #[serde(default = "ConnectionsConfig::default_musicgen")]
    pub musicgen: String,

    /// CLAP ZMQ endpoint (audio analysis via hootpy)
    #[serde(default = "ConnectionsConfig::default_clap")]
    pub clap: String,

    /// AudioLDM2 ZMQ endpoint (text-to-audio diffusion via hootpy)
    #[serde(default = "ConnectionsConfig::default_audioldm2")]
    pub audioldm2: String,

    /// Anticipatory Music Transformer ZMQ endpoint (MIDI generation via hootpy)
    #[serde(default = "ConnectionsConfig::default_anticipatory")]
    pub anticipatory: String,

    /// Demucs ZMQ endpoint (audio separation via hootpy)
    #[serde(default = "ConnectionsConfig::default_demucs")]
    pub demucs: String,

    /// YuE ZMQ endpoint (text-to-song generation via hootpy)
    #[serde(default = "ConnectionsConfig::default_yue")]
    pub yue: String,

    /// MIDI role classifier ZMQ endpoint (voice role classification via hootpy)
    #[serde(default = "ConnectionsConfig::default_midi_role")]
    pub midi_role: String,
}

impl ConnectionsConfig {
    /// Get the IPC socket directory path.
    fn socket_dir() -> String {
        std::env::var("XDG_RUNTIME_DIR")
            .map(|dir| format!("{}/hootenanny", dir))
            .unwrap_or_else(|_| {
                directories::BaseDirs::new()
                    .map(|base| {
                        base.home_dir()
                            .join(".hootenanny/run")
                            .to_string_lossy()
                            .into_owned()
                    })
                    .unwrap_or_else(|| "/tmp/hootenanny".to_string())
            })
    }

    fn default_chaosgarden() -> String {
        "local".to_string()
    }

    fn default_vibeweaver() -> String {
        "tcp://localhost:5575".to_string()
    }

    fn default_rave() -> String {
        format!("ipc://{}/rave.sock", Self::socket_dir())
    }

    fn default_rave_streaming() -> String {
        format!("ipc://{}/rave-stream.sock", Self::socket_dir())
    }

    fn default_orpheus() -> String {
        format!("ipc://{}/orpheus.sock", Self::socket_dir())
    }

    fn default_beatthis() -> String {
        format!("ipc://{}/beatthis.sock", Self::socket_dir())
    }

    fn default_musicgen() -> String {
        format!("ipc://{}/musicgen.sock", Self::socket_dir())
    }

    fn default_clap() -> String {
        format!("ipc://{}/clap.sock", Self::socket_dir())
    }

    fn default_audioldm2() -> String {
        format!("ipc://{}/audioldm2.sock", Self::socket_dir())
    }

    fn default_anticipatory() -> String {
        format!("ipc://{}/anticipatory.sock", Self::socket_dir())
    }

    fn default_demucs() -> String {
        format!("ipc://{}/demucs.sock", Self::socket_dir())
    }

    fn default_yue() -> String {
        format!("ipc://{}/yue.sock", Self::socket_dir())
    }

    fn default_midi_role() -> String {
        format!("ipc://{}/midi-role.sock", Self::socket_dir())
    }
}

impl Default for ConnectionsConfig {
    fn default() -> Self {
        Self {
            chaosgarden: Self::default_chaosgarden(),
            vibeweaver: Self::default_vibeweaver(),
            rave: Self::default_rave(),
            rave_streaming: Self::default_rave_streaming(),
            orpheus: Self::default_orpheus(),
            beatthis: Self::default_beatthis(),
            musicgen: Self::default_musicgen(),
            clap: Self::default_clap(),
            audioldm2: Self::default_audioldm2(),
            anticipatory: Self::default_anticipatory(),
            demucs: Self::default_demucs(),
            yue: Self::default_yue(),
            midi_role: Self::default_midi_role(),
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
        assert_eq!(models.get("gpu_observer"), Some(&"http://127.0.0.1:2099".to_string()));
        assert_eq!(models.len(), 1);
    }

    #[test]
    fn test_connections_musicgen_clap_yue() {
        let conn = ConnectionsConfig::default();
        assert!(conn.musicgen.contains("musicgen.sock"));
        assert!(conn.clap.contains("clap.sock"));
        assert!(conn.yue.contains("yue.sock"));
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
