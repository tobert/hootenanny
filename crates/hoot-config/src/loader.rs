//! Config file discovery, loading, and environment variable overlay.

use crate::{BootstrapConfig, ConfigError, HootConfig, InfraConfig};
use std::env;
use std::path::{Path, PathBuf};

/// Information about where config values came from.
#[derive(Debug, Clone, Default)]
pub struct ConfigSources {
    /// Config files that were loaded (in order)
    pub files: Vec<PathBuf>,
    /// Environment variables that overrode config values
    pub env_overrides: Vec<String>,
}

/// Discover config files in standard locations.
///
/// Returns paths in load order (system, user, local).
/// Only returns files that exist.
pub fn discover_config_files() -> Vec<PathBuf> {
    let mut files = Vec::new();

    // System config
    let system = PathBuf::from("/etc/hootenanny/config.toml");
    if system.exists() {
        files.push(system);
    }

    // User config (XDG_CONFIG_HOME or ~/.config)
    if let Some(config_dir) = directories::BaseDirs::new().map(|d| d.config_dir().to_path_buf()) {
        let user = config_dir.join("hootenanny/config.toml");
        if user.exists() {
            files.push(user);
        }
    }

    // Local override (current directory)
    let local = PathBuf::from("hootenanny.toml");
    if local.exists() {
        files.push(local);
    }

    files
}

/// Load config from a TOML file.
pub fn load_from_file(path: &Path) -> Result<HootConfig, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|e| ConfigError::FileRead {
        path: path.to_path_buf(),
        source: e,
    })?;

    parse_toml(&contents, path)
}

/// Parse config from TOML string.
fn parse_toml(contents: &str, path: &Path) -> Result<HootConfig, ConfigError> {
    // Parse as raw TOML table first to handle nested structure
    let table: toml::Table = contents.parse().map_err(|e: toml::de::Error| ConfigError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    // Extract sections
    let infra: InfraConfig = if let Some(paths) = table.get("paths") {
        let mut infra = InfraConfig::default();
        if let Some(paths_table) = paths.as_table() {
            if let Some(v) = paths_table.get("state_dir").and_then(|v| v.as_str()) {
                infra.paths.state_dir = expand_path(v);
            }
            if let Some(v) = paths_table.get("cas_dir").and_then(|v| v.as_str()) {
                infra.paths.cas_dir = expand_path(v);
            }
            if let Some(v) = paths_table.get("socket_dir").and_then(|v| v.as_str()) {
                infra.paths.socket_dir = expand_path(v);
            }
        }

        if let Some(bind) = table.get("bind").and_then(|v| v.as_table()) {
            if let Some(v) = bind.get("http_port").and_then(|v| v.as_integer()) {
                infra.bind.http_port = v as u16;
            }
            if let Some(v) = bind.get("zmq_router").and_then(|v| v.as_str()) {
                infra.bind.zmq_router = v.to_string();
            }
            if let Some(v) = bind.get("zmq_pub").and_then(|v| v.as_str()) {
                infra.bind.zmq_pub = v.to_string();
            }
        }

        if let Some(telemetry) = table.get("telemetry").and_then(|v| v.as_table()) {
            if let Some(v) = telemetry.get("otlp_endpoint").and_then(|v| v.as_str()) {
                infra.telemetry.otlp_endpoint = v.to_string();
            }
            if let Some(v) = telemetry.get("log_level").and_then(|v| v.as_str()) {
                infra.telemetry.log_level = v.to_string();
            }
        }

        infra
    } else {
        InfraConfig::default()
    };

    let bootstrap: BootstrapConfig = if let Some(bootstrap_section) = table.get("bootstrap") {
        let mut bootstrap = BootstrapConfig::default();

        if let Some(models) = bootstrap_section.get("models").and_then(|v| v.as_table()) {
            for (name, url) in models {
                if let Some(url_str) = url.as_str() {
                    bootstrap.models.insert(name.clone(), url_str.to_string());
                }
            }
        }

        if let Some(conn) = bootstrap_section.get("connections").and_then(|v| v.as_table()) {
            if let Some(v) = conn.get("chaosgarden").and_then(|v| v.as_str()) {
                bootstrap.connections.chaosgarden = v.to_string();
            }
            if let Some(v) = conn.get("luanette").and_then(|v| v.as_str()) {
                bootstrap.connections.luanette = v.to_string();
            }
        }

        if let Some(media) = bootstrap_section.get("media").and_then(|v| v.as_table()) {
            if let Some(dirs) = media.get("soundfont_dirs").and_then(|v| v.as_array()) {
                bootstrap.media.soundfont_dirs = dirs
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(expand_path)
                    .collect();
            }
            if let Some(dirs) = media.get("sample_dirs").and_then(|v| v.as_array()) {
                bootstrap.media.sample_dirs = dirs
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(expand_path)
                    .collect();
            }
        }

        if let Some(defaults) = bootstrap_section.get("defaults").and_then(|v| v.as_table()) {
            if let Some(v) = defaults.get("lua_timeout").and_then(|v| v.as_str()) {
                bootstrap.defaults.lua_timeout = v.to_string();
            }
            if let Some(v) = defaults.get("session_expiration").and_then(|v| v.as_str()) {
                bootstrap.defaults.session_expiration = v.to_string();
            }
            if let Some(v) = defaults.get("max_concurrent_jobs").and_then(|v| v.as_integer()) {
                bootstrap.defaults.max_concurrent_jobs = v as u32;
            }
        }

        bootstrap
    } else {
        BootstrapConfig::default()
    };

    Ok(HootConfig { infra, bootstrap })
}

/// Merge two configs, with `overlay` taking precedence.
pub fn merge_configs(base: HootConfig, overlay: HootConfig) -> HootConfig {
    // For simplicity, overlay completely replaces base for now
    // A more sophisticated merge could be field-by-field
    HootConfig {
        infra: InfraConfig {
            paths: crate::infra::PathsConfig {
                state_dir: if overlay.infra.paths.state_dir != InfraConfig::default().paths.state_dir {
                    overlay.infra.paths.state_dir
                } else {
                    base.infra.paths.state_dir
                },
                cas_dir: if overlay.infra.paths.cas_dir != InfraConfig::default().paths.cas_dir {
                    overlay.infra.paths.cas_dir
                } else {
                    base.infra.paths.cas_dir
                },
                socket_dir: if overlay.infra.paths.socket_dir != InfraConfig::default().paths.socket_dir {
                    overlay.infra.paths.socket_dir
                } else {
                    base.infra.paths.socket_dir
                },
            },
            bind: crate::infra::BindConfig {
                http_port: if overlay.infra.bind.http_port != BindConfig::default().http_port {
                    overlay.infra.bind.http_port
                } else {
                    base.infra.bind.http_port
                },
                zmq_router: if overlay.infra.bind.zmq_router != BindConfig::default().zmq_router {
                    overlay.infra.bind.zmq_router
                } else {
                    base.infra.bind.zmq_router
                },
                zmq_pub: if overlay.infra.bind.zmq_pub != BindConfig::default().zmq_pub {
                    overlay.infra.bind.zmq_pub
                } else {
                    base.infra.bind.zmq_pub
                },
            },
            telemetry: crate::infra::TelemetryConfig {
                otlp_endpoint: if overlay.infra.telemetry.otlp_endpoint != TelemetryConfig::default().otlp_endpoint {
                    overlay.infra.telemetry.otlp_endpoint
                } else {
                    base.infra.telemetry.otlp_endpoint
                },
                log_level: if overlay.infra.telemetry.log_level != TelemetryConfig::default().log_level {
                    overlay.infra.telemetry.log_level
                } else {
                    base.infra.telemetry.log_level
                },
            },
        },
        bootstrap: overlay.bootstrap, // Bootstrap fully replaces for now
    }
}

use crate::infra::{BindConfig, TelemetryConfig};

/// Apply environment variable overrides to config.
pub fn apply_env_overrides(config: &mut HootConfig, sources: &mut ConfigSources) {
    // Infrastructure paths
    if let Ok(v) = env::var("HOOTENANNY_STATE_DIR") {
        config.infra.paths.state_dir = expand_path(&v);
        sources.env_overrides.push("HOOTENANNY_STATE_DIR".to_string());
    }
    if let Ok(v) = env::var("HOOTENANNY_CAS_DIR") {
        config.infra.paths.cas_dir = expand_path(&v);
        sources.env_overrides.push("HOOTENANNY_CAS_DIR".to_string());
    }
    // Legacy support
    if let Ok(v) = env::var("HOOTENANNY_CAS_PATH") {
        config.infra.paths.cas_dir = expand_path(&v);
        sources.env_overrides.push("HOOTENANNY_CAS_PATH".to_string());
    }
    if let Ok(v) = env::var("HOOTENANNY_SOCKET_DIR") {
        config.infra.paths.socket_dir = expand_path(&v);
        sources.env_overrides.push("HOOTENANNY_SOCKET_DIR".to_string());
    }

    // Bind addresses
    if let Ok(v) = env::var("HOOTENANNY_HTTP_PORT") {
        if let Ok(port) = v.parse() {
            config.infra.bind.http_port = port;
            sources.env_overrides.push("HOOTENANNY_HTTP_PORT".to_string());
        }
    }
    if let Ok(v) = env::var("HOOTENANNY_ZMQ_ROUTER") {
        config.infra.bind.zmq_router = v;
        sources.env_overrides.push("HOOTENANNY_ZMQ_ROUTER".to_string());
    }
    if let Ok(v) = env::var("HOOTENANNY_ZMQ_PUB") {
        config.infra.bind.zmq_pub = v;
        sources.env_overrides.push("HOOTENANNY_ZMQ_PUB".to_string());
    }

    // Telemetry
    if let Ok(v) = env::var("HOOTENANNY_OTLP_ENDPOINT") {
        config.infra.telemetry.otlp_endpoint = v;
        sources.env_overrides.push("HOOTENANNY_OTLP_ENDPOINT".to_string());
    }
    // Also support standard OTEL env var
    if let Ok(v) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        config.infra.telemetry.otlp_endpoint = v;
        sources.env_overrides.push("OTEL_EXPORTER_OTLP_ENDPOINT".to_string());
    }
    if let Ok(v) = env::var("HOOTENANNY_LOG_LEVEL") {
        config.infra.telemetry.log_level = v;
        sources.env_overrides.push("HOOTENANNY_LOG_LEVEL".to_string());
    }
    // Also support RUST_LOG
    if let Ok(v) = env::var("RUST_LOG") {
        config.infra.telemetry.log_level = v;
        sources.env_overrides.push("RUST_LOG".to_string());
    }

    // Model endpoints (HOOTENANNY_MODEL_<NAME>)
    for (key, value) in env::vars() {
        if let Some(model_name) = key.strip_prefix("HOOTENANNY_MODEL_") {
            let model_key = model_name.to_lowercase();
            config.bootstrap.models.insert(model_key, value);
            sources.env_overrides.push(key);
        }
    }
}

/// Expand ~ and environment variables in a path.
pub fn expand_path(path: &str) -> PathBuf {
    let expanded = if path.starts_with("~/") {
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            home.join(&path[2..])
        } else {
            PathBuf::from(path)
        }
    } else if path.starts_with('$') {
        // Handle $VAR/rest/of/path
        if let Some(slash_pos) = path.find('/') {
            let var_name = &path[1..slash_pos];
            if let Ok(var_value) = env::var(var_name) {
                PathBuf::from(var_value).join(&path[slash_pos + 1..])
            } else {
                PathBuf::from(path)
            }
        } else {
            let var_name = &path[1..];
            env::var(var_name).map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(path))
        }
    } else {
        PathBuf::from(path)
    };

    expanded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_path_tilde() {
        let expanded = expand_path("~/test/path");
        assert!(!expanded.to_string_lossy().starts_with('~'));
        assert!(expanded.to_string_lossy().contains("test/path"));
    }

    #[test]
    fn test_expand_path_absolute() {
        let expanded = expand_path("/absolute/path");
        assert_eq!(expanded, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_discover_config_files() {
        // Just verify it doesn't panic
        let _files = discover_config_files();
    }

    #[test]
    fn test_parse_minimal_toml() {
        let toml = r#"
[paths]
state_dir = "/custom/state"
"#;
        let config = parse_toml(toml, Path::new("test.toml")).unwrap();
        assert_eq!(config.infra.paths.state_dir, PathBuf::from("/custom/state"));
        // Other values should be defaults
        assert_eq!(config.infra.bind.http_port, 8082);
    }

    #[test]
    fn test_parse_full_toml() {
        let toml = r#"
[paths]
state_dir = "/data/hootenanny"
cas_dir = "/data/cas"

[bind]
http_port = 9000
zmq_router = "tcp://0.0.0.0:6000"

[telemetry]
log_level = "debug"

[bootstrap.models]
orpheus = "http://gpu:2000"
custom_model = "http://custom:3000"

[bootstrap.connections]
chaosgarden = "tcp://localhost:5555"

[bootstrap.media]
soundfont_dirs = ["/my/soundfonts", "/other/sf2"]

[bootstrap.defaults]
lua_timeout = "60s"
max_concurrent_jobs = 8
"#;
        let config = parse_toml(toml, Path::new("test.toml")).unwrap();

        assert_eq!(config.infra.paths.state_dir, PathBuf::from("/data/hootenanny"));
        assert_eq!(config.infra.paths.cas_dir, PathBuf::from("/data/cas"));
        assert_eq!(config.infra.bind.http_port, 9000);
        assert_eq!(config.infra.bind.zmq_router, "tcp://0.0.0.0:6000");
        assert_eq!(config.infra.telemetry.log_level, "debug");

        assert_eq!(config.bootstrap.models.get("orpheus"), Some(&"http://gpu:2000".to_string()));
        assert_eq!(config.bootstrap.models.get("custom_model"), Some(&"http://custom:3000".to_string()));
        assert_eq!(config.bootstrap.connections.chaosgarden, "tcp://localhost:5555");
        assert_eq!(config.bootstrap.media.soundfont_dirs.len(), 2);
        assert_eq!(config.bootstrap.defaults.lua_timeout, "60s");
        assert_eq!(config.bootstrap.defaults.max_concurrent_jobs, 8);
    }
}
