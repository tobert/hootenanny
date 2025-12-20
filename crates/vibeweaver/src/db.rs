//! Sqlite database layer

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use crate::session::{
    Action, HistoryEntry, Marker, MarkerId, Priority, Rule, RuleId, Session, SessionId, Trigger,
};

/// Database wrapper with connection-per-call pattern
pub struct Database {
    path: PathBuf,
    /// For in-memory databases, we keep a persistent connection
    /// since each new in-memory connection creates a fresh database
    memory_conn: Option<Mutex<Connection>>,
}

impl Database {
    /// Open database at path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let db = Self {
            path,
            memory_conn: None,
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Open in-memory database (for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;

        // Set pragmas
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        // Initialize schema
        conn.execute_batch(include_str!("schema.sql"))?;

        Ok(Self {
            path: PathBuf::from(":memory:"),
            memory_conn: Some(Mutex::new(conn)),
        })
    }

    /// Get a connection - for file-based, opens new; for memory, returns ref
    fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        if let Some(ref mutex) = self.memory_conn {
            let conn = mutex.lock().unwrap();
            f(&conn)
        } else {
            let conn = Connection::open(&self.path)?;
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA busy_timeout = 5000;",
            )?;
            f(&conn)
        }
    }

    /// Initialize schema
    pub fn init_schema(&self) -> Result<()> {
        // For memory databases, schema is initialized in open_memory
        if self.memory_conn.is_some() {
            return Ok(());
        }

        self.with_conn(|conn| {
            conn.execute_batch(include_str!("schema.sql"))?;
            Ok(())
        })
    }

    // --- Sessions ---

    pub fn create_session(
        &self,
        name: &str,
        vibe: Option<&str>,
        tempo_bpm: f64,
    ) -> Result<Session> {
        let session = Session::new(name, vibe.map(String::from), tempo_bpm);

        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO sessions (id, name, vibe, tempo_bpm, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session.id.as_str(),
                    session.name,
                    session.vibe,
                    session.tempo_bpm,
                    session.created_at.to_rfc3339(),
                    session.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(session.clone())
        })
    }

    pub fn get_session(&self, id: &SessionId) -> Result<Option<Session>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, vibe, tempo_bpm, created_at, updated_at
                 FROM sessions WHERE id = ?1",
            )?;

            let result = stmt.query_row(params![id.as_str()], |row| {
                Ok(Session {
                    id: SessionId(row.get(0)?),
                    name: row.get(1)?,
                    vibe: row.get(2)?,
                    tempo_bpm: row.get(3)?,
                    created_at: parse_datetime(row.get::<_, String>(4)?),
                    updated_at: parse_datetime(row.get::<_, String>(5)?),
                })
            });

            match result {
                Ok(session) => Ok(Some(session)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    pub fn update_session(&self, session: &Session) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET name = ?2, vibe = ?3, tempo_bpm = ?4, updated_at = ?5
                 WHERE id = ?1",
                params![
                    session.id.as_str(),
                    session.name,
                    session.vibe,
                    session.tempo_bpm,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, vibe, tempo_bpm, created_at, updated_at
                 FROM sessions ORDER BY updated_at DESC",
            )?;

            let sessions = stmt
                .query_map([], |row| {
                    Ok(Session {
                        id: SessionId(row.get(0)?),
                        name: row.get(1)?,
                        vibe: row.get(2)?,
                        tempo_bpm: row.get(3)?,
                        created_at: parse_datetime(row.get::<_, String>(4)?),
                        updated_at: parse_datetime(row.get::<_, String>(5)?),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(sessions)
        })
    }

    // --- Rules ---

    pub fn insert_rule(&self, rule: &Rule) -> Result<()> {
        let trigger_json = serde_json::to_string(&rule.trigger)?;
        let action_json = serde_json::to_string(&rule.action)?;

        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO rules (id, session_id, trigger_type, trigger_params, action_type, action_params,
                                    priority, enabled, one_shot, fired_count, last_fired_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    rule.id.as_str(),
                    rule.session_id.as_str(),
                    rule.trigger.trigger_type(),
                    trigger_json,
                    action_type_str(&rule.action),
                    action_json,
                    rule.priority.as_str(),
                    rule.enabled,
                    rule.one_shot,
                    rule.fired_count,
                    rule.last_fired_at.map(|d| d.to_rfc3339()),
                    rule.created_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_rules_by_session(&self, session_id: &SessionId) -> Result<Vec<Rule>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, trigger_type, trigger_params, action_type, action_params,
                        priority, enabled, one_shot, fired_count, last_fired_at, created_at
                 FROM rules WHERE session_id = ?1 AND enabled = 1
                 ORDER BY priority ASC",
            )?;

            let rules = stmt
                .query_map(params![session_id.as_str()], parse_rule_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(rules)
        })
    }

    pub fn get_rules_by_trigger(
        &self,
        session_id: &SessionId,
        trigger_type: &str,
    ) -> Result<Vec<Rule>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, trigger_type, trigger_params, action_type, action_params,
                        priority, enabled, one_shot, fired_count, last_fired_at, created_at
                 FROM rules WHERE session_id = ?1 AND trigger_type = ?2 AND enabled = 1
                 ORDER BY priority ASC",
            )?;

            let rules = stmt
                .query_map(params![session_id.as_str(), trigger_type], parse_rule_row)?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(rules)
        })
    }

    pub fn update_rule_fired(&self, id: &RuleId, fired_at: DateTime<Utc>) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE rules SET fired_count = fired_count + 1, last_fired_at = ?2 WHERE id = ?1",
                params![id.as_str(), fired_at.to_rfc3339()],
            )?;
            Ok(())
        })
    }

    pub fn delete_rule(&self, id: &RuleId) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM rules WHERE id = ?1", params![id.as_str()])?;
            Ok(())
        })
    }

    pub fn set_rule_enabled(&self, id: &RuleId, enabled: bool) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE rules SET enabled = ?2 WHERE id = ?1",
                params![id.as_str(), enabled],
            )?;
            Ok(())
        })
    }

    // --- Markers ---

    pub fn insert_marker(&self, marker: &Marker) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO markers (id, session_id, beat, name, metadata, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    marker.id.as_str(),
                    marker.session_id.as_str(),
                    marker.beat,
                    marker.name,
                    marker.metadata.as_ref().map(|v| v.to_string()),
                    marker.created_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_markers(&self, session_id: &SessionId) -> Result<Vec<Marker>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, beat, name, metadata, created_at
                 FROM markers WHERE session_id = ?1
                 ORDER BY beat ASC",
            )?;

            let markers = stmt
                .query_map(params![session_id.as_str()], |row| {
                    let metadata_str: Option<String> = row.get(4)?;
                    let metadata = metadata_str.and_then(|s| serde_json::from_str(&s).ok());

                    Ok(Marker {
                        id: MarkerId(row.get(0)?),
                        session_id: SessionId(row.get(1)?),
                        beat: row.get(2)?,
                        name: row.get(3)?,
                        metadata,
                        created_at: parse_datetime(row.get::<_, String>(5)?),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(markers)
        })
    }

    pub fn delete_marker(&self, id: &MarkerId) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM markers WHERE id = ?1", params![id.as_str()])?;
            Ok(())
        })
    }

    // --- History ---

    pub fn append_history(&self, entry: &HistoryEntry) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO history (session_id, action, params, result, success, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    entry.session_id.as_str(),
                    entry.action,
                    entry.params.as_ref().map(|v| v.to_string()),
                    entry.result.as_ref().map(|v| v.to_string()),
                    entry.success,
                    entry.created_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn get_recent_history(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, action, params, result, success, created_at
                 FROM history WHERE session_id = ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )?;

            let entries = stmt
                .query_map(params![session_id.as_str(), limit], |row| {
                    let params_str: Option<String> = row.get(3)?;
                    let params = params_str.and_then(|s| serde_json::from_str(&s).ok());
                    let result_str: Option<String> = row.get(4)?;
                    let result = result_str.and_then(|s| serde_json::from_str(&s).ok());

                    Ok(HistoryEntry {
                        id: row.get(0)?,
                        session_id: SessionId(row.get(1)?),
                        action: row.get(2)?,
                        params,
                        result,
                        success: row.get(5)?,
                        created_at: parse_datetime(row.get::<_, String>(6)?),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(entries)
        })
    }

    // --- Snapshots ---

    pub fn save_snapshot(&self, session_id: &SessionId, state: &[u8]) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO kernel_snapshots (session_id, state_capnp, captured_at)
                 VALUES (?1, ?2, ?3)",
                params![session_id.as_str(), state, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }

    pub fn load_snapshot(&self, session_id: &SessionId) -> Result<Option<Vec<u8>>> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT state_capnp FROM kernel_snapshots WHERE session_id = ?1")?;

            let result = stmt.query_row(params![session_id.as_str()], |row| row.get(0));

            match result {
                Ok(bytes) => Ok(Some(bytes)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }

    // --- Generation stats ---

    pub fn update_generation_stats(&self, space: &str, duration_ms: u64) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO generation_stats (space, avg_duration_ms, sample_count, last_updated)
                 VALUES (?1, ?2, 1, ?3)
                 ON CONFLICT(space) DO UPDATE SET
                    avg_duration_ms = (avg_duration_ms * sample_count + ?2) / (sample_count + 1),
                    sample_count = sample_count + 1,
                    last_updated = ?3",
                params![space, duration_ms, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }

    pub fn get_generation_stats(&self, space: &str) -> Result<Option<(u64, u64)>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT avg_duration_ms, sample_count FROM generation_stats WHERE space = ?1",
            )?;

            let result = stmt.query_row(params![space], |row| Ok((row.get(0)?, row.get(1)?)));

            match result {
                Ok(stats) => Ok(Some(stats)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }
}

// --- Helpers ---

fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn action_type_str(action: &Action) -> &'static str {
    match action {
        Action::Sample { .. } => "sample",
        Action::Schedule { .. } => "schedule",
        Action::SampleAndSchedule { .. } => "sample_and_schedule",
        Action::Play => "play",
        Action::Pause => "pause",
        Action::Stop => "stop",
        Action::Seek { .. } => "seek",
        Action::Audition { .. } => "audition",
        Action::Notify { .. } => "notify",
    }
}

fn parse_rule_row(row: &rusqlite::Row) -> rusqlite::Result<Rule> {
    let trigger_params: String = row.get(3)?;
    let action_params: String = row.get(5)?;
    let priority_str: String = row.get(6)?;
    let last_fired_str: Option<String> = row.get(10)?;

    let trigger: Trigger =
        serde_json::from_str(&trigger_params).map_err(|_| rusqlite::Error::InvalidQuery)?;
    let action: Action =
        serde_json::from_str(&action_params).map_err(|_| rusqlite::Error::InvalidQuery)?;

    Ok(Rule {
        id: RuleId(row.get(0)?),
        session_id: SessionId(row.get(1)?),
        trigger,
        action,
        priority: Priority::parse(&priority_str).unwrap_or_default(),
        enabled: row.get(7)?,
        one_shot: row.get(8)?,
        fired_count: row.get(9)?,
        last_fired_at: last_fired_str.map(parse_datetime),
        created_at: parse_datetime(row.get::<_, String>(11)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_session() {
        let db = Database::open_memory().unwrap();
        let session = db
            .create_session("test", Some("dark techno"), 130.0)
            .unwrap();

        let loaded = db.get_session(&session.id).unwrap().unwrap();
        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.vibe, Some("dark techno".to_string()));
        assert_eq!(loaded.tempo_bpm, 130.0);
    }

    #[test]
    fn test_rules() {
        let db = Database::open_memory().unwrap();
        let session = db.create_session("test", None, 120.0).unwrap();

        let rule = Rule::new(
            session.id.clone(),
            Trigger::Beat { divisor: 4 },
            Action::Notify {
                message: "beat!".to_string(),
            },
        );

        db.insert_rule(&rule).unwrap();

        let rules = db.get_rules_by_session(&session.id).unwrap();
        assert_eq!(rules.len(), 1);

        let rules = db.get_rules_by_trigger(&session.id, "beat").unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_markers() {
        let db = Database::open_memory().unwrap();
        let session = db.create_session("test", None, 120.0).unwrap();

        let marker = Marker::new(session.id.clone(), "drop", 256.0);
        db.insert_marker(&marker).unwrap();

        let markers = db.get_markers(&session.id).unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "drop");
        assert_eq!(markers[0].beat, 256.0);
    }

    #[test]
    fn test_snapshots() {
        let db = Database::open_memory().unwrap();
        let session = db.create_session("test", None, 120.0).unwrap();

        let data = vec![1, 2, 3, 4, 5];
        db.save_snapshot(&session.id, &data).unwrap();

        let loaded = db.load_snapshot(&session.id).unwrap().unwrap();
        assert_eq!(loaded, data);
    }
}
