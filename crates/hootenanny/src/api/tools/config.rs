//! Configuration inspection tool

use crate::api::service::EventDualityServer;
use hoot_config::{ConfigSources, HootConfig};
use hooteproto::{ToolError, ToolOutput, ToolResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request to get configuration values
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConfigGetRequest {
    /// Optional section to return (paths, bind, telemetry, models, connections, media, defaults)
    #[schemars(description = "Config section: 'paths', 'bind', 'telemetry', 'models', 'connections', 'media', 'defaults'. Omit for full config.")]
    pub section: Option<String>,

    /// Optional key within section
    #[schemars(description = "Specific key within section (e.g. 'cas_dir' in paths section)")]
    pub key: Option<String>,
}

/// Response for config_get
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigGetResponse {
    /// The config data (structure depends on section/key)
    #[serde(flatten)]
    pub data: Value,

    /// Source information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<ConfigSourcesInfo>,
}

/// Information about where config values came from
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSourcesInfo {
    /// Config files that were loaded
    pub files: Vec<String>,
    /// Environment variables that overrode values
    pub env_overrides: Vec<String>,
}

impl From<ConfigSources> for ConfigSourcesInfo {
    fn from(sources: ConfigSources) -> Self {
        Self {
            files: sources.files.iter().map(|p| p.display().to_string()).collect(),
            env_overrides: sources.env_overrides,
        }
    }
}

impl EventDualityServer {
    /// Get configuration values
    #[tracing::instrument(
        name = "tool.config_get",
        skip(self, request),
        fields(
            config.section = ?request.section,
            config.key = ?request.key,
        )
    )]
    pub async fn config_get(&self, request: ConfigGetRequest) -> ToolResult {
        // Load config with sources
        let (config, sources) = HootConfig::load_with_sources()
            .map_err(|e| ToolError::internal(format!("Failed to load config: {}", e)))?;

        let data = match (request.section.as_deref(), request.key.as_deref()) {
            // Full config
            (None, None) => {
                serde_json::json!({
                    "infra": {
                        "paths": {
                            "state_dir": config.infra.paths.state_dir,
                            "cas_dir": config.infra.paths.cas_dir,
                            "socket_dir": config.infra.paths.socket_dir,
                        },
                        "bind": {
                            "http_port": config.infra.bind.http_port,
                            "zmq_router": config.infra.bind.zmq_router,
                            "zmq_pub": config.infra.bind.zmq_pub,
                        },
                        "telemetry": {
                            "otlp_endpoint": config.infra.telemetry.otlp_endpoint,
                            "log_level": config.infra.telemetry.log_level,
                        }
                    },
                    "bootstrap": {
                        "models": config.bootstrap.models,
                        "connections": {
                            "chaosgarden": config.bootstrap.connections.chaosgarden,
                            "luanette": config.bootstrap.connections.luanette,
                        },
                        "media": {
                            "soundfont_dirs": config.bootstrap.media.soundfont_dirs,
                            "sample_dirs": config.bootstrap.media.sample_dirs,
                        },
                        "defaults": {
                            "lua_timeout": config.bootstrap.defaults.lua_timeout,
                            "session_expiration": config.bootstrap.defaults.session_expiration,
                            "max_concurrent_jobs": config.bootstrap.defaults.max_concurrent_jobs,
                        }
                    },
                    "sources": ConfigSourcesInfo::from(sources),
                })
            }

            // Section only
            (Some(section), None) => match section {
                "paths" => serde_json::json!({
                    "state_dir": config.infra.paths.state_dir,
                    "cas_dir": config.infra.paths.cas_dir,
                    "socket_dir": config.infra.paths.socket_dir,
                }),
                "bind" => serde_json::json!({
                    "http_port": config.infra.bind.http_port,
                    "zmq_router": config.infra.bind.zmq_router,
                    "zmq_pub": config.infra.bind.zmq_pub,
                }),
                "telemetry" => serde_json::json!({
                    "otlp_endpoint": config.infra.telemetry.otlp_endpoint,
                    "log_level": config.infra.telemetry.log_level,
                }),
                "models" => serde_json::to_value(&config.bootstrap.models)
                    .unwrap_or(Value::Null),
                "connections" => serde_json::json!({
                    "chaosgarden": config.bootstrap.connections.chaosgarden,
                    "luanette": config.bootstrap.connections.luanette,
                }),
                "media" => serde_json::json!({
                    "soundfont_dirs": config.bootstrap.media.soundfont_dirs,
                    "sample_dirs": config.bootstrap.media.sample_dirs,
                }),
                "defaults" => serde_json::json!({
                    "lua_timeout": config.bootstrap.defaults.lua_timeout,
                    "session_expiration": config.bootstrap.defaults.session_expiration,
                    "max_concurrent_jobs": config.bootstrap.defaults.max_concurrent_jobs,
                }),
                _ => return Err(ToolError::invalid_params(format!(
                    "Unknown section: {}. Valid: paths, bind, telemetry, models, connections, media, defaults",
                    section
                ))),
            },

            // Section + key
            (Some(section), Some(key)) => {
                let value = match (section, key) {
                    // paths
                    ("paths", "state_dir") => serde_json::json!(config.infra.paths.state_dir),
                    ("paths", "cas_dir") => serde_json::json!(config.infra.paths.cas_dir),
                    ("paths", "socket_dir") => serde_json::json!(config.infra.paths.socket_dir),
                    // bind
                    ("bind", "http_port") => serde_json::json!(config.infra.bind.http_port),
                    ("bind", "zmq_router") => serde_json::json!(config.infra.bind.zmq_router),
                    ("bind", "zmq_pub") => serde_json::json!(config.infra.bind.zmq_pub),
                    // telemetry
                    ("telemetry", "otlp_endpoint") => serde_json::json!(config.infra.telemetry.otlp_endpoint),
                    ("telemetry", "log_level") => serde_json::json!(config.infra.telemetry.log_level),
                    // connections
                    ("connections", "chaosgarden") => serde_json::json!(config.bootstrap.connections.chaosgarden),
                    ("connections", "luanette") => serde_json::json!(config.bootstrap.connections.luanette),
                    // defaults
                    ("defaults", "lua_timeout") => serde_json::json!(config.bootstrap.defaults.lua_timeout),
                    ("defaults", "session_expiration") => serde_json::json!(config.bootstrap.defaults.session_expiration),
                    ("defaults", "max_concurrent_jobs") => serde_json::json!(config.bootstrap.defaults.max_concurrent_jobs),
                    // media arrays
                    ("media", "soundfont_dirs") => serde_json::json!(config.bootstrap.media.soundfont_dirs),
                    ("media", "sample_dirs") => serde_json::json!(config.bootstrap.media.sample_dirs),
                    // models (key is model name)
                    ("models", model_name) => {
                        config.bootstrap.models.get(model_name)
                            .map(|url| serde_json::json!(url))
                            .unwrap_or(Value::Null)
                    }
                    _ => return Err(ToolError::invalid_params(format!(
                        "Unknown key '{}' in section '{}'",
                        key, section
                    ))),
                };

                serde_json::json!({
                    "value": value,
                    "section": section,
                    "key": key,
                })
            }

            // Key without section doesn't make sense
            (None, Some(_)) => {
                return Err(ToolError::invalid_params(
                    "Cannot specify key without section"
                ));
            }
        };

        let description = match (&request.section, &request.key) {
            (None, None) => "Full configuration".to_string(),
            (Some(s), None) => format!("Config section: {}", s),
            (Some(s), Some(k)) => format!("Config {}.{}", s, k),
            _ => "Config".to_string(),
        };

        Ok(ToolOutput::new(description, &data))
    }
}
