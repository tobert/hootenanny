# 06-broadcast: Broadcast Handler

**File:** `crates/vibeweaver/src/broadcast.rs`
**Dependencies:** 03-zmq
**Unblocks:** 07-mcp

---

## Task

Handle incoming broadcasts from hootenanny, update KernelState, resolve pending futures.

## Deliverables

- `crates/vibeweaver/src/broadcast.rs`
- `crates/vibeweaver/src/state.rs` (KernelState)
- Integration with scheduler

## Types

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, oneshot};
use chrono::{DateTime, Utc};
use crate::zmq::Broadcast;
use crate::db::SessionId;
use anyhow::Result;

// --- KernelState (the latent space) ---

/// The world Python sees when it wakes
#[derive(Debug, Clone)]
pub struct KernelState {
    pub session_id: SessionId,
    pub session_name: String,
    pub session_vibe: Option<String>,
    pub tempo_bpm: f64,

    pub transport: Transport,
    pub beat: BeatInfo,

    pub jobs: HashMap<String, JobInfo>,
    pub recent_artifacts: Vec<ArtifactInfo>,

    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub struct BeatInfo {
    pub current: f64,
    pub tempo_bpm: f64,
}

#[derive(Debug, Clone)]
pub struct JobInfo {
    pub job_id: String,
    pub state: JobState,
    pub artifact_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Pending,
    Running,
    Complete,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ArtifactInfo {
    pub id: String,
    pub content_hash: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

// --- State updates ---

#[derive(Debug, Clone)]
pub enum StateUpdate {
    TransportChanged { state: Transport, position: f64 },
    BeatTick { beat: f64, tempo: f64 },
    JobStateChanged { job_id: String, state: JobState, artifact_id: Option<String> },
    ArtifactCreated { artifact: ArtifactInfo },
}

// --- Broadcast handler ---

/// Pending job completion futures
type JobWaiters = HashMap<String, oneshot::Sender<ArtifactInfo>>;

pub struct BroadcastHandler {
    state: Arc<RwLock<KernelState>>,
    job_waiters: Arc<RwLock<JobWaiters>>,
}

impl BroadcastHandler {
    pub fn new(initial_state: KernelState) -> Self;

    /// Get current state (for Python injection)
    pub async fn state(&self) -> KernelState;

    /// Register a waiter for job completion
    pub async fn wait_for_job(&self, job_id: String) -> oneshot::Receiver<ArtifactInfo>;

    /// Process a broadcast, return state updates
    pub async fn handle(&self, broadcast: Broadcast) -> Vec<StateUpdate>;

    /// Snapshot state for persistence
    pub async fn snapshot(&self) -> KernelState;

    /// Restore from snapshot
    pub async fn restore(&self, state: KernelState);
}

impl BroadcastHandler {
    /// Internal: update state from broadcast
    async fn apply_update(&self, update: StateUpdate);

    /// Internal: resolve job waiters
    async fn resolve_job(&self, job_id: &str, artifact: ArtifactInfo);
}

// --- Serialization for snapshots ---

impl KernelState {
    /// Serialize to Cap'n Proto bytes
    pub fn to_capnp(&self) -> Result<Vec<u8>>;

    /// Deserialize from Cap'n Proto bytes
    pub fn from_capnp(bytes: &[u8]) -> Result<Self>;
}
```

## Broadcast Mapping

| Broadcast | StateUpdate | Additional Action |
|-----------|-------------|-------------------|
| `JobStateChanged` | `JobStateChanged` | Resolve waiters if Complete |
| `ArtifactCreated` | `ArtifactCreated` | Add to recent_artifacts |
| `TransportStateChanged` | `TransportChanged` | — |
| `BeatTick` | `BeatTick` | — |
| `MarkerReached` | — | Scheduler handles |

## Definition of Done

```bash
cargo fmt --check -p vibeweaver
cargo clippy -p vibeweaver -- -D warnings
cargo test -p vibeweaver broadcast::
cargo test -p vibeweaver state::
```

## Acceptance Criteria

- [ ] State updates on broadcast
- [ ] Job waiters resolve on completion
- [ ] Recent artifacts limited (e.g., last 100)
- [ ] Snapshot/restore roundtrip
- [ ] Cap'n Proto serialization works
