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
    /// HTTP bind address (IP only, without port).
    /// Default: "127.0.0.1" (localhost-only for security)
    /// Example: "0.0.0.0" (all interfaces), "100.64.x.y" (tailscale)
    #[serde(default = "BindConfig::default_http_address")]
    pub http_address: String,

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
    fn default_http_address() -> String {
        "127.0.0.1".to_string()
    }

    fn default_http_port() -> u16 {
        8082
    }

    fn default_zmq_router() -> String {
        "tcp://0.0.0.0:5580".to_string()
    }

    fn default_zmq_pub() -> String {
        "tcp://0.0.0.0:5581".to_string()
    }

    /// Get the full HTTP bind address as "ip:port".
    pub fn http_bind_addr(&self) -> String {
        format!("{}:{}", self.http_address, self.http_port)
    }
}

impl Default for BindConfig {
    fn default() -> Self {
        Self {
            http_address: Self::default_http_address(),
            http_port: Self::default_http_port(),
            zmq_router: Self::default_zmq_router(),
            zmq_pub: Self::default_zmq_pub(),
        }
    }
}

/// External HTTP access configuration for URL construction.
///
/// This is separate from bind addresses because the external URL
/// may differ (e.g., behind a proxy, or using a tailscale hostname).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// External hostname/IP for constructing URLs that agents can use.
    /// If unset, falls back to bind.http_address.
    /// Example: "beast.tail1234.ts.net" or "100.64.1.2"
    pub hostname: Option<String>,

    /// External port (if different from bind port, e.g., behind proxy).
    /// If unset, falls back to bind.http_port.
    pub port: Option<u16>,

    /// URL scheme. Default: "http".
    /// Future: "https" when TLS is added.
    #[serde(default = "HttpConfig::default_scheme")]
    pub scheme: String,
}

impl HttpConfig {
    fn default_scheme() -> String {
        "http".to_string()
    }

    /// Construct base URL for external access.
    pub fn base_url(&self, bind: &BindConfig) -> String {
        let host = self.hostname.as_deref().unwrap_or(&bind.http_address);
        let port = self.port.unwrap_or(bind.http_port);
        format!("{}://{}:{}", self.scheme, host, port)
    }

    /// Construct full artifact URL for agent-friendly responses.
    pub fn artifact_url(&self, bind: &BindConfig, artifact_id: &str) -> String {
        format!("{}/artifact/{}", self.base_url(bind), artifact_id)
    }

    /// Construct full stream URL.
    pub fn stream_url(&self, bind: &BindConfig) -> String {
        let host = self.hostname.as_deref().unwrap_or(&bind.http_address);
        let port = self.port.unwrap_or(bind.http_port);
        format!("ws://{}:{}/stream/live", host, port)
    }
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            hostname: None,
            port: None,
            scheme: Self::default_scheme(),
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

    /// External HTTP access configuration (for URL construction).
    #[serde(default)]
    pub http: HttpConfig,

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
        assert_eq!(bind.http_address, "127.0.0.1");
        assert_eq!(bind.http_port, 8082);
        assert_eq!(bind.zmq_router, "tcp://0.0.0.0:5580");
        assert_eq!(bind.zmq_pub, "tcp://0.0.0.0:5581");
        assert_eq!(bind.http_bind_addr(), "127.0.0.1:8082");
    }

    #[test]
    fn test_http_config_defaults() {
        let http = HttpConfig::default();
        assert!(http.hostname.is_none());
        assert!(http.port.is_none());
        assert_eq!(http.scheme, "http");
    }

    #[test]
    fn test_http_config_url_construction() {
        let bind = BindConfig::default();

        // With defaults, uses bind address
        let http = HttpConfig::default();
        assert_eq!(http.base_url(&bind), "http://127.0.0.1:8082");
        assert_eq!(
            http.artifact_url(&bind, "artifact_123"),
            "http://127.0.0.1:8082/artifact/artifact_123"
        );

        // With custom hostname
        let http = HttpConfig {
            hostname: Some("beast.ts.net".to_string()),
            port: None,
            scheme: "http".to_string(),
        };
        assert_eq!(http.base_url(&bind), "http://beast.ts.net:8082");

        // With custom hostname and port
        let http = HttpConfig {
            hostname: Some("beast.ts.net".to_string()),
            port: Some(443),
            scheme: "https".to_string(),
        };
        assert_eq!(http.base_url(&bind), "https://beast.ts.net:443");
    }

    #[test]
    fn test_telemetry_defaults() {
        let telemetry = TelemetryConfig::default();
        assert_eq!(telemetry.otlp_endpoint, "127.0.0.1:4317");
        assert_eq!(telemetry.log_level, "info");
    }
}
