//! The persistence layer for the hootenanny.
//!
//! This module is responsible for saving and loading the state of the musical
//! ensemble, using an event sourcing strategy with periodic snapshots.

pub mod conversation_store;
pub mod journal;
pub mod snapshots;
