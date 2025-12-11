//! ZMQ infrastructure for hootenanny
//!
//! Provides communication with chaosgarden (RT audio daemon) and workers.
//! Re-exports the GardenClient from chaosgarden crate.

mod hooteproto_server;
mod manager;

pub use hooteproto_server::HooteprotoServer;
pub use manager::GardenManager;

// Re-export types from chaosgarden for convenience
pub use chaosgarden::ipc::{
    Behavior, Beat, ContentType, ControlReply, ControlRequest, CurvePoint, ExecutionState,
    GardenEndpoints, IOPubEvent, Message, MessageHeader, NodeDescriptor, Participant,
    ParticipantUpdate, PendingApproval, PortRef, QueryReply, QueryRequest, RegionSummary,
    ShellReply, ShellRequest,
};

// Re-export client
pub use chaosgarden::ipc::client::GardenClient;
