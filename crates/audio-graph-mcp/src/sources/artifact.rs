//! Artifact data source abstraction for Trustfall queries.
//!
//! Provides traits and types for querying artifacts without depending
//! on a specific storage implementation. This enables swapping local
//! storage for network-based storage while keeping GraphQL queries stable.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Default time window for recent artifacts (10 minutes).
pub const DEFAULT_RECENT_WINDOW: Duration = Duration::from_secs(10 * 60);

/// Trait for artifact data sources.
///
/// Implementations can be local (FileStore) or remote (HTTP client).
/// The adapter uses this trait to resolve Artifact queries.
pub trait ArtifactSource: Send + Sync {
    /// Get a single artifact by ID.
    fn get(&self, id: &str) -> anyhow::Result<Option<ArtifactData>>;

    /// Get all artifacts.
    fn all(&self) -> anyhow::Result<Vec<ArtifactData>>;

    /// Get artifacts matching a tag.
    fn by_tag(&self, tag: &str) -> anyhow::Result<Vec<ArtifactData>>;

    /// Get artifacts by creator.
    fn by_creator(&self, creator: &str) -> anyhow::Result<Vec<ArtifactData>>;

    /// Get artifacts that are children of a parent.
    fn by_parent(&self, parent_id: &str) -> anyhow::Result<Vec<ArtifactData>>;

    /// Get artifacts in a variation set.
    fn by_variation_set(&self, set_id: &str) -> anyhow::Result<Vec<ArtifactData>>;

    /// Get annotations for an artifact.
    fn annotations_for(&self, artifact_id: &str) -> anyhow::Result<Vec<AnnotationData>>;

    /// Add an annotation to an artifact.
    fn add_annotation(&self, annotation: AnnotationData) -> anyhow::Result<()>;

    /// Get artifacts created within the given duration from now.
    /// Returns artifacts sorted by created_at descending (newest first).
    fn recent(&self, within: Duration) -> anyhow::Result<Vec<ArtifactData>>;
}

/// Artifact data transfer object.
///
/// This is the format used by the Trustfall adapter.
/// Storage implementations convert their internal format to this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactData {
    pub id: String,
    pub content_hash: String,
    pub created_at: String,
    pub creator: String,
    pub tags: Vec<String>,
    pub parent_id: Option<String>,
    pub variation_set_id: Option<String>,
    pub variation_index: Option<u32>,
    pub metadata: serde_json::Value,
}

/// Annotation data transfer object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationData {
    pub id: String,
    pub artifact_id: String,
    pub message: String,
    pub vibe: Option<String>,
    pub source: String,
    pub created_at: String,
}

impl AnnotationData {
    /// Create a new annotation with generated ID and timestamp.
    pub fn new(artifact_id: String, message: String, vibe: Option<String>, source: String) -> Self {
        let id = format!(
            "ann_{}",
            &uuid::Uuid::new_v4().to_string().replace("-", "")[..12]
        );
        let created_at = chrono::Utc::now().to_rfc3339();

        Self {
            id,
            artifact_id,
            message,
            vibe,
            source,
            created_at,
        }
    }
}
