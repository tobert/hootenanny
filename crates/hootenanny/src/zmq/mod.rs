//! ZMQ infrastructure for hootenanny
//!
//! Provides communication with chaosgarden (RT audio daemon)
//! and vibeweaver (Python kernel for AI music agents).
//! Also provides bidirectional heartbeating with connected clients (holler).
//!
//! Note: GardenPeer is now in hooteproto. Use `hooteproto::GardenPeer` directly.

mod anticipatory_client;
mod audioldm2_client;
mod beatthis_client;
mod clap_client;
mod client_tracker;
mod demucs_client;
mod hooteproto_server;
mod manager;
mod midi_role_client;
mod musicgen_client;
mod orpheus_client;
mod publisher;
mod rave_client;
mod vibeweaver_client;
mod yue_client;

pub use anticipatory_client::{anticipatory_config, AnticipatoryClient, DEFAULT_ANTICIPATORY_TIMEOUT_MS};
pub use audioldm2_client::{audioldm2_config, Audioldm2Client, DEFAULT_AUDIOLDM2_TIMEOUT_MS};
pub use beatthis_client::{beatthis_config, BeatthisClient, DEFAULT_BEATTHIS_TIMEOUT_MS};
pub use clap_client::{clap_config, ClapClient, DEFAULT_CLAP_TIMEOUT_MS};
pub use demucs_client::{demucs_config, DemucsClient, DEFAULT_DEMUCS_TIMEOUT_MS};
pub use hooteproto::{GardenEndpoints, GardenPeer};
pub use hooteproto_server::HooteprotoServer;
pub use manager::GardenManager;
pub use midi_role_client::{midi_role_config, MidiRoleClient, DEFAULT_MIDI_ROLE_TIMEOUT_MS};
pub use musicgen_client::{musicgen_config, MusicgenClient, DEFAULT_MUSICGEN_TIMEOUT_MS};
pub use orpheus_client::{orpheus_config, OrpheusClient, DEFAULT_ORPHEUS_TIMEOUT_MS};
pub use publisher::{BroadcastPublisher, PublisherServer};
pub use rave_client::{rave_config, RaveClient, DEFAULT_RAVE_TIMEOUT_MS};
pub use vibeweaver_client::{vibeweaver_config, VibeweaverClient};
pub use yue_client::{yue_config, YueClient, DEFAULT_YUE_TIMEOUT_MS};
