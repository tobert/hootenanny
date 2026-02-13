use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::types::MusicUnderstanding;

/// SQLite-backed cache for music understanding results.
///
/// Cache key is `(content_hash, version)`. When algorithm version bumps,
/// stale entries are recomputed on next access.
///
/// Thread-safe via Mutex — SQLite operations are fast enough that
/// contention is not a concern for this use case.
pub struct AnalysisCache {
    connection: Mutex<Connection>,
}

impl AnalysisCache {
    pub fn open(db_path: &Path) -> Result<Self> {
        let connection =
            Connection::open(db_path).context("opening music understanding cache db")?;

        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS understanding (
                    content_hash TEXT NOT NULL,
                    version      INTEGER NOT NULL,
                    created_at   TEXT NOT NULL,
                    result_json  TEXT NOT NULL,
                    PRIMARY KEY (content_hash, version)
                );
                CREATE TABLE IF NOT EXISTS embeddings (
                    content_hash  TEXT NOT NULL,
                    model_name    TEXT NOT NULL,
                    embedding     BLOB NOT NULL,
                    embed_dim     INTEGER NOT NULL,
                    PRIMARY KEY (content_hash, model_name)
                );",
            )
            .context("creating cache tables")?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// Look up a cached understanding result for the given hash and version.
    pub fn get(&self, content_hash: &str, version: u32) -> Result<Option<MusicUnderstanding>> {
        let conn = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("cache mutex poisoned"))?;

        let mut stmt = conn.prepare_cached(
            "SELECT result_json FROM understanding WHERE content_hash = ?1 AND version = ?2",
        )?;

        let result = stmt.query_row(rusqlite::params![content_hash, version], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        });

        match result {
            Ok(json) => {
                let understanding: MusicUnderstanding = serde_json::from_str(&json)
                    .context("deserializing cached understanding")?;
                Ok(Some(understanding))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e).context("querying understanding cache"),
        }
    }

    /// Store a computed understanding result in the cache.
    pub fn put(&self, understanding: &MusicUnderstanding) -> Result<()> {
        let json =
            serde_json::to_string(understanding).context("serializing understanding for cache")?;
        let now = chrono::Utc::now().to_rfc3339();

        let conn = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("cache mutex poisoned"))?;

        conn.execute(
            "INSERT OR REPLACE INTO understanding (content_hash, version, created_at, result_json)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![understanding.content_hash, understanding.version, now, json],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use midi_analysis::{MidiFileContext, VoiceRole};
    use tempfile::TempDir;

    fn sample_understanding() -> MusicUnderstanding {
        MusicUnderstanding {
            content_hash: "abc123".into(),
            version: 1,
            context: MidiFileContext {
                ppq: 480,
                format: 1,
                track_count: 1,
                tempo_changes: vec![],
                time_signatures: vec![],
                total_ticks: 1920,
            },
            key: KeyDetection {
                root: "C".into(),
                root_pitch_class: 0,
                mode: KeyMode::Major,
                confidence: 0.9,
            },
            meter: MeterDetection {
                numerator: 4,
                denominator: 4,
                confidence: 0.8,
                triplet_feel: 0.0,
            },
            voices: vec![ClassifiedVoice {
                voice_index: 0,
                role: VoiceRole::Melody,
                confidence: 0.85,
                notes: vec![],
                features: Default::default(),
            }],
            chords: vec![ChordEvent {
                beat: 0.0,
                symbol: "C".into(),
                root_pitch_class: 0,
                quality: ChordQuality::Major,
                confidence: 0.9,
            }],
        }
    }

    #[test]
    fn cache_miss_returns_none() {
        let dir = TempDir::new().unwrap();
        let cache = AnalysisCache::open(&dir.path().join("test.db")).unwrap();
        assert!(cache.get("nonexistent", 1).unwrap().is_none());
    }

    #[test]
    fn cache_roundtrip() {
        let dir = TempDir::new().unwrap();
        let cache = AnalysisCache::open(&dir.path().join("test.db")).unwrap();

        let understanding = sample_understanding();
        cache.put(&understanding).unwrap();

        let retrieved = cache.get("abc123", 1).unwrap().unwrap();
        assert_eq!(retrieved.content_hash, "abc123");
        assert_eq!(retrieved.key.root, "C");
        assert_eq!(retrieved.chords.len(), 1);
    }

    #[test]
    fn version_mismatch_is_cache_miss() {
        let dir = TempDir::new().unwrap();
        let cache = AnalysisCache::open(&dir.path().join("test.db")).unwrap();

        let understanding = sample_understanding();
        cache.put(&understanding).unwrap();

        // Same hash, different version = miss
        assert!(cache.get("abc123", 2).unwrap().is_none());
    }
}
