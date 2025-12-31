//! ZMQ infrastructure for hootenanny
#![allow(dead_code)]
//!
//! Provides communication with chaosgarden (RT audio daemon)
//! and vibeweaver (Python kernel for AI music agents).
//! Also provides bidirectional heartbeating with connected clients (holler).
//!
//! Note: GardenPeer is now in hooteproto. Use `hooteproto::GardenPeer` directly.

mod client_tracker;
mod garden_state_cache;
mod hooteproto_server;
mod manager;
mod publisher;
mod vibeweaver_client;

pub use garden_state_cache::{CacheStats, GardenStateCache};
pub use hooteproto::{GardenEndpoints, GardenPeer};
pub use hooteproto_server::HooteprotoServer;
pub use manager::GardenManager;
pub use publisher::{BroadcastPublisher, PublisherServer};
pub use vibeweaver_client::{vibeweaver_config, VibeweaverClient};
