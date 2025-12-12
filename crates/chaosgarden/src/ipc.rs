//! ZeroMQ IPC Layer
//!
//! Implements the Jupyter-inspired 5-socket protocol for communication between
//! hootenanny (control plane) and chaosgarden (RT audio daemon).

pub mod client;
pub mod messages;
pub mod server;
pub mod wire;

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
    /// Default IPC endpoints for local daemon
    pub fn local() -> Self {
        Self {
            control: "ipc:///tmp/chaosgarden-control".into(),
            shell: "ipc:///tmp/chaosgarden-shell".into(),
            iopub: "ipc:///tmp/chaosgarden-iopub".into(),
            heartbeat: "ipc:///tmp/chaosgarden-hb".into(),
            query: "ipc:///tmp/chaosgarden-query".into(),
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

