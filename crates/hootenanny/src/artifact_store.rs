//! Artifact storage with variation tracking
//!
//! Universal artifact system with variation semantics:
//! - Every artifact has optional variation_set_id (grouping)
//! - Every artifact has optional parent_id (refinement chains)
//! - Every artifact has tags (arbitrary metadata)
//! - Query logic deferred to Lua (later)

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Universal artifact with variation semantics
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    /// Unique identifier
    pub id: String,

    /// Part of a variation set?
    pub variation_set_id: Option<String>,

    /// Position in variation set (0, 1, 2, ...)
    pub variation_index: Option<u32>,

    /// Parent artifact (for refinements)
    pub parent_id: Option<String>,

    /// Arbitrary tags for organization/filtering
    pub tags: Vec<String>,

    /// When this was created
    pub created_at: DateTime<Utc>,

    /// Who created it (agent_id or user_id)
    pub creator: String,

    /// Type-specific data (MIDI metadata, contribution text, etc.)
    pub data: serde_json::Value,
}

impl Artifact {
    /// Create a new artifact with minimal fields
    pub fn new(id: impl Into<String>, creator: impl Into<String>, data: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            variation_set_id: None,
            variation_index: None,
            parent_id: None,
            tags: Vec::new(),
            created_at: Utc::now(),
            creator: creator.into(),
            data,
        }
    }

    /// Builder: set variation set
    pub fn with_variation_set(mut self, set_id: impl Into<String>, index: u32) -> Self {
        self.variation_set_id = Some(set_id.into());
        self.variation_index = Some(index);
        self
    }

    /// Builder: set parent
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Builder: add tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Builder: add multiple tags
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags.extend(tags.into_iter().map(|t| t.into()));
        self
    }

    /// Check if artifact has a tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// Check if artifact has any of these tags
    pub fn has_any_tag(&self, tags: &[&str]) -> bool {
        self.tags.iter().any(|t| tags.contains(&t.as_str()))
    }

    /// Check if artifact has all of these tags
    pub fn has_all_tags(&self, tags: &[&str]) -> bool {
        tags.iter().all(|tag| self.has_tag(tag))
    }

    /// Get tags with a specific prefix (e.g., "role:")
    pub fn tags_with_prefix(&self, prefix: &str) -> Vec<&str> {
        self.tags
            .iter()
            .filter(|t| t.starts_with(prefix))
            .map(|t| t.as_str())
            .collect()
    }

    /// Helper: get the role tag (first "role:*" tag)
    pub fn role(&self) -> Option<&str> {
        self.tags_with_prefix("role:").first().copied()
    }

    /// Helper: get the type tag (first "type:*" tag)
    pub fn artifact_type(&self) -> Option<&str> {
        self.tags_with_prefix("type:").first().copied()
    }

    /// Helper: get the phase tag (first "phase:*" tag)
    pub fn phase(&self) -> Option<&str> {
        self.tags_with_prefix("phase:").first().copied()
    }
}

/// Trait for artifact storage backends
pub trait ArtifactStore: Send + Sync {
    /// Get artifact by ID
    fn get(&self, id: &str) -> Result<Option<Artifact>>;

    /// Store an artifact (insert or update)
    fn put(&self, artifact: Artifact) -> Result<()>;

    /// Delete an artifact by ID
    fn delete(&self, id: &str) -> Result<bool>;

    /// Get all artifacts (for iteration/filtering in Lua)
    fn all(&self) -> Result<Vec<Artifact>>;

    /// Get count of artifacts
    fn count(&self) -> Result<usize> {
        Ok(self.all()?.len())
    }

    /// Check if artifact exists
    fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.get(id)?.is_some())
    }

    /// Persist to storage (if applicable)
    fn flush(&self) -> Result<()> {
        Ok(()) // No-op for in-memory stores
    }

    /// Get next variation index for a set (helper)
    fn next_variation_index(&self, set_id: &str) -> Result<u32> {
        let max_index = self
            .all()?
            .iter()
            .filter(|a| a.variation_set_id.as_deref() == Some(set_id))
            .filter_map(|a| a.variation_index)
            .max()
            .unwrap_or(0);
        Ok(max_index + 1)
    }
}

/// In-memory artifact store (HashMap-backed)
#[derive(Debug)]
pub struct InMemoryStore {
    artifacts: RwLock<HashMap<String, Artifact>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            artifacts: RwLock::new(HashMap::new()),
        }
    }

    pub fn from_artifacts(artifacts: Vec<Artifact>) -> Self {
        let map = artifacts
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();
        Self {
            artifacts: RwLock::new(map),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactStore for InMemoryStore {
    fn get(&self, id: &str) -> Result<Option<Artifact>> {
        let artifacts = self.artifacts.read().unwrap();
        Ok(artifacts.get(id).cloned())
    }

    fn put(&self, artifact: Artifact) -> Result<()> {
        let mut artifacts = self.artifacts.write().unwrap();
        artifacts.insert(artifact.id.clone(), artifact);
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<bool> {
        let mut artifacts = self.artifacts.write().unwrap();
        Ok(artifacts.remove(id).is_some())
    }

    fn all(&self) -> Result<Vec<Artifact>> {
        let artifacts = self.artifacts.read().unwrap();
        Ok(artifacts.values().cloned().collect())
    }

    fn count(&self) -> Result<usize> {
        let artifacts = self.artifacts.read().unwrap();
        Ok(artifacts.len())
    }

    fn exists(&self, id: &str) -> Result<bool> {
        let artifacts = self.artifacts.read().unwrap();
        Ok(artifacts.contains_key(id))
    }
}

/// File-backed artifact store (JSON + InMemoryStore)
pub struct FileStore {
    path: PathBuf,
    store: InMemoryStore,
}

impl FileStore {
    /// Create/load from file
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let artifacts = if path.exists() {
            let json = std::fs::read_to_string(&path)?;
            serde_json::from_str::<Vec<Artifact>>(&json)?
        } else {
            Vec::new()
        };

        Ok(Self {
            path,
            store: InMemoryStore::from_artifacts(artifacts),
        })
    }

    /// Save to disk
    pub fn save(&self) -> Result<()> {
        let artifacts = self.store.all()?;
        let json = serde_json::to_string_pretty(&artifacts)?;

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Atomic write: write to temp, then rename
        let temp_path = self.path.with_extension("tmp");
        std::fs::write(&temp_path, json)?;
        std::fs::rename(&temp_path, &self.path)?;

        Ok(())
    }
}

impl ArtifactStore for FileStore {
    fn get(&self, id: &str) -> Result<Option<Artifact>> {
        self.store.get(id)
    }

    fn put(&self, artifact: Artifact) -> Result<()> {
        self.store.put(artifact)
    }

    fn delete(&self, id: &str) -> Result<bool> {
        self.store.delete(id)
    }

    fn all(&self) -> Result<Vec<Artifact>> {
        self.store.all()
    }

    fn count(&self) -> Result<usize> {
        self.store.count()
    }

    fn exists(&self, id: &str) -> Result<bool> {
        self.store.exists(id)
    }

    fn flush(&self) -> Result<()> {
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_artifact_builder() {
        let artifact = Artifact::new("test_001", "agent_test", json!({"foo": "bar"}))
            .with_variation_set("vset_123", 0)
            .with_parent("parent_001")
            .with_tag("type:test")
            .with_tag("phase:initial");

        assert_eq!(artifact.id, "test_001");
        assert_eq!(artifact.variation_set_id, Some("vset_123".to_string()));
        assert_eq!(artifact.variation_index, Some(0));
        assert_eq!(artifact.parent_id, Some("parent_001".to_string()));
        assert!(artifact.has_tag("type:test"));
        assert!(artifact.has_tag("phase:initial"));
    }

    #[test]
    fn test_tag_helpers() {
        let artifact = Artifact::new("test", "agent", json!({}))
            .with_tags(vec!["type:midi", "role:melody_specialist", "phase:initial"]);

        assert!(artifact.has_tag("type:midi"));
        assert!(artifact.has_any_tag(&["type:midi", "type:audio"]));
        assert!(artifact.has_all_tags(&["type:midi", "phase:initial"]));
        assert_eq!(artifact.artifact_type(), Some("type:midi"));
        assert_eq!(artifact.role(), Some("role:melody_specialist"));
        assert_eq!(artifact.phase(), Some("phase:initial"));
    }

    #[test]
    fn test_in_memory_store() {
        let store = InMemoryStore::new();

        let artifact = Artifact::new("test_001", "agent", json!({"data": "value"}));

        store.put(artifact.clone()).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        assert!(store.exists("test_001").unwrap());

        let retrieved = store.get("test_001").unwrap().unwrap();
        assert_eq!(retrieved.id, "test_001");

        let deleted = store.delete("test_001").unwrap();
        assert!(deleted);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_next_variation_index() {
        let store = InMemoryStore::new();

        // First variation
        let a1 = Artifact::new("a1", "agent", json!({}))
            .with_variation_set("vset_123", 0);
        store.put(a1).unwrap();
        assert_eq!(store.next_variation_index("vset_123").unwrap(), 1);

        // Second variation
        let a2 = Artifact::new("a2", "agent", json!({}))
            .with_variation_set("vset_123", 1);
        store.put(a2).unwrap();
        assert_eq!(store.next_variation_index("vset_123").unwrap(), 2);

        // Different set
        assert_eq!(store.next_variation_index("vset_456").unwrap(), 1);
    }

    #[test]
    fn test_file_store() {
        let temp_dir = std::env::temp_dir().join("hrmcp_test_artifacts");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let path = temp_dir.join("test_artifacts.json");

        // Create and save
        {
            let store = FileStore::new(&path).unwrap();
            let artifact = Artifact::new("test_001", "agent", json!({"data": "value"}));
            store.put(artifact).unwrap();
            store.flush().unwrap();
        }

        // Load and verify
        {
            let store = FileStore::new(&path).unwrap();
            assert_eq!(store.count().unwrap(), 1);
            let artifact = store.get("test_001").unwrap().unwrap();
            assert_eq!(artifact.id, "test_001");
        }

        // Cleanup
        std::fs::remove_file(&path).ok();
    }
}
