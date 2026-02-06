//! ZMQ infrastructure for hootenanny
//!
//! Provides communication with chaosgarden (RT audio daemon)
//! and vibeweaver (Python kernel for AI music agents).
//! Also provides bidirectional heartbeating with connected clients (holler).
//!
//! Note: GardenPeer is now in hooteproto. Use `hooteproto::GardenPeer` directly.

mod beatthis_client;
mod clap_client;
mod client_tracker;
mod hooteproto_server;
mod manager;
mod musicgen_client;
mod orpheus_client;
mod publisher;
mod rave_client;
mod vibeweaver_client;

pub use beatthis_client::{beatthis_config, BeatthisClient, DEFAULT_BEATTHIS_TIMEOUT_MS};
pub use clap_client::{clap_config, ClapClient, DEFAULT_CLAP_TIMEOUT_MS};
pub use hooteproto::{GardenEndpoints, GardenPeer};
pub use hooteproto_server::HooteprotoServer;
pub use manager::GardenManager;
pub use musicgen_client::{musicgen_config, MusicgenClient, DEFAULT_MUSICGEN_TIMEOUT_MS};
pub use orpheus_client::{orpheus_config, OrpheusClient, DEFAULT_ORPHEUS_TIMEOUT_MS};
pub use publisher::{BroadcastPublisher, PublisherServer};
pub use rave_client::{rave_config, RaveClient, DEFAULT_RAVE_TIMEOUT_MS};
pub use vibeweaver_client::{vibeweaver_config, VibeweaverClient};
