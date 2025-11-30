//! Artifact storage with variation tracking
//!
//! Universal artifact system with variation semantics:
//! - Every artifact has a content_hash pointing to CAS content
//! - Every artifact has optional variation_set_id (grouping)
//! - Every artifact has optional parent_id (refinement chains)
//! - Every artifact has tags (arbitrary metadata)
//! - Access tracking for observability

use crate::types::{ArtifactId, ContentHash, VariationSetId};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Universal artifact with variation semantics
#[derive(Clone, Debug, Serialize)]
pub struct Artifact {
    /// Unique identifier
    pub id: ArtifactId,

    /// Reference to content in CAS
    pub content_hash: ContentHash,

    /// Part of a variation set?
    pub variation_set_id: Option<VariationSetId>,

    /// Position in variation set (0, 1, 2, ...)
    pub variation_index: Option<u32>,

    /// Parent artifact (for refinements)
    pub parent_id: Option<ArtifactId>,

    /// Arbitrary tags for organization/filtering
    pub tags: Vec<String>,

    /// When this was created
    pub created_at: DateTime<Utc>,

    /// Who created it (agent_id or user_id)
    pub creator: String,

    /// Type-specific metadata (tool params, etc. - NOT the hash)
    pub metadata: serde_json::Value,

    /// Number of times this artifact has been accessed
    #[serde(default)]
    pub access_count: u64,

    /// Last time this artifact was accessed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_accessed: Option<DateTime<Utc>>,
}

/// Legacy format for backwards compatibility when loading old artifacts.json
#[derive(Deserialize)]
struct LegacyArtifact {
    id: String,
    variation_set_id: Option<String>,
    variation_index: Option<u32>,
    parent_id: Option<String>,
    tags: Vec<String>,
    created_at: DateTime<Utc>,
    creator: String,
    // Old field (optional - may not exist in new format)
    #[serde(default)]
    data: serde_json::Value,
    // New fields might exist
    #[serde(default)]
    content_hash: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
    #[serde(default)]
    access_count: u64,
    #[serde(default)]
    last_accessed: Option<DateTime<Utc>>,
}

impl<'de> Deserialize<'de> for Artifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let legacy = LegacyArtifact::deserialize(deserializer)?;

        // Extract content_hash: prefer explicit field, fall back to data.hash
        let content_hash = if let Some(hash) = legacy.content_hash {
            ContentHash::new(hash)
        } else if let Some(hash) = legacy.data.get("hash").and_then(|v| v.as_str()) {
            ContentHash::new(hash)
        } else {
            // No hash found - this shouldn't happen but create empty placeholder
            ContentHash::new("")
        };

        // Metadata: prefer explicit field, fall back to data (minus hash)
        let metadata = if let Some(m) = legacy.metadata {
            m
        } else {
            // Remove 'hash' from data to create metadata
            let mut data = legacy.data;
            if let Some(obj) = data.as_object_mut() {
                obj.remove("hash");
            }
            data
        };

        Ok(Artifact {
            id: ArtifactId::new(legacy.id),
            content_hash,
            variation_set_id: legacy.variation_set_id.map(VariationSetId::new),
            variation_index: legacy.variation_index,
            parent_id: legacy.parent_id.map(ArtifactId::new),
            tags: legacy.tags,
            created_at: legacy.created_at,
            creator: legacy.creator,
            metadata,
            access_count: legacy.access_count,
            last_accessed: legacy.last_accessed,
        })
    }
}

impl Artifact {
    /// Create a new artifact with required fields
    pub fn new(
        id: ArtifactId,
        content_hash: ContentHash,
        creator: impl Into<String>,
        metadata: serde_json::Value,
    ) -> Self {
        Self {
            id,
            content_hash,
            variation_set_id: None,
            variation_index: None,
            parent_id: None,
            tags: Vec::new(),
            created_at: Utc::now(),
            creator: creator.into(),
            metadata,
            access_count: 0,
            last_accessed: None,
        }
    }

    /// Builder: set variation set
    pub fn with_variation_set(mut self, set_id: VariationSetId, index: u32) -> Self {
        self.variation_set_id = Some(set_id);
        self.variation_index = Some(index);
        self
    }

    /// Builder: set parent
    pub fn with_parent(mut self, parent_id: ArtifactId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Builder: add tag
    #[allow(dead_code)]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Builder: add multiple tags
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags.extend(tags.into_iter().map(|t| t.into()));
        self
    }

    /// Record an access to this artifact
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Some(Utc::now());
    }

    /// Check if artifact has a tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// Check if artifact has any of these tags
    #[allow(dead_code)]
    pub fn has_any_tag(&self, tags: &[&str]) -> bool {
        self.tags.iter().any(|t| tags.contains(&t.as_str()))
    }

    /// Check if artifact has all of these tags
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn role(&self) -> Option<&str> {
        self.tags_with_prefix("role:").first().copied()
    }

    /// Helper: get the type tag (first "type:*" tag)
    #[allow(dead_code)]
    pub fn artifact_type(&self) -> Option<&str> {
        self.tags_with_prefix("type:").first().copied()
    }

    /// Helper: get the phase tag (first "phase:*" tag)
    pub fn phase(&self) -> Option<&str> {
        self.tags_with_prefix("phase:").first().copied()
    }
}

/// Trait for artifact storage backends
#[allow(dead_code)]
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
            .filter(|a| a.variation_set_id.as_ref().map(|s| s.as_str()) == Some(set_id))
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
            .map(|a| (a.id.as_str().to_string(), a))
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
        artifacts.insert(artifact.id.as_str().to_string(), artifact);
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
        let content_hash = ContentHash::new("abc123def456abc123def456abc123de");
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

        let artifact = Artifact::new(
            artifact_id,
            content_hash.clone(),
            "agent_test",
            json!({"foo": "bar"}),
        )
        .with_variation_set(VariationSetId::new("vset_123"), 0)
        .with_parent(ArtifactId::new("parent_001"))
        .with_tag("type:test")
        .with_tag("phase:initial");

        assert_eq!(artifact.id.as_str(), "artifact_abc123def456");
        assert_eq!(artifact.content_hash.as_str(), "abc123def456abc123def456abc123de");
        assert_eq!(
            artifact.variation_set_id.as_ref().map(|s| s.as_str()),
            Some("vset_123")
        );
        assert_eq!(artifact.variation_index, Some(0));
        assert_eq!(
            artifact.parent_id.as_ref().map(|s| s.as_str()),
            Some("parent_001")
        );
        assert!(artifact.has_tag("type:test"));
        assert!(artifact.has_tag("phase:initial"));
    }

    #[test]
    fn test_access_tracking() {
        let content_hash = ContentHash::new("abc123def456abc123def456abc123de");
        let artifact_id = ArtifactId::from_hash_prefix(&content_hash);

        let mut artifact = Artifact::new(artifact_id, content_hash, "agent", json!({}));

        assert_eq!(artifact.access_count, 0);
        assert!(artifact.last_accessed.is_none());

        artifact.record_access();
        assert_eq!(artifact.access_count, 1);
        assert!(artifact.last_accessed.is_some());

        let first_access = artifact.last_accessed;
        artifact.record_access();
        assert_eq!(artifact.access_count, 2);
        assert!(artifact.last_accessed >= first_access);
    }

    #[test]
    fn test_tag_helpers() {
        let content_hash = ContentHash::new("abc123def456abc123def456abc123de");
        let artifact = Artifact::new(
            ArtifactId::new("test"),
            content_hash,
            "agent",
            json!({}),
        )
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

        let content_hash = ContentHash::new("abc123def456abc123def456abc123de");
        let artifact = Artifact::new(
            ArtifactId::new("test_001"),
            content_hash,
            "agent",
            json!({"data": "value"}),
        );

        store.put(artifact).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        assert!(store.exists("test_001").unwrap());

        let retrieved = store.get("test_001").unwrap().unwrap();
        assert_eq!(retrieved.id.as_str(), "test_001");

        let deleted = store.delete("test_001").unwrap();
        assert!(deleted);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_next_variation_index() {
        let store = InMemoryStore::new();

        // First variation
        let a1 = Artifact::new(
            ArtifactId::new("a1"),
            ContentHash::new("hash1hash1hash1hash1hash1hash1ha"),
            "agent",
            json!({}),
        )
        .with_variation_set(VariationSetId::new("vset_123"), 0);
        store.put(a1).unwrap();
        assert_eq!(store.next_variation_index("vset_123").unwrap(), 1);

        // Second variation
        let a2 = Artifact::new(
            ArtifactId::new("a2"),
            ContentHash::new("hash2hash2hash2hash2hash2hash2ha"),
            "agent",
            json!({}),
        )
        .with_variation_set(VariationSetId::new("vset_123"), 1);
        store.put(a2).unwrap();
        assert_eq!(store.next_variation_index("vset_123").unwrap(), 2);

        // Different set
        assert_eq!(store.next_variation_index("vset_456").unwrap(), 1);
    }

    #[test]
    fn test_legacy_deserialization() {
        // Simulate old format with hash in data
        let legacy_json = r#"{
            "id": "test_legacy",
            "variation_set_id": null,
            "variation_index": null,
            "parent_id": null,
            "tags": ["type:midi"],
            "created_at": "2024-01-01T00:00:00Z",
            "creator": "agent",
            "data": {"hash": "legacyhashlegacyhashlegacyhashle", "other": "value"}
        }"#;

        let artifact: Artifact = serde_json::from_str(legacy_json).unwrap();

        assert_eq!(artifact.id.as_str(), "test_legacy");
        assert_eq!(
            artifact.content_hash.as_str(),
            "legacyhashlegacyhashlegacyhashle"
        );
        // Hash should be removed from metadata
        assert!(artifact.metadata.get("hash").is_none());
        assert_eq!(artifact.metadata.get("other").unwrap(), "value");
        assert_eq!(artifact.access_count, 0);
        assert!(artifact.last_accessed.is_none());
    }

    #[test]
    fn test_new_format_deserialization() {
        // New format with explicit content_hash
        let new_json = r#"{
            "id": "test_new",
            "content_hash": "newhashnewhashnewhashnewhashneha",
            "variation_set_id": "vset_001",
            "variation_index": 2,
            "parent_id": "parent_001",
            "tags": ["type:audio"],
            "created_at": "2024-01-01T00:00:00Z",
            "creator": "agent",
            "data": {},
            "metadata": {"sample_rate": 44100},
            "access_count": 5,
            "last_accessed": "2024-06-01T12:00:00Z"
        }"#;

        let artifact: Artifact = serde_json::from_str(new_json).unwrap();

        assert_eq!(artifact.id.as_str(), "test_new");
        assert_eq!(artifact.content_hash.as_str(), "newhashnewhashnewhashnewhashneha");
        assert_eq!(
            artifact.variation_set_id.as_ref().map(|s| s.as_str()),
            Some("vset_001")
        );
        assert_eq!(artifact.variation_index, Some(2));
        assert_eq!(artifact.metadata.get("sample_rate").unwrap(), 44100);
        assert_eq!(artifact.access_count, 5);
        assert!(artifact.last_accessed.is_some());
    }

    #[test]
    fn test_file_store() {
        let temp_dir = std::env::temp_dir().join("hrmcp_test_artifacts");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let path = temp_dir.join("test_artifacts_v2.json");

        // Create and save
        {
            let store = FileStore::new(&path).unwrap();
            let content_hash = ContentHash::new("filehashfilehashfilehashfilehash");
            let artifact = Artifact::new(
                ArtifactId::new("test_001"),
                content_hash,
                "agent",
                json!({"data": "value"}),
            );
            store.put(artifact).unwrap();
            store.flush().unwrap();
        }

        // Load and verify
        {
            let store = FileStore::new(&path).unwrap();
            assert_eq!(store.count().unwrap(), 1);
            let artifact = store.get("test_001").unwrap().unwrap();
            assert_eq!(artifact.id.as_str(), "test_001");
            assert_eq!(artifact.content_hash.as_str(), "filehashfilehashfilehashfilehash");
        }

        // Cleanup
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_serde_roundtrip() {
        let content_hash = ContentHash::new("roundtriproundtriproundtriproun");
        let original = Artifact::new(
            ArtifactId::new("roundtrip_test"),
            content_hash,
            "agent",
            json!({"key": "value"}),
        )
        .with_variation_set(VariationSetId::new("vset"), 1)
        .with_parent(ArtifactId::new("parent"))
        .with_tags(vec!["type:test"]);

        let json = serde_json::to_string(&original).unwrap();
        let restored: Artifact = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id.as_str(), restored.id.as_str());
        assert_eq!(original.content_hash.as_str(), restored.content_hash.as_str());
        assert_eq!(
            original.variation_set_id.as_ref().map(|s| s.as_str()),
            restored.variation_set_id.as_ref().map(|s| s.as_str())
        );
        assert_eq!(original.variation_index, restored.variation_index);
        assert_eq!(
            original.parent_id.as_ref().map(|s| s.as_str()),
            restored.parent_id.as_ref().map(|s| s.as_str())
        );
        assert_eq!(original.tags, restored.tags);
        assert_eq!(original.creator, restored.creator);
    }

    #[test]
    fn test_variation_set_tracking() {
        let store = InMemoryStore::new();

        // Create variation set with 3 artifacts
        for i in 0..3 {
            let content_hash = ContentHash::new(format!(
                "varhash{}varhash{}varhash{}varhash{}",
                i, i, i, i
            ));
            let artifact = Artifact::new(
                ArtifactId::new(format!("var_{}", i)),
                content_hash,
                "agent_claude",
                json!({"variation": i}),
            )
            .with_variation_set(VariationSetId::new("vset_exploration"), i)
            .with_tags(vec!["phase:exploration", "type:midi"]);

            store.put(artifact).unwrap();
        }

        // Verify all in same set
        let all = store.all().unwrap();
        let in_set: Vec<_> = all
            .iter()
            .filter(|a| a.variation_set_id.as_ref().map(|s| s.as_str()) == Some("vset_exploration"))
            .collect();
        assert_eq!(in_set.len(), 3);

        // Next index should be 3
        assert_eq!(store.next_variation_index("vset_exploration").unwrap(), 3);
    }
}
