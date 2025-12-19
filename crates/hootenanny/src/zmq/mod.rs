//! ZMQ infrastructure for hootenanny
#![allow(dead_code)]
//!
//! Provides communication with chaosgarden (RT audio daemon) and luanette (Lua scripting).
//! Also provides bidirectional heartbeating with connected clients (holler).

mod client_tracker;
mod garden_client;
mod hooteproto_server;
mod luanette_client;
mod manager;
mod publisher;

pub use hooteproto_server::HooteprotoServer;
pub use luanette_client::LuanetteClient;
pub use manager::GardenManager;
pub use publisher::{BroadcastPublisher, PublisherServer};
