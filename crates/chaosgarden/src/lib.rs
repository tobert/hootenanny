//! Chaosgarden: Realtime Audio Daemon
//!
//! The realtime audio component of the Hootenanny system. A standalone daemon
//! with RT priority that handles playback, graph state, and PipeWire I/O.
//!
//! Chaosgarden communicates with hootenanny (the control plane) via ZeroMQ using
//! a Jupyter-inspired 5-socket protocol:
//!
//! - **Control** (ROUTER/DEALER): Urgent commands - stop, pause, shutdown
//! - **Shell** (ROUTER/DEALER): Normal commands - create region, resolve latent
//! - **IOPub** (PUB/SUB): Events broadcast to all subscribers
//! - **Heartbeat** (REP/REQ): Liveness detection
//! - **Query** (REP/REQ): Trustfall queries about graph state

pub mod capabilities;
pub mod daemon;
pub mod external_io;
pub mod graph;
pub mod ipc;
pub mod latent;
pub mod nodes;
pub mod patterns;
#[cfg(feature = "pipewire")]
pub mod pipewire_output;
#[cfg(feature = "pipewire")]
pub mod pipewire_input;
pub mod playback;
pub mod primitives;
pub mod query;
pub mod stream_io;

pub use capabilities::{
    Capability, CapabilityRegistry, CapabilityRequirement, CapabilityUri, Constraint,
    ConstraintKind, ConstraintValue, IdentityCandidate, IdentityHints, IdentityMatch, Participant,
    ParticipantKind, SatisfactionResult,
};
pub use external_io::{
    ExternalIOError, ExternalIOManager, ExternalInputNode, ExternalOutputNode, MidiDevice,
    MidiDirection, MidiInputNode, MidiOutputNode, PipeWireInput, PipeWireOutput, RingBuffer,
};
pub use graph::{Edge, Graph, GraphError, GraphSnapshot};
pub use ipc::{client::GardenClient, server::GardenServer, GardenEndpoints};
pub use latent::{
    ApprovalDecision, Decision, IOPubPublisher, LatentConfig, LatentError, LatentEvent,
    LatentManager, MixInSchedule, MixInStrategy, PendingApproval,
};
pub use patterns::{
    Bus, BusOutput, Project, Section, SectionHints, Send, Timeline, Track, TrackOutput,
};
pub use nodes::{
    decode_audio, decode_wav, AudioFileNode, ContentResolver, DecodedAudio, FileCasClient,
    MemoryResolver,
};
pub use playback::{CompiledGraph, PlaybackEngine, PlaybackPosition};
pub use primitives::*;
pub use query::ChaosgardenAdapter;
pub use daemon::{DaemonConfig, GardenDaemon};
#[cfg(feature = "pipewire")]
pub use pipewire_output::{PipeWireOutputConfig, PipeWireOutputError, PipeWireOutputStream};
#[cfg(feature = "pipewire")]
pub use pipewire_input::{PipeWireInputConfig, PipeWireInputError, PipeWireInputStream};
