//! Domain types for capture sessions.
//!
//! Sessions group multiple streams together with timing information and segments.

use crate::streams::StreamUri;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::time::SystemTime;

/// Unique identifier for a capture session
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Generate a new unique session ID
    pub fn generate() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let random = uuid::Uuid::new_v4();
        Self(format!("session-{}-{}", timestamp, random.as_simple()))
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a session segment
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SegmentId(pub String);

impl SegmentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Generate a segment ID for a session and index
    pub fn for_session(session_id: &SessionId, index: usize) -> Self {
        Self(format!("{}-seg-{}", session_id.as_str(), index))
    }
}

impl std::fmt::Display for SegmentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session mode defines how the session operates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionMode {
    /// Continuous capture with retrospective slicing
    Passive,
    /// Send MIDI, capture response
    RequestResponse {
        midi_out: StreamUri,
        audio_in: StreamUri,
    },
}

/// Current status of a capture session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Actively capturing streams
    Recording,
    /// Finalized, no more recording
    Stopped,
}

/// Checkpoint in the session timeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionCheckpoint {
    /// Session started
    Start,
    /// Session stopped
    End,
    /// Named checkpoint for future use
    Named(u32),
}

/// Multi-clock snapshot for timeline correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockSnapshot {
    pub checkpoint: SessionCheckpoint,
    pub wall_clock: SystemTime,
    pub audio_sample_position: Option<u64>,
    pub midi_clock_ticks: Option<u64>,
}

impl ClockSnapshot {
    pub fn now(checkpoint: SessionCheckpoint) -> Self {
        Self {
            checkpoint,
            wall_clock: SystemTime::now(),
            audio_sample_position: None,
            midi_clock_ticks: None,
        }
    }

    pub fn with_audio_position(mut self, position: u64) -> Self {
        self.audio_sample_position = Some(position);
        self
    }

    pub fn with_midi_ticks(mut self, ticks: u64) -> Self {
        self.midi_clock_ticks = Some(ticks);
        self
    }
}

/// Session timeline tracks all available clock sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTimeline {
    pub started_at: SystemTime,
    pub clock_snapshots: Vec<ClockSnapshot>,
}

impl SessionTimeline {
    pub fn new() -> Self {
        Self {
            started_at: SystemTime::now(),
            clock_snapshots: vec![ClockSnapshot::now(SessionCheckpoint::Start)],
        }
    }

    pub fn add_snapshot(&mut self, snapshot: ClockSnapshot) {
        self.clock_snapshots.push(snapshot);
    }

    pub fn end(&mut self) {
        self.clock_snapshots
            .push(ClockSnapshot::now(SessionCheckpoint::End));
    }
}

impl Default for SessionTimeline {
    fn default() -> Self {
        Self::new()
    }
}

/// A segment is a contiguous recording period within a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSegment {
    pub id: SegmentId,
    pub started_at: ClockSnapshot,
    pub ended_at: Option<ClockSnapshot>,
    pub chunk_range: Range<usize>,
}

impl SessionSegment {
    pub fn new(id: SegmentId, started_at: ClockSnapshot) -> Self {
        Self {
            id,
            started_at,
            ended_at: None,
            chunk_range: 0..0,
        }
    }

    pub fn end(&mut self, ended_at: ClockSnapshot) {
        self.ended_at = Some(ended_at);
    }

    pub fn is_active(&self) -> bool {
        self.ended_at.is_none()
    }
}

/// A capture session groups multiple streams with timing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSession {
    pub id: SessionId,
    pub mode: SessionMode,
    pub streams: Vec<StreamUri>,
    pub segments: Vec<SessionSegment>,
    pub timeline: SessionTimeline,
    pub status: SessionStatus,
}

impl CaptureSession {
    pub fn new(id: SessionId, mode: SessionMode, streams: Vec<StreamUri>) -> Self {
        Self {
            id,
            mode,
            streams,
            segments: Vec::new(),
            timeline: SessionTimeline::new(),
            status: SessionStatus::Recording,
        }
    }

    /// Start a new segment
    pub fn start_segment(&mut self) {
        let segment_id = SegmentId::for_session(&self.id, self.segments.len());
        let snapshot = ClockSnapshot::now(SessionCheckpoint::Start);
        let segment = SessionSegment::new(segment_id, snapshot);
        self.segments.push(segment);
    }

    /// End the current segment
    pub fn end_current_segment(&mut self) {
        if let Some(segment) = self.segments.last_mut() {
            if segment.is_active() {
                let snapshot = ClockSnapshot::now(SessionCheckpoint::End);
                segment.end(snapshot);
            }
        }
    }

    /// Stop the session
    pub fn stop(&mut self) {
        self.end_current_segment();
        self.timeline.end();
        self.status = SessionStatus::Stopped;
    }

    /// Get the current active segment, if any
    pub fn current_segment(&self) -> Option<&SessionSegment> {
        self.segments.last().filter(|s| s.is_active())
    }

    /// Get the current active segment mutably
    pub fn current_segment_mut(&mut self) -> Option<&mut SessionSegment> {
        self.segments.last_mut().filter(|s| s.is_active())
    }

    pub fn is_recording(&self) -> bool {
        self.status == SessionStatus::Recording
    }

    pub fn is_stopped(&self) -> bool {
        self.status == SessionStatus::Stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_generation() {
        let id1 = SessionId::generate();
        let id2 = SessionId::generate();

        assert_ne!(id1, id2);
        assert!(id1.as_str().starts_with("session-"));
    }

    #[test]
    fn test_segment_id_for_session() {
        let session_id = SessionId::new("test-session");
        let seg_id = SegmentId::for_session(&session_id, 0);

        assert_eq!(seg_id.as_str(), "test-session-seg-0");
    }

    #[test]
    fn test_clock_snapshot() {
        let snapshot = ClockSnapshot::now(SessionCheckpoint::Start)
            .with_audio_position(12345)
            .with_midi_ticks(678);

        assert_eq!(snapshot.checkpoint, SessionCheckpoint::Start);
        assert_eq!(snapshot.audio_sample_position, Some(12345));
        assert_eq!(snapshot.midi_clock_ticks, Some(678));
    }

    #[test]
    fn test_session_timeline() {
        let mut timeline = SessionTimeline::new();

        assert_eq!(timeline.clock_snapshots.len(), 1);
        assert_eq!(
            timeline.clock_snapshots[0].checkpoint,
            SessionCheckpoint::Start
        );

        timeline.add_snapshot(ClockSnapshot::now(SessionCheckpoint::Named(1)));
        assert_eq!(timeline.clock_snapshots.len(), 2);

        timeline.end();
        assert_eq!(timeline.clock_snapshots.len(), 3);
        assert_eq!(
            timeline.clock_snapshots[2].checkpoint,
            SessionCheckpoint::End
        );
    }

    #[test]
    fn test_session_segment() {
        let id = SegmentId::new("seg-1");
        let mut segment = SessionSegment::new(id.clone(), ClockSnapshot::now(SessionCheckpoint::Start));

        assert_eq!(segment.id, id);
        assert!(segment.is_active());

        segment.end(ClockSnapshot::now(SessionCheckpoint::End));
        assert!(!segment.is_active());
    }

    #[test]
    fn test_capture_session_lifecycle() {
        let session_id = SessionId::new("test-session");
        let streams = vec![StreamUri::from("stream://test/audio")];
        let mut session = CaptureSession::new(session_id.clone(), SessionMode::Passive, streams);

        assert_eq!(session.id, session_id);
        assert!(session.is_recording());
        assert!(!session.is_stopped());
        assert_eq!(session.segments.len(), 0);

        // Start first segment
        session.start_segment();
        assert_eq!(session.segments.len(), 1);
        assert!(session.current_segment().unwrap().is_active());

        // End first segment
        session.end_current_segment();
        assert!(!session.segments[0].is_active());

        // Start second segment
        session.start_segment();
        assert_eq!(session.segments.len(), 2);
        assert!(session.current_segment().unwrap().is_active());

        // Stop session
        session.stop();
        assert!(!session.is_recording());
        assert!(session.is_stopped());
        assert!(!session.segments[1].is_active());
    }

    #[test]
    fn test_passive_mode() {
        let session_id = SessionId::new("passive-session");
        let streams = vec![StreamUri::from("stream://test/audio")];
        let session = CaptureSession::new(session_id, SessionMode::Passive, streams);

        assert!(matches!(session.mode, SessionMode::Passive));
    }

    #[test]
    fn test_request_response_mode() {
        let session_id = SessionId::new("rr-session");
        let midi_out = StreamUri::from("stream://test/midi-out");
        let audio_in = StreamUri::from("stream://test/audio-in");
        let streams = vec![midi_out.clone(), audio_in.clone()];

        let session = CaptureSession::new(
            session_id,
            SessionMode::RequestResponse {
                midi_out: midi_out.clone(),
                audio_in: audio_in.clone(),
            },
            streams,
        );

        match &session.mode {
            SessionMode::RequestResponse {
                midi_out: m,
                audio_in: a,
            } => {
                assert_eq!(m, &midi_out);
                assert_eq!(a, &audio_in);
            }
            _ => panic!("expected RequestResponse mode"),
        }
    }
}
