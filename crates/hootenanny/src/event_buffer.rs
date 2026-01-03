//! Event buffer for cursor-based polling over MCP
//!
//! Stores broadcasts in a ring buffer for retrieval by sshwarma and other clients.
//! Beat ticks are stored separately (latest only) to avoid flooding the buffer.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use hooteproto::Broadcast;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::debug;

/// Default buffer capacity
pub const DEFAULT_CAPACITY: usize = 16_384;

/// Default poll limit
pub const DEFAULT_LIMIT: usize = 100;

/// Maximum poll limit
pub const MAX_LIMIT: usize = 1000;

/// Default timeout in milliseconds
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;

/// Maximum timeout in milliseconds
pub const MAX_TIMEOUT_MS: u64 = 30_000;

/// A buffered event with sequence number and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferedEvent {
    /// Monotonic sequence number
    pub seq: u64,
    /// Unix timestamp in milliseconds when event was buffered
    pub timestamp_ms: u64,
    /// Event type name (e.g., "job_state_changed", "artifact_created")
    pub event_type: String,
    /// Full event data as JSON
    pub data: serde_json::Value,
}

/// Latest beat tick info (stored separately from ring buffer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatTickInfo {
    pub beat: u64,
    pub position_beats: f64,
    pub tempo_bpm: f64,
    pub timestamp_ms: u64,
}

/// Transport state info (stored separately from ring buffer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportInfo {
    /// Current state: "playing", "paused", "stopped"
    pub state: String,
    /// Current position in beats
    pub position_beats: f64,
    /// Current tempo in BPM
    pub tempo_bpm: f64,
    /// Timestamp when this state was captured (ms since epoch)
    pub timestamp_ms: u64,
}

/// Buffer statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferStats {
    pub oldest_cursor: u64,
    pub newest_cursor: u64,
    pub total_events: u64,
    pub capacity: u64,
}

/// Poll result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollResult {
    pub events: Vec<BufferedEvent>,
    pub cursor: u64,
    pub has_more: bool,
    pub latest_beat: Option<BeatTickInfo>,
    pub buffer: BufferStats,
    /// Server timestamp at response time (millis since epoch)
    pub server_time_ms: u64,
}

/// Poll error types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum PollError {
    /// Cursor fell off the buffer (too old)
    CursorExpired {
        message: String,
        oldest_cursor: u64,
    },
    /// Cursor is in the future
    InvalidCursor {
        message: String,
        newest_cursor: u64,
    },
    /// Timeout value out of range
    InvalidTimeout {
        message: String,
    },
    /// Limit value out of range
    InvalidLimit {
        message: String,
    },
    /// Unknown event type in filter
    InvalidTypes {
        message: String,
        unknown_types: Vec<String>,
    },
}

impl std::fmt::Display for PollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PollError::CursorExpired { message, .. } => write!(f, "{}", message),
            PollError::InvalidCursor { message, .. } => write!(f, "{}", message),
            PollError::InvalidTimeout { message } => write!(f, "{}", message),
            PollError::InvalidLimit { message } => write!(f, "{}", message),
            PollError::InvalidTypes { message, .. } => write!(f, "{}", message),
        }
    }
}

impl std::error::Error for PollError {}

/// Known event types for validation
const KNOWN_EVENT_TYPES: &[&str] = &[
    "config_update",
    "shutdown",
    "script_invalidate",
    "job_state_changed",
    "progress",
    "artifact_created",
    "transport_state_changed",
    "marker_reached",
    "log",
    "device_connected",
    "device_disconnected",
];

/// Event buffer with ring buffer storage
pub struct EventBuffer {
    events: VecDeque<BufferedEvent>,
    next_seq: u64,
    capacity: usize,
    latest_beat: Option<BeatTickInfo>,
    /// Latest transport state (from TransportStateChanged)
    latest_transport: Option<TransportInfo>,
    /// Number of connected devices
    device_count: u32,
    /// Total events ever pushed (for stats)
    total_pushed: u64,
}

impl EventBuffer {
    /// Create a new event buffer with the given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            next_seq: 1, // Start at 1 so 0 can mean "no cursor"
            capacity,
            latest_beat: None,
            latest_transport: None,
            device_count: 0,
            total_pushed: 0,
        }
    }

    /// Push a broadcast into the buffer
    ///
    /// Beat ticks are stored separately (latest only).
    /// Transport state and device count are tracked separately.
    /// All other events go into the ring buffer.
    pub fn push(&mut self, broadcast: &Broadcast) {
        let timestamp_ms = current_time_ms();

        // Handle beat ticks separately (don't buffer, just track latest)
        if let Broadcast::BeatTick {
            beat,
            position_beats,
            tempo_bpm,
        } = broadcast
        {
            self.latest_beat = Some(BeatTickInfo {
                beat: *beat,
                position_beats: *position_beats,
                tempo_bpm: *tempo_bpm,
                timestamp_ms,
            });
            return;
        }

        // Track transport state changes
        if let Broadcast::TransportStateChanged {
            state,
            position_beats,
            tempo_bpm,
        } = broadcast
        {
            self.latest_transport = Some(TransportInfo {
                state: state.clone(),
                position_beats: *position_beats,
                tempo_bpm: *tempo_bpm,
                timestamp_ms,
            });
        }

        // Track device connections
        if matches!(broadcast, Broadcast::DeviceConnected { .. }) {
            self.device_count = self.device_count.saturating_add(1);
        }
        if matches!(broadcast, Broadcast::DeviceDisconnected { .. }) {
            self.device_count = self.device_count.saturating_sub(1);
        }

        // Convert broadcast to buffered event
        let event_type = broadcast_type_name(broadcast);
        let data = broadcast_to_json(broadcast);

        let event = BufferedEvent {
            seq: self.next_seq,
            timestamp_ms,
            event_type: event_type.to_string(),
            data,
        };

        // Evict oldest if at capacity
        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }

        self.events.push_back(event);
        self.next_seq += 1;
        self.total_pushed += 1;

        debug!(
            "Buffered event seq={} type={} (buffer size: {})",
            self.next_seq - 1,
            event_type,
            self.events.len()
        );
    }

    /// Get buffer statistics
    pub fn stats(&self) -> BufferStats {
        let oldest_cursor = self.events.front().map(|e| e.seq).unwrap_or(self.next_seq);
        let newest_cursor = self.next_seq.saturating_sub(1);

        BufferStats {
            oldest_cursor,
            newest_cursor,
            total_events: self.total_pushed,
            capacity: self.capacity as u64,
        }
    }

    /// Poll for events after the given cursor or within a time window
    ///
    /// If since_ms is Some, returns events from the last N milliseconds.
    /// If cursor is Some, returns events after that cursor.
    /// If both are None, returns the newest `limit` events.
    pub fn poll(
        &self,
        cursor: Option<u64>,
        since_ms: Option<u64>,
        types: Option<&[String]>,
        limit: usize,
    ) -> Result<PollResult, PollError> {
        let stats = self.stats();
        let server_time_ms = current_time_ms();

        // Validate types if provided
        if let Some(type_filter) = types {
            let unknown: Vec<String> = type_filter
                .iter()
                .filter(|t| !KNOWN_EVENT_TYPES.contains(&t.as_str()))
                .cloned()
                .collect();

            if !unknown.is_empty() {
                return Err(PollError::InvalidTypes {
                    message: format!("Unknown event types: {:?}", unknown),
                    unknown_types: unknown,
                });
            }
        }

        // Determine which query mode to use
        let events = if let Some(since) = since_ms {
            // Time-window query: get events from the last N milliseconds
            let cutoff = server_time_ms.saturating_sub(since);
            self.get_events_since(cutoff, types, limit)
        } else if let Some(cursor_seq) = cursor {
            // Cursor-based query: validate and get events after cursor
            if cursor_seq > 0 && cursor_seq < stats.oldest_cursor {
                return Err(PollError::CursorExpired {
                    message: format!(
                        "Cursor {} is no longer available. Oldest: {}",
                        cursor_seq, stats.oldest_cursor
                    ),
                    oldest_cursor: stats.oldest_cursor,
                });
            }

            if cursor_seq > stats.newest_cursor && stats.newest_cursor > 0 {
                return Err(PollError::InvalidCursor {
                    message: format!(
                        "Cursor {} is in the future. Newest: {}",
                        cursor_seq, stats.newest_cursor
                    ),
                    newest_cursor: stats.newest_cursor,
                });
            }

            self.get_events_after(cursor_seq, types, limit)
        } else {
            // No cursor or since_ms: return newest `limit` events from tail
            self.get_tail_events(types, limit)
        };

        // Calculate new cursor and has_more
        let new_cursor = events.last().map(|e| e.seq).unwrap_or_else(|| {
            cursor.unwrap_or(stats.newest_cursor)
        });

        // Check if there are more events beyond what we returned
        let has_more = if since_ms.is_some() {
            // For time-window query: has_more if we hit the limit
            events.len() >= limit
        } else if cursor.is_some() {
            // Cursor poll: has_more if there are newer events after what we returned
            events.last().map(|e| e.seq < stats.newest_cursor).unwrap_or(false)
        } else {
            // Initial poll: has_more if there are older events we didn't return
            events.first().map(|e| e.seq > stats.oldest_cursor).unwrap_or(false)
        };

        Ok(PollResult {
            events,
            cursor: new_cursor,
            has_more,
            latest_beat: self.latest_beat.clone(),
            buffer: stats,
            server_time_ms,
        })
    }

    /// Get events since the given timestamp (cutoff in ms since epoch)
    fn get_events_since(
        &self,
        cutoff_ms: u64,
        types: Option<&[String]>,
        limit: usize,
    ) -> Vec<BufferedEvent> {
        self.events
            .iter()
            .filter(|e| e.timestamp_ms >= cutoff_ms)
            .filter(|e| match types {
                Some(filter) => filter.iter().any(|t| t == &e.event_type),
                None => true,
            })
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get newest `limit` events from the tail of the buffer
    fn get_tail_events(&self, types: Option<&[String]>, limit: usize) -> Vec<BufferedEvent> {
        self.events
            .iter()
            .rev()
            .filter(|e| match types {
                Some(filter) => filter.iter().any(|t| t == &e.event_type),
                None => true,
            })
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Get events after the given cursor (exclusive)
    fn get_events_after(
        &self,
        cursor: u64,
        types: Option<&[String]>,
        limit: usize,
    ) -> Vec<BufferedEvent> {
        self.events
            .iter()
            .filter(|e| e.seq > cursor)
            .filter(|e| match types {
                Some(filter) => filter.iter().any(|t| t == &e.event_type),
                None => true,
            })
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get the latest beat tick info
    pub fn latest_beat(&self) -> Option<&BeatTickInfo> {
        self.latest_beat.as_ref()
    }

    /// Get the latest transport state info
    pub fn latest_transport(&self) -> Option<&TransportInfo> {
        self.latest_transport.as_ref()
    }

    /// Get the current device count
    pub fn device_count(&self) -> u32 {
        self.device_count
    }

    /// Check if there are events after the given cursor
    pub fn has_events_after(&self, cursor: u64) -> bool {
        self.events.iter().any(|e| e.seq > cursor)
    }
}

/// Thread-safe event buffer handle
pub type EventBufferHandle = Arc<RwLock<EventBuffer>>;

/// Create a new thread-safe event buffer
pub fn create_event_buffer(capacity: usize) -> EventBufferHandle {
    Arc::new(RwLock::new(EventBuffer::new(capacity)))
}

/// Get current time in milliseconds since epoch
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Get the type name for a broadcast
fn broadcast_type_name(broadcast: &Broadcast) -> &'static str {
    match broadcast {
        Broadcast::ConfigUpdate { .. } => "config_update",
        Broadcast::Shutdown { .. } => "shutdown",
        Broadcast::ScriptInvalidate { .. } => "script_invalidate",
        Broadcast::JobStateChanged { .. } => "job_state_changed",
        Broadcast::Progress { .. } => "progress",
        Broadcast::ArtifactCreated { .. } => "artifact_created",
        Broadcast::TransportStateChanged { .. } => "transport_state_changed",
        Broadcast::MarkerReached { .. } => "marker_reached",
        Broadcast::BeatTick { .. } => "beat_tick",
        Broadcast::Log { .. } => "log",
        Broadcast::DeviceConnected { .. } => "device_connected",
        Broadcast::DeviceDisconnected { .. } => "device_disconnected",
    }
}

/// Convert broadcast to JSON value
fn broadcast_to_json(broadcast: &Broadcast) -> serde_json::Value {
    serde_json::to_value(broadcast).unwrap_or(serde_json::Value::Null)
}

/// Validate poll parameters
pub fn validate_poll_params(
    timeout_ms: Option<u64>,
    limit: Option<usize>,
) -> Result<(u64, usize), PollError> {
    let timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    if timeout > MAX_TIMEOUT_MS {
        return Err(PollError::InvalidTimeout {
            message: format!(
                "timeout_ms {} exceeds maximum {}",
                timeout, MAX_TIMEOUT_MS
            ),
        });
    }

    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(PollError::InvalidLimit {
            message: format!("limit must be between 1 and {}, got {}", MAX_LIMIT, limit),
        });
    }

    Ok((timeout, limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_poll() {
        let mut buffer = EventBuffer::new(100);

        // Push some events
        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job1".to_string(),
            state: "complete".to_string(),
            result: None,
        });
        buffer.push(&Broadcast::ArtifactCreated {
            artifact_id: "art1".to_string(),
            content_hash: "hash1".to_string(),
            tags: vec!["test".to_string()],
            creator: Some("claude".to_string()),
        });

        // Poll without cursor - get all
        let result = buffer.poll(None, None, None, 100).unwrap();
        assert_eq!(result.events.len(), 2);
        assert_eq!(result.cursor, 2);
        assert!(!result.has_more);
    }

    #[test]
    fn test_cursor_polling() {
        let mut buffer = EventBuffer::new(100);

        // Push events
        for i in 0..5 {
            buffer.push(&Broadcast::JobStateChanged {
                job_id: format!("job{}", i),
                state: "complete".to_string(),
                result: None,
            });
        }

        // Poll from cursor 2
        let result = buffer.poll(Some(2), None, None, 100).unwrap();
        assert_eq!(result.events.len(), 3); // Events 3, 4, 5
        assert_eq!(result.events[0].seq, 3);
        assert_eq!(result.cursor, 5);
    }

    #[test]
    fn test_type_filtering() {
        let mut buffer = EventBuffer::new(100);

        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job1".to_string(),
            state: "complete".to_string(),
            result: None,
        });
        buffer.push(&Broadcast::ArtifactCreated {
            artifact_id: "art1".to_string(),
            content_hash: "hash1".to_string(),
            tags: vec![],
            creator: None,
        });
        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job2".to_string(),
            state: "failed".to_string(),
            result: None,
        });

        // Filter for job events only
        let types = vec!["job_state_changed".to_string()];
        let result = buffer.poll(None, None, Some(&types), 100).unwrap();
        assert_eq!(result.events.len(), 2);
    }

    #[test]
    fn test_beat_tick_separate() {
        let mut buffer = EventBuffer::new(100);

        // Push regular event
        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job1".to_string(),
            state: "complete".to_string(),
            result: None,
        });

        // Push beat tick (should NOT go in buffer)
        buffer.push(&Broadcast::BeatTick {
            beat: 100,
            position_beats: 100.5,
            tempo_bpm: 120.0,
        });

        // Only one event in buffer
        let result = buffer.poll(None, None, None, 100).unwrap();
        assert_eq!(result.events.len(), 1);

        // But latest_beat is present
        assert!(result.latest_beat.is_some());
        assert_eq!(result.latest_beat.unwrap().beat, 100);
    }

    #[test]
    fn test_ring_buffer_eviction() {
        let mut buffer = EventBuffer::new(5);

        // Push 10 events into buffer of size 5
        for i in 0..10 {
            buffer.push(&Broadcast::JobStateChanged {
                job_id: format!("job{}", i),
                state: "complete".to_string(),
                result: None,
            });
        }

        // Should only have last 5 events
        let result = buffer.poll(None, None, None, 100).unwrap();
        assert_eq!(result.events.len(), 5);
        assert_eq!(result.events[0].seq, 6); // Oldest is seq 6
        assert_eq!(result.buffer.oldest_cursor, 6);
    }

    #[test]
    fn test_cursor_expired() {
        let mut buffer = EventBuffer::new(5);

        // Push 10 events
        for i in 0..10 {
            buffer.push(&Broadcast::JobStateChanged {
                job_id: format!("job{}", i),
                state: "complete".to_string(),
                result: None,
            });
        }

        // Try to poll with old cursor
        let result = buffer.poll(Some(2), None, None, 100);
        assert!(matches!(result, Err(PollError::CursorExpired { .. })));
    }

    #[test]
    fn test_invalid_cursor() {
        let mut buffer = EventBuffer::new(100);

        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job1".to_string(),
            state: "complete".to_string(),
            result: None,
        });

        // Try cursor in the future
        let result = buffer.poll(Some(999), None, None, 100);
        assert!(matches!(result, Err(PollError::InvalidCursor { .. })));
    }

    #[test]
    fn test_limit() {
        let mut buffer = EventBuffer::new(100);

        for i in 0..10 {
            buffer.push(&Broadcast::JobStateChanged {
                job_id: format!("job{}", i),
                state: "complete".to_string(),
                result: None,
            });
        }

        // Poll with limit 3
        let result = buffer.poll(None, None, None, 3).unwrap();
        assert_eq!(result.events.len(), 3);
        assert!(result.has_more);
        // Should get newest 3 (seq 8, 9, 10)
        assert_eq!(result.events[0].seq, 8);
    }

    #[test]
    fn test_transport_tracking() {
        let mut buffer = EventBuffer::new(100);

        // Push transport state change
        buffer.push(&Broadcast::TransportStateChanged {
            state: "playing".to_string(),
            position_beats: 100.5,
            tempo_bpm: 120.0,
        });

        // Latest transport should be tracked
        let transport = buffer.latest_transport().unwrap();
        assert_eq!(transport.state, "playing");
        assert_eq!(transport.position_beats, 100.5);
        assert_eq!(transport.tempo_bpm, 120.0);

        // Event should also be in buffer
        let result = buffer.poll(None, None, None, 100).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0].event_type, "transport_state_changed");
    }

    #[test]
    fn test_device_count_tracking() {
        let mut buffer = EventBuffer::new(100);

        assert_eq!(buffer.device_count(), 0);

        // Connect some devices
        buffer.push(&Broadcast::DeviceConnected {
            pipewire_id: 42,
            name: "USB MIDI".to_string(),
            media_class: Some("Midi/Bridge".to_string()),
            identity_id: None,
            identity_name: None,
        });
        assert_eq!(buffer.device_count(), 1);

        buffer.push(&Broadcast::DeviceConnected {
            pipewire_id: 43,
            name: "MIDI Keyboard".to_string(),
            media_class: Some("Midi/Bridge".to_string()),
            identity_id: None,
            identity_name: None,
        });
        assert_eq!(buffer.device_count(), 2);

        // Disconnect one
        buffer.push(&Broadcast::DeviceDisconnected {
            pipewire_id: 42,
            name: Some("USB MIDI".to_string()),
        });
        assert_eq!(buffer.device_count(), 1);
    }

    #[test]
    fn test_since_ms_returns_recent_events() {
        let mut buffer = EventBuffer::new(100);

        // Push events
        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job1".to_string(),
            state: "complete".to_string(),
            result: None,
        });

        // Large window should return events
        let result = buffer.poll(None, Some(10000), None, 100).unwrap();
        assert_eq!(result.events.len(), 1);
    }

    #[test]
    fn test_since_ms_with_type_filter() {
        let mut buffer = EventBuffer::new(100);

        // Push mixed events
        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job1".to_string(),
            state: "complete".to_string(),
            result: None,
        });
        buffer.push(&Broadcast::ArtifactCreated {
            artifact_id: "art1".to_string(),
            content_hash: "hash1".to_string(),
            tags: vec![],
            creator: None,
        });
        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job2".to_string(),
            state: "running".to_string(),
            result: None,
        });

        // Filter by type within time window
        let types = vec!["job_state_changed".to_string()];
        let result = buffer.poll(None, Some(10000), Some(&types), 100).unwrap();
        assert_eq!(result.events.len(), 2);
        assert!(result.events.iter().all(|e| e.event_type == "job_state_changed"));
    }

    #[test]
    fn test_since_ms_respects_limit() {
        let mut buffer = EventBuffer::new(100);

        // Push several events
        for i in 0..10 {
            buffer.push(&Broadcast::JobStateChanged {
                job_id: format!("job{}", i),
                state: "complete".to_string(),
                result: None,
            });
        }

        // Should respect limit
        let result = buffer.poll(None, Some(10000), None, 3).unwrap();
        assert_eq!(result.events.len(), 3);
        assert!(result.has_more); // More events available
    }

    #[test]
    fn test_since_ms_takes_precedence_over_cursor() {
        let mut buffer = EventBuffer::new(100);

        // Push events
        for i in 0..5 {
            buffer.push(&Broadcast::JobStateChanged {
                job_id: format!("job{}", i),
                state: "complete".to_string(),
                result: None,
            });
        }

        // When both cursor and since_ms provided, since_ms wins
        // Cursor would return events after seq 2, but since_ms returns from time window
        let result = buffer.poll(Some(2), Some(10000), None, 100).unwrap();
        // Should get all 5 events (from time window), not just 3 (from cursor)
        assert_eq!(result.events.len(), 5);
    }

    #[test]
    fn test_since_ms_empty_for_narrow_window() {
        use std::thread;
        use std::time::Duration;

        let mut buffer = EventBuffer::new(100);

        // Push an event
        buffer.push(&Broadcast::JobStateChanged {
            job_id: "job1".to_string(),
            state: "complete".to_string(),
            result: None,
        });

        // Wait a bit so event is "old"
        thread::sleep(Duration::from_millis(50));

        // Very narrow window (1ms) shouldn't include events from 50ms ago
        let result = buffer.poll(None, Some(1), None, 100).unwrap();
        assert_eq!(result.events.len(), 0);
    }

    #[test]
    fn test_validate_poll_params() {
        // Valid params
        assert!(validate_poll_params(Some(5000), Some(100)).is_ok());

        // Invalid timeout
        assert!(matches!(
            validate_poll_params(Some(999999), None),
            Err(PollError::InvalidTimeout { .. })
        ));

        // Invalid limit
        assert!(matches!(
            validate_poll_params(None, Some(0)),
            Err(PollError::InvalidLimit { .. })
        ));
        assert!(matches!(
            validate_poll_params(None, Some(9999)),
            Err(PollError::InvalidLimit { .. })
        ));
    }

    #[test]
    fn test_snapshot_data_available() {
        // Test that all data needed for Snapshot is available from EventBuffer
        let mut buffer = EventBuffer::new(100);

        // Initially everything is None/zero
        assert!(buffer.latest_beat().is_none());
        assert!(buffer.latest_transport().is_none());
        assert_eq!(buffer.device_count(), 0);

        // Add a beat tick
        buffer.push(&Broadcast::BeatTick {
            beat: 42,
            position_beats: 42.5,
            tempo_bpm: 120.0,
        });

        let beat = buffer.latest_beat().unwrap();
        assert_eq!(beat.beat, 42);
        assert_eq!(beat.position_beats, 42.5);
        assert_eq!(beat.tempo_bpm, 120.0);

        // Add transport state
        buffer.push(&Broadcast::TransportStateChanged {
            state: "playing".to_string(),
            position_beats: 42.5,
            tempo_bpm: 120.0,
        });

        let transport = buffer.latest_transport().unwrap();
        assert_eq!(transport.state, "playing");
        assert_eq!(transport.position_beats, 42.5);
        assert_eq!(transport.tempo_bpm, 120.0);

        // Add devices
        buffer.push(&Broadcast::DeviceConnected {
            pipewire_id: 100,
            name: "MIDI Controller".to_string(),
            media_class: Some("Midi/Bridge".to_string()),
            identity_id: None,
            identity_name: None,
        });
        buffer.push(&Broadcast::DeviceConnected {
            pipewire_id: 101,
            name: "Audio Interface".to_string(),
            media_class: Some("Audio/Sink".to_string()),
            identity_id: None,
            identity_name: None,
        });

        assert_eq!(buffer.device_count(), 2);

        // Verify all snapshot data is present and correct
        assert!(buffer.latest_beat().is_some());
        assert!(buffer.latest_transport().is_some());
        assert_eq!(buffer.device_count(), 2);
    }

    #[test]
    fn test_transport_updates_on_each_change() {
        let mut buffer = EventBuffer::new(100);

        // First state: stopped
        buffer.push(&Broadcast::TransportStateChanged {
            state: "stopped".to_string(),
            position_beats: 0.0,
            tempo_bpm: 120.0,
        });
        assert_eq!(buffer.latest_transport().unwrap().state, "stopped");

        // Play
        buffer.push(&Broadcast::TransportStateChanged {
            state: "playing".to_string(),
            position_beats: 0.0,
            tempo_bpm: 120.0,
        });
        assert_eq!(buffer.latest_transport().unwrap().state, "playing");

        // Pause
        buffer.push(&Broadcast::TransportStateChanged {
            state: "paused".to_string(),
            position_beats: 16.0,
            tempo_bpm: 120.0,
        });
        let transport = buffer.latest_transport().unwrap();
        assert_eq!(transport.state, "paused");
        assert_eq!(transport.position_beats, 16.0);
    }
}
