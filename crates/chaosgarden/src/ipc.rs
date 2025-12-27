//! ZeroMQ IPC Layer
//!
//! Implements the Jupyter-inspired 5-socket protocol for communication between
//! hootenanny (control plane) and chaosgarden (RT audio daemon).

pub mod capnp_server;
pub mod messages;

pub use messages::*;

use std::time::Duration;

/// Configuration for connecting to a chaosgarden daemon
#[derive(Debug, Clone)]
pub struct GardenEndpoints {
    /// Control channel (DEALER/ROUTER) - urgent commands
    pub control: String,
    /// Shell channel (DEALER/ROUTER) - normal commands
    pub shell: String,
    /// IOPub channel (SUB/PUB) - event broadcasts
    pub iopub: String,
    /// Heartbeat channel (REQ/REP) - liveness detection
    pub heartbeat: String,
    /// Query channel (REQ/REP) - Trustfall queries
    pub query: String,
}

impl GardenEndpoints {
    /// Default IPC endpoints for local daemon (uses /tmp)
    pub fn local() -> Self {
        Self::from_socket_dir("/tmp")
    }

    /// IPC endpoints in a specific directory
    ///
    /// Use this with `paths.socket_dir` from HootConfig:
    /// ```ignore
    /// let endpoints = GardenEndpoints::from_socket_dir(
    ///     &config.infra.paths.socket_dir.to_string_lossy()
    /// );
    /// ```
    pub fn from_socket_dir(dir: &str) -> Self {
        Self {
            control: format!("ipc://{}/chaosgarden-control", dir),
            shell: format!("ipc://{}/chaosgarden-shell", dir),
            iopub: format!("ipc://{}/chaosgarden-iopub", dir),
            heartbeat: format!("ipc://{}/chaosgarden-hb", dir),
            query: format!("ipc://{}/chaosgarden-query", dir),
        }
    }

    /// TCP endpoints for remote daemon
    pub fn tcp(host: &str, base_port: u16) -> Self {
        Self {
            control: format!("tcp://{}:{}", host, base_port),
            shell: format!("tcp://{}:{}", host, base_port + 1),
            iopub: format!("tcp://{}:{}", host, base_port + 2),
            heartbeat: format!("tcp://{}:{}", host, base_port + 3),
            query: format!("tcp://{}:{}", host, base_port + 4),
        }
    }

    /// In-process endpoints for testing
    pub fn inproc(prefix: &str) -> Self {
        Self {
            control: format!("inproc://{}-control", prefix),
            shell: format!("inproc://{}-shell", prefix),
            iopub: format!("inproc://{}-iopub", prefix),
            heartbeat: format!("inproc://{}-hb", prefix),
            query: format!("inproc://{}-query", prefix),
        }
    }
}

impl Default for GardenEndpoints {
    fn default() -> Self {
        Self::local()
    }
}

/// Default heartbeat interval
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);

/// Default heartbeat timeout (miss 3 beats = dead)
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);

