//! Session manager - coordinates capture sessions across multiple streams.

use super::types::{
    CaptureSession, ClockSnapshot, SessionCheckpoint, SessionId, SessionMode, SessionStatus,
};
use crate::streams::{StreamManager, StreamUri};
use anyhow::{Context, Result};
use cas::{ContentHash, ContentStore, FileStore};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Active session state
struct ActiveSession {
    session: CaptureSession,
}

/// Manager for capture session lifecycle
pub struct SessionManager {
    cas: Arc<FileStore>,
    stream_manager: Arc<StreamManager>,
    active_sessions: Arc<RwLock<HashMap<SessionId, ActiveSession>>>,
}

impl SessionManager {
    pub fn new(cas: Arc<FileStore>, stream_manager: Arc<StreamManager>) -> Self {
        Self {
            cas,
            stream_manager,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new capture session
    ///
    /// The session is created but not started. Call `play()` to begin recording.
    pub fn create_session(
        &self,
        mode: SessionMode,
        streams: Vec<StreamUri>,
    ) -> Result<SessionId> {
        let session_id = SessionId::generate();

        let mut sessions = self.active_sessions.write().unwrap();

        if sessions.contains_key(&session_id) {
            anyhow::bail!("session already exists: {}", session_id);
        }

        let session = CaptureSession::new(session_id.clone(), mode, streams);

        info!("created session: {}", session_id);

        sessions.insert(
            session_id.clone(),
            ActiveSession {
                session,
            },
        );

        Ok(session_id)
    }

    /// Start recording (play) - begins a new segment
    ///
    /// Starts all streams in the session and creates a new segment.
    pub fn play(&self, session_id: &SessionId) -> Result<()> {
        let mut sessions = self.active_sessions.write().unwrap();

        let active = sessions
            .get_mut(session_id)
            .with_context(|| format!("session not found: {}", session_id))?;

        if active.session.status != SessionStatus::Recording {
            anyhow::bail!("session is not in recording state: {}", session_id);
        }

        // Start new segment
        active.session.start_segment();

        info!(
            "started segment {} for session: {}",
            active.session.segments.len() - 1,
            session_id
        );

        Ok(())
    }

    /// Pause recording - ends the current segment
    ///
    /// This is effectively the same as ending a segment. To resume, call `play()` again
    /// which will create a new segment.
    pub fn pause(&self, session_id: &SessionId) -> Result<()> {
        let mut sessions = self.active_sessions.write().unwrap();

        let active = sessions
            .get_mut(session_id)
            .with_context(|| format!("session not found: {}", session_id))?;

        active.session.end_current_segment();

        info!("paused session: {} (ended current segment)", session_id);

        Ok(())
    }

    /// Stop the session - finalize and archive
    ///
    /// Stops all streams, ends the current segment, and stores the session artifact.
    /// Returns the content hash of the stored session.
    pub fn stop(&self, session_id: &SessionId) -> Result<ContentHash> {
        let mut sessions = self.active_sessions.write().unwrap();

        let mut active = sessions
            .remove(session_id)
            .with_context(|| format!("session not found: {}", session_id))?;

        // Stop the session (ends segment, updates timeline)
        active.session.stop();

        // Store session as artifact
        let session_json = serde_json::to_vec(&active.session)
            .context("failed to serialize session")?;
        let session_hash = self
            .cas
            .store(&session_json, "application/json")
            .context("failed to store session")?;

        info!(
            "stopped session: {} (hash: {})",
            session_id, session_hash
        );

        Ok(session_hash)
    }

    /// Get session info
    pub fn get_session(&self, session_id: &SessionId) -> Result<Option<CaptureSession>> {
        let sessions = self.active_sessions.read().unwrap();
        Ok(sessions.get(session_id).map(|s| s.session.clone()))
    }

    /// Get list of active session IDs
    pub fn active_sessions(&self) -> Vec<SessionId> {
        let sessions = self.active_sessions.read().unwrap();
        sessions.keys().cloned().collect()
    }

    /// Get session status
    pub fn session_status(&self, session_id: &SessionId) -> Option<SessionStatus> {
        let sessions = self.active_sessions.read().unwrap();
        sessions.get(session_id).map(|s| s.session.status)
    }

    /// Update timeline with a clock snapshot
    pub fn add_clock_snapshot(&self, session_id: &SessionId, snapshot: ClockSnapshot) -> Result<()> {
        let mut sessions = self.active_sessions.write().unwrap();

        let active = sessions
            .get_mut(session_id)
            .with_context(|| format!("session not found: {}", session_id))?;

        debug!(
            "added clock snapshot to session: {} (checkpoint: {:?})",
            session_id, snapshot.checkpoint
        );

        active.session.timeline.add_snapshot(snapshot);

        Ok(())
    }

    /// Update the chunk range for the current segment
    ///
    /// This should be called when chunks are added to streams in the session.
    pub fn update_segment_chunk_range(
        &self,
        session_id: &SessionId,
        chunk_range: std::ops::Range<usize>,
    ) -> Result<()> {
        let mut sessions = self.active_sessions.write().unwrap();

        let active = sessions
            .get_mut(session_id)
            .with_context(|| format!("session not found: {}", session_id))?;

        if let Some(segment) = active.session.current_segment_mut() {
            segment.chunk_range = chunk_range;
            debug!(
                "updated chunk range for session {}: {:?}",
                session_id, segment.chunk_range
            );
        } else {
            anyhow::bail!("no active segment in session: {}", session_id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::{SessionCheckpoint, SessionMode};
    use tempfile::TempDir;

    fn setup_test_managers() -> (TempDir, Arc<FileStore>, Arc<StreamManager>, SessionManager) {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(FileStore::at_path(temp_dir.path()).unwrap());
        let stream_manager = Arc::new(StreamManager::new(store.clone()));
        let session_manager = SessionManager::new(store.clone(), stream_manager.clone());
        (temp_dir, store, stream_manager, session_manager)
    }

    #[test]
    fn test_create_session() {
        let (_temp, _store, _stream_mgr, session_mgr) = setup_test_managers();

        let streams = vec![StreamUri::from("stream://test/audio")];
        let session_id = session_mgr
            .create_session(SessionMode::Passive, streams)
            .unwrap();

        assert!(session_id.as_str().starts_with("session-"));

        let active = session_mgr.active_sessions();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0], session_id);

        let status = session_mgr.session_status(&session_id);
        assert_eq!(status, Some(SessionStatus::Recording));
    }

    #[test]
    fn test_session_lifecycle() {
        let (_temp, _store, _stream_mgr, session_mgr) = setup_test_managers();

        let streams = vec![StreamUri::from("stream://test/audio")];
        let session_id = session_mgr
            .create_session(SessionMode::Passive, streams)
            .unwrap();

        // Get session info
        let session = session_mgr.get_session(&session_id).unwrap().unwrap();
        assert_eq!(session.id, session_id);
        assert_eq!(session.segments.len(), 0);

        // Start recording (first segment)
        session_mgr.play(&session_id).unwrap();

        let session = session_mgr.get_session(&session_id).unwrap().unwrap();
        assert_eq!(session.segments.len(), 1);
        assert!(session.current_segment().unwrap().is_active());

        // Pause (end first segment)
        session_mgr.pause(&session_id).unwrap();

        let session = session_mgr.get_session(&session_id).unwrap().unwrap();
        assert!(!session.segments[0].is_active());

        // Resume (start second segment)
        session_mgr.play(&session_id).unwrap();

        let session = session_mgr.get_session(&session_id).unwrap().unwrap();
        assert_eq!(session.segments.len(), 2);
        assert!(session.current_segment().unwrap().is_active());

        // Stop session
        let session_hash = session_mgr.stop(&session_id).unwrap();
        assert!(!session_hash.to_string().is_empty());

        let active = session_mgr.active_sessions();
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn test_clock_snapshot() {
        let (_temp, _store, _stream_mgr, session_mgr) = setup_test_managers();

        let streams = vec![StreamUri::from("stream://test/audio")];
        let session_id = session_mgr
            .create_session(SessionMode::Passive, streams)
            .unwrap();

        let snapshot = ClockSnapshot::now(SessionCheckpoint::Named(1))
            .with_audio_position(12345)
            .with_midi_ticks(678);

        session_mgr
            .add_clock_snapshot(&session_id, snapshot.clone())
            .unwrap();

        let session = session_mgr.get_session(&session_id).unwrap().unwrap();
        assert_eq!(session.timeline.clock_snapshots.len(), 2); // Start + Named(1)
        assert_eq!(
            session.timeline.clock_snapshots[1].audio_sample_position,
            Some(12345)
        );
    }

    #[test]
    fn test_update_segment_chunk_range() {
        let (_temp, _store, _stream_mgr, session_mgr) = setup_test_managers();

        let streams = vec![StreamUri::from("stream://test/audio")];
        let session_id = session_mgr
            .create_session(SessionMode::Passive, streams)
            .unwrap();

        // Start a segment
        session_mgr.play(&session_id).unwrap();

        // Update chunk range
        session_mgr
            .update_segment_chunk_range(&session_id, 0..5)
            .unwrap();

        let session = session_mgr.get_session(&session_id).unwrap().unwrap();
        assert_eq!(session.current_segment().unwrap().chunk_range, 0..5);
    }

    #[test]
    fn test_request_response_mode() {
        let (_temp, _store, _stream_mgr, session_mgr) = setup_test_managers();

        let midi_out = StreamUri::from("stream://test/midi-out");
        let audio_in = StreamUri::from("stream://test/audio-in");
        let streams = vec![midi_out.clone(), audio_in.clone()];

        let session_id = session_mgr
            .create_session(
                SessionMode::RequestResponse {
                    midi_out: midi_out.clone(),
                    audio_in: audio_in.clone(),
                },
                streams,
            )
            .unwrap();

        let session = session_mgr.get_session(&session_id).unwrap().unwrap();

        match session.mode {
            SessionMode::RequestResponse {
                midi_out: ref m,
                audio_in: ref a,
            } => {
                assert_eq!(m, &midi_out);
                assert_eq!(a, &audio_in);
            }
            _ => panic!("expected RequestResponse mode"),
        }
    }

    #[test]
    fn test_stop_nonexistent_session() {
        let (_temp, _store, _stream_mgr, session_mgr) = setup_test_managers();

        let session_id = SessionId::new("nonexistent");
        let result = session_mgr.stop(&session_id);

        assert!(result.is_err());
    }

    #[test]
    fn test_play_without_segment() {
        let (_temp, _store, _stream_mgr, session_mgr) = setup_test_managers();

        let streams = vec![StreamUri::from("stream://test/audio")];
        let session_id = session_mgr
            .create_session(SessionMode::Passive, streams)
            .unwrap();

        // Play should create first segment
        session_mgr.play(&session_id).unwrap();

        let session = session_mgr.get_session(&session_id).unwrap().unwrap();
        assert_eq!(session.segments.len(), 1);
    }
}
