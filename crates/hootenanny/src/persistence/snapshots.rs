//! State Snapshotting.
//!
//! To ensure fast startups, the system periodically saves a complete snapshot
//! of the current state. On startup, the latest snapshot is loaded, and then
//! any subsequent events from the journal are replayed.
