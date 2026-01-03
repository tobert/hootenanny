//! Broadcast handler - updates KernelState from hootenanny broadcasts

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{oneshot, RwLock};

use crate::state::{ArtifactInfo, JobState, KernelState, Transport};
use crate::zmq_client::Broadcast;

/// State update events
#[derive(Debug, Clone)]
pub enum StateUpdate {
    TransportChanged {
        state: Transport,
        position: f64,
    },
    BeatTick {
        beat: f64,
        tempo: f64,
    },
    JobStateChanged {
        job_id: String,
        state: JobState,
        artifact_id: Option<String>,
    },
    ArtifactCreated {
        artifact: ArtifactInfo,
    },
}

/// Pending job completion futures
type JobWaiters = HashMap<String, oneshot::Sender<ArtifactInfo>>;

/// Global broadcast handler instance
static HANDLER: std::sync::OnceLock<BroadcastHandler> = std::sync::OnceLock::new();

/// Handles broadcasts and updates kernel state
pub struct BroadcastHandler {
    state: Arc<RwLock<KernelState>>,
    job_waiters: Arc<RwLock<JobWaiters>>,
}

impl BroadcastHandler {
    pub fn new(initial_state: KernelState) -> Self {
        Self {
            state: Arc::new(RwLock::new(initial_state)),
            job_waiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize the global handler (call once at startup)
    pub fn init_global(handler: BroadcastHandler) -> anyhow::Result<()> {
        HANDLER
            .set(handler)
            .map_err(|_| anyhow::anyhow!("BroadcastHandler already initialized"))
    }

    /// Get the global handler
    pub fn global() -> Option<&'static BroadcastHandler> {
        HANDLER.get()
    }

    /// Get current state (for Python injection)
    pub async fn state(&self) -> KernelState {
        self.state.read().await.clone()
    }

    /// Register a waiter for job completion
    pub async fn wait_for_job(&self, job_id: String) -> oneshot::Receiver<ArtifactInfo> {
        let (tx, rx) = oneshot::channel();
        self.job_waiters.write().await.insert(job_id, tx);
        rx
    }

    /// Process a broadcast, return state updates
    pub async fn handle(&self, broadcast: Broadcast) -> Vec<StateUpdate> {
        let mut updates = Vec::new();

        match broadcast {
            Broadcast::TransportStateChanged {
                state,
                position_beats,
            } => {
                let transport = Transport::parse(&state);
                self.apply_transport(transport, position_beats).await;
                updates.push(StateUpdate::TransportChanged {
                    state: transport,
                    position: position_beats,
                });
            }

            Broadcast::BeatTick { beat, tempo_bpm } => {
                self.apply_beat(beat, tempo_bpm).await;
                updates.push(StateUpdate::BeatTick {
                    beat,
                    tempo: tempo_bpm,
                });
            }

            Broadcast::JobStateChanged {
                job_id,
                state,
                artifact_id,
            } => {
                let job_state = JobState::parse(&state);

                // Update state
                self.state
                    .write()
                    .await
                    .update_job(job_id.clone(), job_state, artifact_id.clone());

                updates.push(StateUpdate::JobStateChanged {
                    job_id: job_id.clone(),
                    state: job_state,
                    artifact_id: artifact_id.clone(),
                });

                // Resolve waiters if complete
                if job_state == JobState::Complete {
                    if let Some(artifact_id) = artifact_id {
                        let artifact = ArtifactInfo {
                            id: artifact_id,
                            content_hash: String::new(), // TODO: Get from broadcast
                            tags: vec![],
                            created_at: Utc::now(),
                        };
                        self.resolve_job(&job_id, artifact).await;
                    }
                }
            }

            Broadcast::ArtifactCreated {
                artifact_id,
                content_hash,
                tags,
            } => {
                let artifact = ArtifactInfo {
                    id: artifact_id,
                    content_hash,
                    tags,
                    created_at: Utc::now(),
                };

                self.state.write().await.add_artifact(artifact.clone());

                updates.push(StateUpdate::ArtifactCreated { artifact });
            }

            Broadcast::MarkerReached { .. } => {
                // Markers are handled by scheduler, not state
            }

            Broadcast::Unknown { .. } => {
                // Ignore unknown broadcasts
            }
        }

        updates
    }

    /// Snapshot state for persistence
    pub async fn snapshot(&self) -> KernelState {
        let mut state = self.state.read().await.clone();
        state.captured_at = Utc::now();
        state
    }

    /// Restore from snapshot
    pub async fn restore(&self, state: KernelState) {
        *self.state.write().await = state;
    }

    /// Update transport state
    async fn apply_transport(&self, state: Transport, position: f64) {
        let mut kernel_state = self.state.write().await;
        kernel_state.transport = state;
        kernel_state.beat.current = position;
    }

    /// Update beat state
    async fn apply_beat(&self, beat: f64, tempo: f64) {
        let mut state = self.state.write().await;
        state.beat.current = beat;
        state.beat.tempo_bpm = tempo;
        state.tempo_bpm = tempo;
    }

    /// Resolve job waiters
    async fn resolve_job(&self, job_id: &str, artifact: ArtifactInfo) {
        if let Some(tx) = self.job_waiters.write().await.remove(job_id) {
            let _ = tx.send(artifact);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionId;

    #[tokio::test]
    async fn test_beat_updates() {
        let initial = KernelState::new(SessionId::new(), "test".to_string(), 120.0);
        let handler = BroadcastHandler::new(initial);

        let broadcast = Broadcast::BeatTick {
            beat: 4.0,
            tempo_bpm: 130.0,
        };

        let updates = handler.handle(broadcast).await;
        assert_eq!(updates.len(), 1);

        let state = handler.state().await;
        assert_eq!(state.beat.current, 4.0);
        assert_eq!(state.beat.tempo_bpm, 130.0);
    }

    #[tokio::test]
    async fn test_job_waiter() {
        let initial = KernelState::new(SessionId::new(), "test".to_string(), 120.0);
        let handler = BroadcastHandler::new(initial);

        // Register waiter
        let rx = handler.wait_for_job("job_123".to_string()).await;

        // Simulate job completion
        let broadcast = Broadcast::JobStateChanged {
            job_id: "job_123".to_string(),
            state: "complete".to_string(),
            artifact_id: Some("artifact_456".to_string()),
        };

        handler.handle(broadcast).await;

        // Waiter should receive artifact
        let artifact = rx.await.unwrap();
        assert_eq!(artifact.id, "artifact_456");
    }

    #[tokio::test]
    async fn test_artifact_limit() {
        let initial = KernelState::new(SessionId::new(), "test".to_string(), 120.0);
        let handler = BroadcastHandler::new(initial);

        // Add many artifacts
        for i in 0..150 {
            let broadcast = Broadcast::ArtifactCreated {
                artifact_id: format!("art_{}", i),
                content_hash: format!("hash_{}", i),
                tags: vec![],
            };
            handler.handle(broadcast).await;
        }

        let state = handler.state().await;
        assert_eq!(state.recent_artifacts.len(), 100);
    }
}
