//! ZMQ infrastructure for hootenanny
//!
//! Provides communication with chaosgarden (RT audio daemon) and luanette (Lua scripting).
//! Also provides bidirectional heartbeating with connected clients (holler).

mod client_tracker;
mod hooteproto_server;
mod luanette_client;
mod manager;
mod publisher;

pub use client_tracker::{ClientInfo, ClientTracker};
pub use hooteproto_server::HooteprotoServer;
pub use luanette_client::{LuanetteClient, spawn_heartbeat_task as spawn_luanette_heartbeat};
pub use manager::GardenManager;
pub use publisher::{BroadcastPublisher, PublisherServer};
