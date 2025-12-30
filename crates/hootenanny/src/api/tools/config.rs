//! Configuration inspection tool - request types for schema generation

use serde::{Deserialize, Serialize};

/// Request to get configuration values
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConfigGetRequest {
    /// Optional section to return (paths, bind, telemetry, models, connections, media, defaults)
    #[schemars(
        description = "Config section: 'paths', 'bind', 'telemetry', 'models', 'connections', 'media', 'defaults'. Omit for full config."
    )]
    pub section: Option<String>,

    /// Optional key within section
    #[schemars(description = "Specific key within section (e.g. 'cas_dir' in paths section)")]
    pub key: Option<String>,
}
