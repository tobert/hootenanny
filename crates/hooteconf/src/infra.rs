//! Infrastructure configuration - things that cannot change at runtime.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Filesystem paths for Hootenanny state and data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    /// Base directory for runtime state (sled databases, artifact store).
    /// Default: ~/.local/share/hootenanny
    #[serde(default = "PathsConfig::default_state_dir")]
    pub state_dir: PathBuf,

    /// Content-addressable storage directory.
    /// Default: ~/.hootenanny/cas
    #[serde(default = "PathsConfig::default_cas_dir")]
    pub cas_dir: PathBuf,

    /// Directory for IPC sockets (chaosgarden).
    /// REQUIRED - must be set in config file. No default.
    /// Example: /tmp or /run/hootenanny
    pub socket_dir: Option<PathBuf>,
}

impl PathsConfig {
    fn default_state_dir() -> PathBuf {
        directories::BaseDirs::new()
            .map(|dirs| dirs.home_dir().join(".local/share/hootenanny"))
            .unwrap_or_else(|| PathBuf::from(".local/share/hootenanny"))
    }

    fn default_cas_dir() -> PathBuf {
        directories::BaseDirs::new()
            .map(|dirs| dirs.home_dir().join(".hootenanny/cas"))
            .unwrap_or_else(|| PathBuf::from(".hootenanny/cas"))
    }

    /// Get socket_dir or return error if not configured.
    ///
    /// Call this at startup to fail fast if socket_dir is missing.
    pub fn require_socket_dir(&self) -> anyhow::Result<&PathBuf> {
        self.socket_dir.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Missing required config: infra.paths.socket_dir\n\
                 Add to your hootenanny.toml:\n\
                 \n\
                 [infra.paths]\n\
                 socket_dir = \"/tmp\"  # or /run/hootenanny"
            )
        })
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            state_dir: Self::default_state_dir(),
            cas_dir: Self::default_cas_dir(),
            socket_dir: None, // Must be explicitly configured
        }
    }
}

/// Network bind addresses for this process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindConfig {
    /// HTTP port for artifacts and health endpoints.
    /// Default: 8082
    #[serde(default = "BindConfig::default_http_port")]
    pub http_port: u16,

    /// ZMQ ROUTER address for hooteproto gateway.
    /// Default: tcp://0.0.0.0:5580
    #[serde(default = "BindConfig::default_zmq_router")]
    pub zmq_router: String,

    /// ZMQ PUB address for event broadcasts.
    /// Default: tcp://0.0.0.0:5581
    #[serde(default = "BindConfig::default_zmq_pub")]
    pub zmq_pub: String,
}

impl BindConfig {
    fn default_http_port() -> u16 {
        8082
    }

    fn default_zmq_router() -> String {
        "tcp://0.0.0.0:5580".to_string()
    }

    fn default_zmq_pub() -> String {
        "tcp://0.0.0.0:5581".to_string()
    }
}

impl Default for BindConfig {
    fn default() -> Self {
        Self {
            http_port: Self::default_http_port(),
            zmq_router: Self::default_zmq_router(),
            zmq_pub: Self::default_zmq_pub(),
        }
    }
}

/// Telemetry and observability configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// OTLP gRPC endpoint for OpenTelemetry.
    /// Default: 127.0.0.1:4317
    #[serde(default = "TelemetryConfig::default_otlp_endpoint")]
    pub otlp_endpoint: String,

    /// Log level (trace, debug, info, warn, error).
    /// Default: info
    #[serde(default = "TelemetryConfig::default_log_level")]
    pub log_level: String,
}

impl TelemetryConfig {
    fn default_otlp_endpoint() -> String {
        "127.0.0.1:4317".to_string()
    }

    fn default_log_level() -> String {
        "info".to_string()
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: Self::default_otlp_endpoint(),
            log_level: Self::default_log_level(),
        }
    }
}

/// Gateway (holler) configuration.
///
/// Settings for the MCP gateway that connects to hootenanny.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// HTTP port for MCP and health endpoints.
    /// Default: 8080
    #[serde(default = "GatewayConfig::default_http_port")]
    pub http_port: u16,

    /// Hootenanny ZMQ ROUTER endpoint to connect to.
    /// Default: tcp://localhost:5580
    #[serde(default = "GatewayConfig::default_hootenanny")]
    pub hootenanny: String,

    /// Hootenanny ZMQ PUB endpoint for broadcasts.
    /// Default: tcp://localhost:5581
    #[serde(default = "GatewayConfig::default_hootenanny_pub")]
    pub hootenanny_pub: String,

    /// Request timeout in milliseconds.
    /// Should be slightly longer than inner service timeouts (30s) to allow for overhead.
    /// Default: 35000 (35s)
    #[serde(default = "GatewayConfig::default_timeout_ms")]
    pub timeout_ms: u64,
}

impl GatewayConfig {
    fn default_http_port() -> u16 {
        8080
    }

    fn default_hootenanny() -> String {
        "tcp://localhost:5580".to_string()
    }

    fn default_hootenanny_pub() -> String {
        "tcp://localhost:5581".to_string()
    }

    fn default_timeout_ms() -> u64 {
        35_000
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            http_port: Self::default_http_port(),
            hootenanny: Self::default_hootenanny(),
            hootenanny_pub: Self::default_hootenanny_pub(),
            timeout_ms: Self::default_timeout_ms(),
        }
    }
}

/// Vibeweaver service configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VibeweaverConfig {
    /// ZMQ ROUTER address to bind (for receiving requests).
    /// Default: tcp://0.0.0.0:5575
    #[serde(default = "VibeweaverConfig::default_zmq_router")]
    pub zmq_router: String,

    /// Hootenanny ZMQ ROUTER endpoint to connect to.
    /// Default: tcp://localhost:5580
    #[serde(default = "VibeweaverConfig::default_hootenanny")]
    pub hootenanny: String,

    /// Hootenanny ZMQ PUB endpoint for broadcasts.
    /// Default: tcp://localhost:5581
    #[serde(default = "VibeweaverConfig::default_hootenanny_pub")]
    pub hootenanny_pub: String,

    /// Request timeout in milliseconds.
    /// Default: 30000
    #[serde(default = "VibeweaverConfig::default_timeout_ms")]
    pub timeout_ms: u64,
}

impl VibeweaverConfig {
    fn default_zmq_router() -> String {
        "tcp://0.0.0.0:5575".to_string()
    }

    fn default_hootenanny() -> String {
        "tcp://localhost:5580".to_string()
    }

    fn default_hootenanny_pub() -> String {
        "tcp://localhost:5581".to_string()
    }

    fn default_timeout_ms() -> u64 {
        30000
    }
}

impl Default for VibeweaverConfig {
    fn default() -> Self {
        Self {
            zmq_router: Self::default_zmq_router(),
            hootenanny: Self::default_hootenanny(),
            hootenanny_pub: Self::default_hootenanny_pub(),
            timeout_ms: Self::default_timeout_ms(),
        }
    }
}

/// Chaosgarden service configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosgardenConfig {
    /// ZMQ ROUTER address to bind (for receiving requests).
    /// Default: tcp://0.0.0.0:5585
    #[serde(default = "ChaosgardenConfig::default_zmq_router")]
    pub zmq_router: String,

    /// IPC socket path for local communication.
    /// Default: /tmp/chaosgarden.sock
    #[serde(default = "ChaosgardenConfig::default_ipc_socket")]
    pub ipc_socket: String,
}

impl ChaosgardenConfig {
    fn default_zmq_router() -> String {
        "tcp://0.0.0.0:5585".to_string()
    }

    fn default_ipc_socket() -> String {
        "/tmp/chaosgarden.sock".to_string()
    }
}

impl Default for ChaosgardenConfig {
    fn default() -> Self {
        Self {
            zmq_router: Self::default_zmq_router(),
            ipc_socket: Self::default_ipc_socket(),
        }
    }
}

/// Per-service configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServicesConfig {
    /// Vibeweaver Python/AI agent service.
    #[serde(default)]
    pub vibeweaver: VibeweaverConfig,

    /// Chaosgarden audio output daemon.
    #[serde(default)]
    pub chaosgarden: ChaosgardenConfig,
}

/// Infrastructure configuration - cannot change at runtime.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InfraConfig {
    /// Filesystem paths.
    #[serde(default)]
    pub paths: PathsConfig,

    /// Network bind addresses (for hootenanny server).
    #[serde(default)]
    pub bind: BindConfig,

    /// Telemetry settings.
    #[serde(default)]
    pub telemetry: TelemetryConfig,

    /// Gateway (holler) settings.
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// Per-service settings.
    #[serde(default)]
    pub services: ServicesConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_defaults() {
        let paths = PathsConfig::default();
        assert!(paths.state_dir.to_string_lossy().contains("hootenanny"));
        assert!(paths.cas_dir.to_string_lossy().contains("hootenanny"));
        // socket_dir has no default - must be configured explicitly
        assert!(paths.socket_dir.is_none());
    }

    #[test]
    fn test_require_socket_dir_missing() {
        let paths = PathsConfig::default();
        let result = paths.require_socket_dir();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Missing required config"));
        assert!(err.contains("socket_dir"));
    }

    #[test]
    fn test_require_socket_dir_present() {
        let mut paths = PathsConfig::default();
        paths.socket_dir = Some(PathBuf::from("/run/hootenanny"));
        let result = paths.require_socket_dir();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), &PathBuf::from("/run/hootenanny"));
    }

    #[test]
    fn test_bind_defaults() {
        let bind = BindConfig::default();
        assert_eq!(bind.http_port, 8082);
        assert_eq!(bind.zmq_router, "tcp://0.0.0.0:5580");
        assert_eq!(bind.zmq_pub, "tcp://0.0.0.0:5581");
    }

    #[test]
    fn test_telemetry_defaults() {
        let telemetry = TelemetryConfig::default();
        assert_eq!(telemetry.otlp_endpoint, "127.0.0.1:4317");
        assert_eq!(telemetry.log_level, "info");
    }
}
