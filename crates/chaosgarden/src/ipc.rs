//! ZeroMQ IPC Layer
//!
//! Implements the Jupyter-inspired 5-socket protocol for communication between
//! hootenanny (control plane) and chaosgarden (RT audio daemon).
//!
//! GardenEndpoints is now defined in hooteproto and re-exported here for
//! backward compatibility.

pub mod capnp_server;
pub mod messages;

pub use messages::*;

// Re-export from hooteproto - the canonical location for garden protocol types
pub use hooteproto::GardenEndpoints;

