use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

use crate::types::*;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS identities (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    data JSON NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS identity_hints (
    identity_id TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    hint_kind TEXT NOT NULL,
    hint_value TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (hint_kind, hint_value)
);
CREATE INDEX IF NOT EXISTS idx_hints_identity ON identity_hints(identity_id);

CREATE TABLE IF NOT EXISTS tags (
    identity_id TEXT NOT NULL REFERENCES identities(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (identity_id, namespace, value)
);
CREATE INDEX IF NOT EXISTS idx_tags_ns_val ON tags(namespace, value);

CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    source TEXT NOT NULL,
    message TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notes_target ON notes(target_kind, target_id);

CREATE TABLE IF NOT EXISTS manual_connections (
    id TEXT PRIMARY KEY,
    from_identity TEXT NOT NULL,
    from_port TEXT NOT NULL,
    to_identity TEXT NOT NULL,
    to_port TEXT NOT NULL,
    transport_kind TEXT,
    signal_direction TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL,
    UNIQUE (from_identity, from_port, to_identity, to_port)
);

CREATE TABLE IF NOT EXISTS changelog (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    source TEXT NOT NULL,
    operation TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    details JSON NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_changelog_target ON changelog(target_kind, target_id);
CREATE INDEX IF NOT EXISTS idx_changelog_time ON changelog(timestamp DESC);
"#;

/// Database with connection-per-call for concurrent access.
/// Each method creates a fresh connection with WAL mode enabled.
pub struct Database {
    path: PathBuf,
}

impl Database {
    /// Open a file-based database with WAL mode for concurrent access.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let db = Self { path };
        // Initialize schema with first connection
        let conn = db.conn()?;
        conn.execute_batch(SCHEMA)?;
        Ok(db)
    }

    /// Create a temporary database file with a unique name.
    /// Each call creates a new database - suitable for tests.
    pub fn in_memory() -> Result<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let temp_dir = std::env::temp_dir();
        let unique_id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let db_name = format!("audio_graph_{}_{}.db", std::process::id(), unique_id);
        let path = temp_dir.join(db_name);
        Self::open(path)
    }

    /// Create a new connection with WAL mode and busy timeout.
    fn conn(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path)
            .with_context(|| format!("Failed to open database: {:?}", self.path))?;
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA foreign_keys = ON;
        ")?;
        Ok(conn)
    }

    // Identity CRUD
    pub fn create_identity(&self, id: &str, name: &str, data: serde_json::Value) -> Result<Identity> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO identities (id, name, data) VALUES (?1, ?2, ?3)",
            params![id, name, data.to_string()],
        )?;
        self.get_identity(id)?.ok_or_else(|| anyhow::anyhow!("Failed to retrieve created identity"))
    }

    pub fn get_identity(&self, id: &str) -> Result<Option<Identity>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, name, created_at, data FROM identities WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Identity {
                id: IdentityId(row.get(0)?),
                name: row.get(1)?,
                created_at: row.get(2)?,
                data: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_identities(&self) -> Result<Vec<Identity>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, name, created_at, data FROM identities")?;
        let rows = stmt.query_map([], |row| {
            Ok(Identity {
                id: IdentityId(row.get(0)?),
                name: row.get(1)?,
                created_at: row.get(2)?,
                data: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().context("Failed to list identities")
    }

    pub fn delete_identity(&self, id: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM identities WHERE id = ?1", params![id])?;
        Ok(())
    }

    // Hints
    pub fn add_hint(&self, identity_id: &str, kind: HintKind, value: &str, confidence: f64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO identity_hints (identity_id, hint_kind, hint_value, confidence) VALUES (?1, ?2, ?3, ?4)",
            params![identity_id, kind.as_str(), value, confidence],
        )?;
        Ok(())
    }

    pub fn get_hints(&self, identity_id: &str) -> Result<Vec<IdentityHint>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT identity_id, hint_kind, hint_value, confidence FROM identity_hints WHERE identity_id = ?1")?;
        let rows = stmt.query_map(params![identity_id], |row| {
            Ok(IdentityHint {
                identity_id: IdentityId(row.get(0)?),
                kind: row.get::<_, String>(1)?.parse().unwrap(),
                value: row.get(2)?,
                confidence: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().context("Failed to get hints")
    }

    pub fn find_identity_by_hint(&self, kind: HintKind, value: &str) -> Result<Option<Identity>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT identity_id FROM identity_hints WHERE hint_kind = ?1 AND hint_value = ?2")?;
        let mut rows = stmt.query(params![kind.as_str(), value])?;
        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            drop(rows);
            drop(stmt);
            drop(conn);
            self.get_identity(&id)
        } else {
            Ok(None)
        }
    }

    // Tags
    pub fn add_tag(&self, identity_id: &str, namespace: &str, value: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO tags (identity_id, namespace, value) VALUES (?1, ?2, ?3)",
            params![identity_id, namespace, value],
        )?;
        Ok(())
    }

    pub fn remove_tag(&self, identity_id: &str, namespace: &str, value: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM tags WHERE identity_id = ?1 AND namespace = ?2 AND value = ?3",
            params![identity_id, namespace, value],
        )?;
        Ok(())
    }

    pub fn get_tags(&self, identity_id: &str) -> Result<Vec<Tag>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT identity_id, namespace, value FROM tags WHERE identity_id = ?1")?;
        let rows = stmt.query_map(params![identity_id], |row| {
            Ok(Tag {
                identity_id: IdentityId(row.get(0)?),
                namespace: row.get(1)?,
                value: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().context("Failed to get tags")
    }

    pub fn find_identities_by_tag(&self, namespace: &str, value: &str) -> Result<Vec<Identity>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT i.id, i.name, i.created_at, i.data FROM identities i
             JOIN tags t ON i.id = t.identity_id
             WHERE t.namespace = ?1 AND t.value = ?2"
        )?;
        let rows = stmt.query_map(params![namespace, value], |row| {
            Ok(Identity {
                id: IdentityId(row.get(0)?),
                name: row.get(1)?,
                created_at: row.get(2)?,
                data: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().context("Failed to find identities by tag")
    }

    // Changelog
    pub fn log_event(&self, source: &str, operation: &str, target_kind: &str, target_id: &str, details: serde_json::Value) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO changelog (source, operation, target_kind, target_id, details) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![source, operation, target_kind, target_id, details.to_string()],
        )?;
        Ok(())
    }

    // Manual Connections
    pub fn add_connection(
        &self,
        id: &str,
        from_identity: &str,
        from_port: &str,
        to_identity: &str,
        to_port: &str,
        transport_kind: Option<&str>,
        created_by: &str,
    ) -> Result<ManualConnection> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO manual_connections (id, from_identity, from_port, to_identity, to_port, transport_kind, signal_direction, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'forward', ?7)",
            params![id, from_identity, from_port, to_identity, to_port, transport_kind, created_by],
        )?;
        self.get_connection(id)?.ok_or_else(|| anyhow::anyhow!("Failed to retrieve created connection"))
    }

    pub fn get_connection(&self, id: &str) -> Result<Option<ManualConnection>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, from_identity, from_port, to_identity, to_port, transport_kind, signal_direction, created_at, created_by
             FROM manual_connections WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(ManualConnection {
                id: row.get(0)?,
                from_identity: IdentityId(row.get(1)?),
                from_port: row.get(2)?,
                to_identity: IdentityId(row.get(3)?),
                to_port: row.get(4)?,
                transport_kind: row.get(5)?,
                signal_direction: row.get(6)?,
                created_at: row.get(7)?,
                created_by: row.get(8)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_connections(&self, identity_id: Option<&str>) -> Result<Vec<ManualConnection>> {
        let conn = self.conn()?;
        let query = if let Some(id) = identity_id {
            format!(
                "SELECT id, from_identity, from_port, to_identity, to_port, transport_kind, signal_direction, created_at, created_by
                 FROM manual_connections WHERE from_identity = '{}' OR to_identity = '{}'", id, id
            )
        } else {
            "SELECT id, from_identity, from_port, to_identity, to_port, transport_kind, signal_direction, created_at, created_by
             FROM manual_connections".to_string()
        };
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            Ok(ManualConnection {
                id: row.get(0)?,
                from_identity: IdentityId(row.get(1)?),
                from_port: row.get(2)?,
                to_identity: IdentityId(row.get(3)?),
                to_port: row.get(4)?,
                transport_kind: row.get(5)?,
                signal_direction: row.get(6)?,
                created_at: row.get(7)?,
                created_by: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().context("Failed to list connections")
    }

    pub fn remove_connection(&self, id: &str) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM manual_connections WHERE id = ?1", params![id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_identity_crud() {
        let db = Database::in_memory().unwrap();

        // Create
        let identity = db.create_identity("jdxi", "Roland JD-Xi", json!({"model": "JD-Xi"})).unwrap();
        assert_eq!(identity.id.0, "jdxi");
        assert_eq!(identity.name, "Roland JD-Xi");

        // Read
        let found = db.get_identity("jdxi").unwrap().unwrap();
        assert_eq!(found.name, "Roland JD-Xi");

        // List
        let all = db.list_identities().unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        db.delete_identity("jdxi").unwrap();
        assert!(db.get_identity("jdxi").unwrap().is_none());
    }

    #[test]
    fn test_hint_matching() {
        let db = Database::in_memory().unwrap();
        db.create_identity("jdxi", "Roland JD-Xi", json!({})).unwrap();

        db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();
        db.add_hint("jdxi", HintKind::MidiName, "JD-Xi", 0.9).unwrap();

        let found = db.find_identity_by_hint(HintKind::UsbDeviceId, "0582:0160").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id.0, "jdxi");

        let hints = db.get_hints("jdxi").unwrap();
        assert_eq!(hints.len(), 2);
    }

    #[test]
    fn test_tags() {
        let db = Database::in_memory().unwrap();
        db.create_identity("jdxi", "Roland JD-Xi", json!({})).unwrap();

        db.add_tag("jdxi", "manufacturer", "roland").unwrap();
        db.add_tag("jdxi", "role", "sound-source").unwrap();

        let tags = db.get_tags("jdxi").unwrap();
        assert_eq!(tags.len(), 2);

        let roland = db.find_identities_by_tag("manufacturer", "roland").unwrap();
        assert_eq!(roland.len(), 1);
    }

    #[test]
    fn test_cascade_delete() {
        let db = Database::in_memory().unwrap();
        db.create_identity("test", "Test", json!({})).unwrap();
        db.add_hint("test", HintKind::MidiName, "Test", 1.0).unwrap();
        db.add_tag("test", "role", "test").unwrap();

        db.delete_identity("test").unwrap();

        // Hints and tags should be gone
        let hints = db.get_hints("test").unwrap();
        assert!(hints.is_empty());
        let tags = db.get_tags("test").unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn test_connections() {
        let db = Database::in_memory().unwrap();
        db.create_identity("poly2", "Polyend Poly 2", json!({})).unwrap();
        db.create_identity("a110", "Doepfer A-110", json!({})).unwrap();

        let conn = db.add_connection(
            "conn1", "poly2", "cv_out_1", "a110", "voct_in",
            Some("patch_cable_cv"), "test"
        ).unwrap();

        assert_eq!(conn.from_identity.0, "poly2");
        assert_eq!(conn.to_identity.0, "a110");

        let all = db.list_connections(None).unwrap();
        assert_eq!(all.len(), 1);

        let by_identity = db.list_connections(Some("poly2")).unwrap();
        assert_eq!(by_identity.len(), 1);

        db.remove_connection("conn1").unwrap();
        let after = db.list_connections(None).unwrap();
        assert!(after.is_empty());
    }
}
