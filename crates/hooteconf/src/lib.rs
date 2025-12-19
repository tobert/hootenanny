//! Minimal configuration loading for Hootenanny.
//!
//! This crate provides configuration loading with minimal dependencies,
//! designed to be imported by all Hootenanny crates without causing
//! circular dependency issues.
//!
//! # Configuration Philosophy
//!
//! Configuration is split into two categories:
//!
//! - **Infrastructure** (`InfraConfig`): Things that physically cannot change
//!   at runtime - paths, bind addresses, telemetry endpoints.
//!
//! - **Bootstrap** (`BootstrapConfig`): Initial values that seed runtime state.
//!   After startup, the runtime becomes the source of truth.
//!
//! # Usage
//!
//! ```rust,no_run
//! use hooteconf::HootConfig;
//!
//! let config = HootConfig::load().expect("Failed to load config");
//!
//! // Infrastructure (fixed)
//! println!("CAS dir: {}", config.infra.paths.cas_dir.display());
//! println!("HTTP port: {}", config.infra.bind.http_port);
//!
//! // Bootstrap (seeds runtime)
//! for (name, url) in &config.bootstrap.models {
//!     println!("Model {}: {}", name, url);
//! }
//! ```
//!
//! # Config File Locations
//!
//! Files are loaded in order (later wins):
//! 1. `/etc/hootenanny/config.toml` (system)
//! 2. `~/.config/hootenanny/config.toml` (user)
//! 3. `./hootenanny.toml` (local override)
//! 4. Environment variables (`HOOTENANNY_*`)
//!
//! # Example Config
//!
//! ```toml
//! [paths]
//! state_dir = "~/.local/share/hootenanny"
//! cas_dir = "~/.hootenanny/cas"
//!
//! [bind]
//! http_port = 8082
//! zmq_router = "tcp://0.0.0.0:5580"
//!
//! [telemetry]
//! otlp_endpoint = "127.0.0.1:4317"
//! log_level = "info"
//!
//! [bootstrap.models]
//! orpheus = "http://127.0.0.1:2000"
//!
//! [bootstrap.media]
//! soundfont_dirs = ["~/midi/SF2", "/usr/share/sounds/sf2"]
//! ```

pub mod bootstrap;
pub mod infra;
pub mod loader;

pub use bootstrap::{BootstrapConfig, ConnectionsConfig, DefaultsConfig, MediaConfig, ModelsConfig};
pub use infra::{BindConfig, GatewayConfig, InfraConfig, PathsConfig, TelemetryConfig};
pub use loader::{ConfigSources, discover_config_files_with_override};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Configuration loading errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file {path}: {source}")]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse config file {path}: {message}")]
    Parse { path: PathBuf, message: String },
}

/// Complete Hootenanny configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HootConfig {
    /// Infrastructure - cannot change at runtime.
    #[serde(flatten)]
    pub infra: InfraConfig,

    /// Bootstrap - seeds runtime state.
    #[serde(default)]
    pub bootstrap: BootstrapConfig,
}

impl HootConfig {
    /// Load configuration from all sources.
    ///
    /// Load order (later wins):
    /// 1. Compiled defaults
    /// 2. `/etc/hootenanny/config.toml`
    /// 3. `~/.config/hootenanny/config.toml`
    /// 4. `./hootenanny.toml`
    /// 5. Environment variables
    pub fn load() -> Result<Self, ConfigError> {
        let (config, _sources) = Self::load_with_sources_from(None)?;
        Ok(config)
    }

    /// Load configuration from a specific file path, then apply env overrides.
    ///
    /// If `config_path` is provided, it takes precedence over the local
    /// `./hootenanny.toml` override. System and user configs still load first.
    pub fn load_from(config_path: Option<&std::path::Path>) -> Result<Self, ConfigError> {
        let (config, _sources) = Self::load_with_sources_from(config_path)?;
        Ok(config)
    }

    /// Load configuration and return information about sources.
    pub fn load_with_sources() -> Result<(Self, ConfigSources), ConfigError> {
        Self::load_with_sources_from(None)
    }

    /// Load configuration from optional path and return information about sources.
    pub fn load_with_sources_from(
        config_path: Option<&std::path::Path>,
    ) -> Result<(Self, ConfigSources), ConfigError> {
        let mut sources = ConfigSources::default();
        let mut config = HootConfig::default();

        // Load config files in order
        for path in loader::discover_config_files_with_override(config_path) {
            let file_config = loader::load_from_file(&path)?;
            config = loader::merge_configs(config, file_config);
            sources.files.push(path);
        }

        // Apply environment variable overrides
        loader::apply_env_overrides(&mut config, &mut sources);

        Ok((config, sources))
    }

    /// Serialize config to TOML string.
    pub fn to_toml(&self) -> String {
        // Build TOML manually for nicer formatting
        let mut output = String::new();

        output.push_str("# Hootenanny Configuration\n\n");

        output.push_str("[paths]\n");
        output.push_str(&format!(
            "state_dir = \"{}\"\n",
            self.infra.paths.state_dir.display()
        ));
        output.push_str(&format!(
            "cas_dir = \"{}\"\n",
            self.infra.paths.cas_dir.display()
        ));
        output.push_str(&format!(
            "socket_dir = \"{}\"\n",
            self.infra.paths.socket_dir.display()
        ));

        output.push_str("\n[bind]\n");
        output.push_str(&format!("http_port = {}\n", self.infra.bind.http_port));
        output.push_str(&format!("zmq_router = \"{}\"\n", self.infra.bind.zmq_router));
        output.push_str(&format!("zmq_pub = \"{}\"\n", self.infra.bind.zmq_pub));

        output.push_str("\n[telemetry]\n");
        output.push_str(&format!(
            "otlp_endpoint = \"{}\"\n",
            self.infra.telemetry.otlp_endpoint
        ));
        output.push_str(&format!(
            "log_level = \"{}\"\n",
            self.infra.telemetry.log_level
        ));

        output.push_str("\n[gateway]\n");
        output.push_str(&format!("http_port = {}\n", self.infra.gateway.http_port));
        output.push_str(&format!(
            "hootenanny = \"{}\"\n",
            self.infra.gateway.hootenanny
        ));
        output.push_str(&format!(
            "hootenanny_pub = \"{}\"\n",
            self.infra.gateway.hootenanny_pub
        ));

        output.push_str("\n[bootstrap.models]\n");
        let mut models: Vec<_> = self.bootstrap.models.iter().collect();
        models.sort_by_key(|(k, _)| *k);
        for (name, url) in models {
            output.push_str(&format!("{} = \"{}\"\n", name, url));
        }

        output.push_str("\n[bootstrap.connections]\n");
        output.push_str(&format!(
            "chaosgarden = \"{}\"\n",
            self.bootstrap.connections.chaosgarden
        ));
        output.push_str(&format!(
            "luanette = \"{}\"\n",
            self.bootstrap.connections.luanette
        ));

        output.push_str("\n[bootstrap.media]\n");
        output.push_str("soundfont_dirs = [\n");
        for dir in &self.bootstrap.media.soundfont_dirs {
            output.push_str(&format!("    \"{}\",\n", dir.display()));
        }
        output.push_str("]\n");
        output.push_str("sample_dirs = [\n");
        for dir in &self.bootstrap.media.sample_dirs {
            output.push_str(&format!("    \"{}\",\n", dir.display()));
        }
        output.push_str("]\n");

        output.push_str("\n[bootstrap.defaults]\n");
        output.push_str(&format!(
            "lua_timeout = \"{}\"\n",
            self.bootstrap.defaults.lua_timeout
        ));
        output.push_str(&format!(
            "session_expiration = \"{}\"\n",
            self.bootstrap.defaults.session_expiration
        ));
        output.push_str(&format!(
            "max_concurrent_jobs = {}\n",
            self.bootstrap.defaults.max_concurrent_jobs
        ));

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HootConfig::default();
        assert_eq!(config.infra.bind.http_port, 8082);
        assert!(!config.bootstrap.models.is_empty());
    }

    #[test]
    fn test_to_toml() {
        let config = HootConfig::default();
        let toml = config.to_toml();
        assert!(toml.contains("[paths]"));
        assert!(toml.contains("[bind]"));
        assert!(toml.contains("[bootstrap.models]"));
        assert!(toml.contains("orpheus"));
    }

    #[test]
    fn test_load_defaults() {
        // Load should work even with no config files
        let config = HootConfig::load().unwrap();
        assert_eq!(config.infra.bind.http_port, 8082);
    }
}
