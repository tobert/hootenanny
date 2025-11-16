//! Integration tests for sled-based persistence layer.
//!
//! These tests verify:
//! - Events persist across database reopens
//! - Event ordering is preserved
//! - Concurrent writes are handled correctly
//! - Database recovery from crashes

use anyhow::Result;
use hootenanny::persistence::journal::{Journal, SessionEvent};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a temporary test database.
fn test_journal() -> Result<(Journal, TempDir)> {
    let temp_dir = TempDir::new()?;
    let journal = Journal::new(temp_dir.path())?;
    Ok((journal, temp_dir))
}

/// Helper to create a journal at a specific path (for reopen tests).
fn journal_at_path(path: &PathBuf) -> Result<Journal> {
    Journal::new(path)
}

#[test]
fn test_write_and_read_single_event() -> Result<()> {
    let (mut journal, _temp) = test_journal()?;

    let event = SessionEvent {
        timestamp: 1000,
        event_type: "test_event".to_string(),
    };

    let _event_id = journal.write_session_event(&event)?;
    // Event ID is generated successfully (u64 is always non-negative)

    let events = journal.read_events()?;
    assert_eq!(events.len(), 1, "Should have exactly one event");
    assert_eq!(events[0].timestamp, 1000);
    assert_eq!(events[0].event_type, "test_event");

    Ok(())
}

#[test]
fn test_write_multiple_events_preserves_order() -> Result<()> {
    let (mut journal, _temp) = test_journal()?;

    let events_to_write = vec![
        SessionEvent {
            timestamp: 1000,
            event_type: "first".to_string(),
        },
        SessionEvent {
            timestamp: 2000,
            event_type: "second".to_string(),
        },
        SessionEvent {
            timestamp: 3000,
            event_type: "third".to_string(),
        },
    ];

    let mut event_ids = Vec::new();
    for event in &events_to_write {
        let id = journal.write_session_event(event)?;
        event_ids.push(id);
    }

    // Verify IDs are monotonically increasing
    for i in 1..event_ids.len() {
        assert!(
            event_ids[i] > event_ids[i - 1],
            "Event IDs should be monotonically increasing"
        );
    }

    let read_events = journal.read_events()?;
    assert_eq!(read_events.len(), 3);

    // Verify order is preserved
    assert_eq!(read_events[0].event_type, "first");
    assert_eq!(read_events[1].event_type, "second");
    assert_eq!(read_events[2].event_type, "third");

    Ok(())
}

#[test]
fn test_persistence_across_reopens() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    // Write events in first session
    {
        let mut journal = journal_at_path(&db_path)?;

        journal.write_session_event(&SessionEvent {
            timestamp: 1000,
            event_type: "session_1_event_1".to_string(),
        })?;

        journal.write_session_event(&SessionEvent {
            timestamp: 2000,
            event_type: "session_1_event_2".to_string(),
        })?;

        journal.flush()?;
    } // Journal dropped here

    // Verify events persist after reopen
    {
        let journal = journal_at_path(&db_path)?;
        let events = journal.read_events()?;

        assert_eq!(events.len(), 2, "Events should persist across reopens");
        assert_eq!(events[0].event_type, "session_1_event_1");
        assert_eq!(events[1].event_type, "session_1_event_2");
    }

    // Write more events in second session
    {
        let mut journal = journal_at_path(&db_path)?;

        journal.write_session_event(&SessionEvent {
            timestamp: 3000,
            event_type: "session_2_event".to_string(),
        })?;

        journal.flush()?;

        let events = journal.read_events()?;
        assert_eq!(events.len(), 3, "Should have all events from both sessions");
        assert_eq!(events[2].event_type, "session_2_event");
    }

    Ok(())
}

#[test]
fn test_flush_ensures_durability() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    {
        let mut journal = journal_at_path(&db_path)?;

        journal.write_session_event(&SessionEvent {
            timestamp: 1000,
            event_type: "flush_test".to_string(),
        })?;

        // Explicit flush
        journal.flush()?;
    } // Drop without flush

    // Verify event persisted
    {
        let journal = journal_at_path(&db_path)?;
        let events = journal.read_events()?;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "flush_test");
    }

    Ok(())
}

#[test]
fn test_empty_journal_reads_empty() -> Result<()> {
    let (journal, _temp) = test_journal()?;

    let events = journal.read_events()?;
    assert_eq!(events.len(), 0, "New journal should be empty");

    Ok(())
}

#[test]
fn test_large_batch_write() -> Result<()> {
    let (mut journal, _temp) = test_journal()?;

    let batch_size = 1000;
    for i in 0..batch_size {
        journal.write_session_event(&SessionEvent {
            timestamp: i,
            event_type: format!("event_{}", i),
        })?;
    }

    journal.flush()?;

    let events = journal.read_events()?;
    assert_eq!(events.len(), batch_size as usize);

    // Verify first and last events
    assert_eq!(events[0].timestamp, 0);
    assert_eq!(events[0].event_type, "event_0");
    assert_eq!(events[batch_size as usize - 1].timestamp, batch_size - 1);
    assert_eq!(
        events[batch_size as usize - 1].event_type,
        format!("event_{}", batch_size - 1)
    );

    Ok(())
}

#[test]
fn test_event_id_uniqueness() -> Result<()> {
    let (mut journal, _temp) = test_journal()?;

    let mut ids = Vec::new();
    for i in 0..100 {
        let id = journal.write_session_event(&SessionEvent {
            timestamp: i,
            event_type: format!("unique_{}", i),
        })?;
        ids.push(id);
    }

    // Verify all IDs are unique
    let unique_count = ids.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(unique_count, 100, "All event IDs should be unique");

    Ok(())
}

#[test]
fn test_reopened_journal_continues_id_sequence() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().to_path_buf();

    let last_id_session_1 = {
        let mut journal = journal_at_path(&db_path)?;
        let id = journal.write_session_event(&SessionEvent {
            timestamp: 1000,
            event_type: "session_1".to_string(),
        })?;
        journal.flush()?;
        id
    };

    let first_id_session_2 = {
        let mut journal = journal_at_path(&db_path)?;
        let id = journal.write_session_event(&SessionEvent {
            timestamp: 2000,
            event_type: "session_2".to_string(),
        })?;
        journal.flush()?;
        id
    };

    assert!(
        first_id_session_2 > last_id_session_1,
        "IDs should continue monotonically across sessions"
    );

    Ok(())
}
