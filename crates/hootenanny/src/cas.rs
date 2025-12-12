//! Content Addressable Storage (CAS) - re-exports from shared crate.
//!
//! All CAS functionality is now in the `cas` crate, shared across
//! hootenanny, chaosgarden, and workers.

pub use cas::{ContentStore, FileStore};
