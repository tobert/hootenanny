# 02-session: Session + Sqlite

**File:** `crates/vibeweaver/src/db.rs`, `crates/vibeweaver/src/session.rs`
**Dependencies:** None
**Unblocks:** 04-scheduler, 05-api

---

## Task

Create sqlite database layer and session management following the connection-per-call pattern from audio-graph-mcp.

## Deliverables

- `crates/vibeweaver/src/db.rs` - Database wrapper
- `crates/vibeweaver/src/session.rs` - Session types
- Schema initialization
- Unit tests

## Types

```rust
use rusqlite::Connection;
use std::path::Path;
use chrono::{DateTime, Utc};
use anyhow::Result;

// --- IDs ---

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MarkerId(pub String);

// --- Session ---

#[derive(Debug, Clone)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub vibe: Option<String>,
    pub tempo_bpm: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// --- Rules ---

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: RuleId,
    pub session_id: SessionId,
    pub trigger: Trigger,
    pub action: Action,
    pub priority: Priority,
    pub enabled: bool,
    pub one_shot: bool,
    pub fired_count: u64,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Trigger {
    Beat { divisor: u32 },
    Marker { name: String },
    Deadline { beat: f64 },
    Artifact { tag: Option<String> },
    JobComplete { job_id: String },
    Transport { state: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    Sample { space: String, prompt: Option<String>, inference: serde_json::Value },
    Schedule { content_hash: String, at: f64, duration: Option<f64>, gain: f64 },
    SampleAndSchedule { space: String, prompt: Option<String>, at: f64 },
    Play,
    Pause,
    Stop,
    Seek { beat: f64 },
    Audition { content_hash: String, duration: f64 },
    Notify { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
    Idle = 4,
}

// --- Markers ---

#[derive(Debug, Clone)]
pub struct Marker {
    pub id: MarkerId,
    pub session_id: SessionId,
    pub beat: f64,
    pub name: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

// --- History ---

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub session_id: SessionId,
    pub action: String,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub success: bool,
    pub created_at: DateTime<Utc>,
}

// --- Database ---

pub struct Database {
    path: std::path::PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;
    pub fn open_memory() -> Result<Self>;

    /// Get a new connection (connection-per-call pattern)
    fn conn(&self) -> Result<Connection>;

    /// Initialize schema
    pub fn init_schema(&self) -> Result<()>;

    // Sessions
    pub fn create_session(&self, name: &str, vibe: Option<&str>, tempo_bpm: f64) -> Result<Session>;
    pub fn get_session(&self, id: &SessionId) -> Result<Option<Session>>;
    pub fn update_session(&self, session: &Session) -> Result<()>;
    pub fn list_sessions(&self) -> Result<Vec<Session>>;

    // Rules
    pub fn insert_rule(&self, rule: &Rule) -> Result<()>;
    pub fn get_rules_by_session(&self, session_id: &SessionId) -> Result<Vec<Rule>>;
    pub fn get_rules_by_trigger(&self, session_id: &SessionId, trigger_type: &str) -> Result<Vec<Rule>>;
    pub fn update_rule_fired(&self, id: &RuleId, fired_at: DateTime<Utc>) -> Result<()>;
    pub fn delete_rule(&self, id: &RuleId) -> Result<()>;
    pub fn set_rule_enabled(&self, id: &RuleId, enabled: bool) -> Result<()>;

    // Markers
    pub fn insert_marker(&self, marker: &Marker) -> Result<()>;
    pub fn get_markers(&self, session_id: &SessionId) -> Result<Vec<Marker>>;
    pub fn delete_marker(&self, id: &MarkerId) -> Result<()>;

    // History
    pub fn append_history(&self, entry: &HistoryEntry) -> Result<()>;
    pub fn get_recent_history(&self, session_id: &SessionId, limit: usize) -> Result<Vec<HistoryEntry>>;

    // Snapshots
    pub fn save_snapshot(&self, session_id: &SessionId, state: &[u8]) -> Result<()>;
    pub fn load_snapshot(&self, session_id: &SessionId) -> Result<Option<Vec<u8>>>;

    // Generation stats
    pub fn update_generation_stats(&self, space: &str, duration_ms: u64) -> Result<()>;
    pub fn get_generation_stats(&self, space: &str) -> Result<Option<(u64, u64)>>; // (avg_ms, count)
}
```

## Schema

See DETAIL.md for full schema. Key points:
- WAL mode for concurrency
- Foreign keys enabled
- Indexes on session_id and trigger_type

## Definition of Done

```bash
cargo fmt --check -p vibeweaver
cargo clippy -p vibeweaver -- -D warnings
cargo test -p vibeweaver db::
cargo test -p vibeweaver session::
```

## Acceptance Criteria

- [ ] Create session, retrieve by ID
- [ ] Insert rule, query by trigger type
- [ ] Markers sorted by beat
- [ ] History entries with pagination
- [ ] Snapshot save/load roundtrip
