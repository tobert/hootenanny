//! ZMQ infrastructure for hootenanny
#![allow(dead_code)]
//!
//! Provides communication with chaosgarden (RT audio daemon), luanette (Lua scripting),
//! and vibeweaver (Python kernel for AI music agents).
//! Also provides bidirectional heartbeating with connected clients (holler).

mod client_tracker;
mod garden_client;
mod hooteproto_server;
mod luanette_client;
mod manager;
mod publisher;
mod vibeweaver_client;

pub use hooteproto_server::HooteprotoServer;
pub use luanette_client::{luanette_config, LuanetteClient};
pub use manager::GardenManager;
pub use publisher::{BroadcastPublisher, PublisherServer};
pub use vibeweaver_client::{vibeweaver_config, VibeweaverClient};
