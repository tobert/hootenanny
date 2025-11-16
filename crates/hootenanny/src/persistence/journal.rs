//! Sled-based persistence for HalfRemembered.
//!
//! Stores:
//! - Musical events (sequential log)
//! - Conversation nodes (graph structure)
//! - Musical contexts (evolving state)
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Simple event structure for testing persistence.
/// Will be expanded with full Event Duality later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub timestamp: u64,
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionalVector {
    pub valence: f32,
    pub arousal: f32,
    pub agency: f32,
}

/// Sled-based journal for events.
pub struct Journal {
    db: sled::Db,
    events: sled::Tree,
}

impl Journal {
    /// Opens or creates a journal database at the given path.
    pub fn new(dir: &Path) -> Result<Self> {
        let db = sled::open(dir).context("Failed to open sled database")?;
        let events = db.open_tree("events").context("Failed to open events tree")?;

        Ok(Self { db, events })
    }

    /// Writes a session event to the journal.
    pub fn write_session_event(&mut self, event: &SessionEvent) -> Result<u64> {
        let event_id = self.db.generate_id().context("Failed to generate event ID")?;

        let bytes = bincode::serialize(&event).context("Failed to serialize event")?;

        // Use big-endian key for ordered iteration
        self.events
            .insert(event_id.to_be_bytes(), bytes)
            .context("Failed to insert event")?;

        Ok(event_id)
    }

    /// Reads all events from the journal.
    pub fn read_events(&self) -> Result<Vec<SessionEvent>> {
        let mut events = Vec::new();

        for result in self.events.iter() {
            let (_key, value) = result.context("Failed to read event")?;
            let event: SessionEvent =
                bincode::deserialize(&value).context("Failed to deserialize event")?;
            events.push(event);
        }

        Ok(events)
    }

    /// Flushes all pending writes to disk.
    pub fn flush(&self) -> Result<()> {
        self.db.flush().context("Failed to flush database")?;
        Ok(())
    }
}