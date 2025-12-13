//! Region types for timeline scheduling
//!
//! Regions represent spans of musical time on the timeline with behaviors
//! like playing content, generating latent content, or emitting triggers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::primitives::{Beat, Generation, Lifecycle};

// =============================================================================
// REGION TYPES
// =============================================================================

/// Content type for artifacts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    Audio,
    Midi,
}

/// Playback parameters for a region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackParams {
    pub gain: f64,
    pub rate: f64,
    pub offset: Beat,
    pub reverse: bool,
    pub fade_in: Beat,
    pub fade_out: Beat,
}

impl Default for PlaybackParams {
    fn default() -> Self {
        Self {
            gain: 1.0,
            rate: 1.0,
            offset: Beat::zero(),
            reverse: false,
            fade_in: Beat::zero(),
            fade_out: Beat::zero(),
        }
    }
}

/// Curve interpolation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CurveType {
    #[default]
    Linear,
    Exponential,
    Logarithmic,
    SCurve,
    Hold,
}

/// A point on an automation curve
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurvePoint {
    pub position: f64,
    pub value: f64,
    pub curve: CurveType,
}

/// Status of a latent region's generation job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LatentStatus {
    #[default]
    Pending,
    Running,
    Resolved,
    Approved,
    Rejected,
    Failed,
}

/// Resolved content reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedContent {
    pub content_hash: String,
    pub content_type: ContentType,
    pub artifact_id: String,
}

/// State of a latent region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatentState {
    pub job_id: Option<String>,
    pub progress: f32,
    pub status: LatentStatus,
    pub resolved: Option<ResolvedContent>,
}

impl Default for LatentState {
    fn default() -> Self {
        Self {
            job_id: None,
            progress: 0.0,
            status: LatentStatus::Pending,
            resolved: None,
        }
    }
}

/// Types of trigger events
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerKind {
    SectionStart,
    SectionEnd,
    BarStart,
    BeatStart,
    Cue(String),
    Custom(String),
}

/// Behavior of a region on the timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Behavior {
    PlayContent {
        content_hash: String,
        content_type: ContentType,
        params: PlaybackParams,
    },
    Latent {
        tool: String,
        params: serde_json::Value,
        state: LatentState,
    },
    ApplyProcessing {
        target_node: Uuid,
        parameter: String,
        curve: Vec<CurvePoint>,
    },
    EmitTrigger {
        kind: TriggerKind,
        data: Option<serde_json::Value>,
    },
    Custom {
        behavior_type: String,
        config: serde_json::Value,
    },
}

/// Metadata for a region
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegionMetadata {
    pub name: Option<String>,
    pub color: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// A region on the timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub id: Uuid,
    pub position: Beat,
    pub duration: Beat,
    pub behavior: Behavior,
    pub metadata: RegionMetadata,
    pub lifecycle: Lifecycle,
}

impl Region {
    // Constructors

    pub fn play_audio(position: Beat, duration: Beat, content_hash: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            position,
            duration,
            behavior: Behavior::PlayContent {
                content_hash,
                content_type: ContentType::Audio,
                params: PlaybackParams::default(),
            },
            metadata: RegionMetadata::default(),
            lifecycle: Lifecycle::default(),
        }
    }

    pub fn play_midi(position: Beat, duration: Beat, content_hash: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            position,
            duration,
            behavior: Behavior::PlayContent {
                content_hash,
                content_type: ContentType::Midi,
                params: PlaybackParams::default(),
            },
            metadata: RegionMetadata::default(),
            lifecycle: Lifecycle::default(),
        }
    }

    pub fn latent(position: Beat, duration: Beat, tool: &str, params: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            position,
            duration,
            behavior: Behavior::Latent {
                tool: tool.to_string(),
                params,
                state: LatentState::default(),
            },
            metadata: RegionMetadata::default(),
            lifecycle: Lifecycle::default(),
        }
    }

    pub fn trigger(position: Beat, kind: TriggerKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            position,
            duration: Beat::zero(),
            behavior: Behavior::EmitTrigger { kind, data: None },
            metadata: RegionMetadata::default(),
            lifecycle: Lifecycle::default(),
        }
    }

    // Queries

    pub fn end(&self) -> Beat {
        self.position + self.duration
    }

    pub fn contains(&self, beat: Beat) -> bool {
        beat.0 >= self.position.0 && beat.0 < self.end().0
    }

    pub fn overlaps(&self, other: &Region) -> bool {
        self.position.0 < other.end().0 && self.end().0 > other.position.0
    }

    // Latent state queries

    pub fn is_latent(&self) -> bool {
        matches!(self.behavior, Behavior::Latent { .. })
    }

    pub fn is_resolved(&self) -> bool {
        match &self.behavior {
            Behavior::Latent { state, .. } => state.status == LatentStatus::Resolved,
            _ => false,
        }
    }

    pub fn is_approved(&self) -> bool {
        match &self.behavior {
            Behavior::Latent { state, .. } => state.status == LatentStatus::Approved,
            _ => false,
        }
    }

    pub fn is_playable(&self) -> bool {
        match &self.behavior {
            Behavior::PlayContent { .. } => true,
            Behavior::Latent { state, .. } => state.status == LatentStatus::Approved,
            _ => false,
        }
    }

    pub fn latent_status(&self) -> Option<LatentStatus> {
        match &self.behavior {
            Behavior::Latent { state, .. } => Some(state.status),
            _ => None,
        }
    }

    // Latent state transitions

    pub fn start_job(&mut self, job_id: String) {
        if let Behavior::Latent { state, .. } = &mut self.behavior {
            state.job_id = Some(job_id);
            state.status = LatentStatus::Running;
            state.progress = 0.0;
        }
    }

    pub fn update_progress(&mut self, progress: f32) {
        if let Behavior::Latent { state, .. } = &mut self.behavior {
            state.progress = progress.clamp(0.0, 1.0);
        }
    }

    pub fn resolve(&mut self, content: ResolvedContent) {
        if let Behavior::Latent { state, .. } = &mut self.behavior {
            state.resolved = Some(content);
            state.status = LatentStatus::Resolved;
            state.progress = 1.0;
        }
    }

    pub fn approve(&mut self) {
        if let Behavior::Latent { state, .. } = &mut self.behavior {
            if state.status == LatentStatus::Resolved {
                state.status = LatentStatus::Approved;
            }
        }
    }

    pub fn reject(&mut self) {
        if let Behavior::Latent { state, .. } = &mut self.behavior {
            state.status = LatentStatus::Rejected;
        }
    }

    pub fn fail(&mut self) {
        if let Behavior::Latent { state, .. } = &mut self.behavior {
            state.status = LatentStatus::Failed;
        }
    }

    // Builders

    pub fn with_name(mut self, name: &str) -> Self {
        self.metadata.name = Some(name.to_string());
        self
    }

    pub fn with_tag(mut self, tag: &str) -> Self {
        self.metadata.tags.push(tag.to_string());
        self
    }

    // Lifecycle delegates

    pub fn touch(&mut self, generation: Generation) {
        self.lifecycle.touch(generation);
    }

    pub fn tombstone(&mut self, generation: Generation) {
        self.lifecycle.tombstone(generation);
    }

    pub fn set_permanent(&mut self, permanent: bool) {
        self.lifecycle.set_permanent(permanent);
    }

    pub fn is_tombstoned(&self) -> bool {
        self.lifecycle.is_tombstoned()
    }

    pub fn is_alive(&self) -> bool {
        self.lifecycle.is_alive()
    }
}

// Implement Serialize/Deserialize for TriggerKind
impl Serialize for TriggerKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            TriggerKind::SectionStart => serializer.serialize_str("section_start"),
            TriggerKind::SectionEnd => serializer.serialize_str("section_end"),
            TriggerKind::BarStart => serializer.serialize_str("bar_start"),
            TriggerKind::BeatStart => serializer.serialize_str("beat_start"),
            TriggerKind::Cue(s) => serializer.serialize_str(&format!("cue:{}", s)),
            TriggerKind::Custom(s) => serializer.serialize_str(&format!("custom:{}", s)),
        }
    }
}

impl<'de> Deserialize<'de> for TriggerKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "section_start" => Ok(TriggerKind::SectionStart),
            "section_end" => Ok(TriggerKind::SectionEnd),
            "bar_start" => Ok(TriggerKind::BarStart),
            "beat_start" => Ok(TriggerKind::BeatStart),
            _ if s.starts_with("cue:") => Ok(TriggerKind::Cue(s[4..].to_string())),
            _ if s.starts_with("custom:") => Ok(TriggerKind::Custom(s[7..].to_string())),
            _ => Err(serde::de::Error::custom(format!(
                "unknown trigger kind: {}",
                s
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_contains() {
        let region = Region::play_audio(Beat(4.0), Beat(8.0), "hash".to_string());
        assert!(!region.contains(Beat(3.0)));
        assert!(region.contains(Beat(4.0)));
        assert!(region.contains(Beat(8.0)));
        assert!(!region.contains(Beat(12.0)));
    }

    #[test]
    fn test_region_overlaps() {
        let r1 = Region::play_audio(Beat(0.0), Beat(4.0), "hash1".to_string());
        let r2 = Region::play_audio(Beat(2.0), Beat(4.0), "hash2".to_string());
        let r3 = Region::play_audio(Beat(4.0), Beat(4.0), "hash3".to_string());

        assert!(r1.overlaps(&r2));
        assert!(!r1.overlaps(&r3));
    }

    #[test]
    fn test_latent_state_transitions() {
        let mut region = Region::latent(
            Beat(0.0),
            Beat(4.0),
            "orpheus_generate",
            serde_json::json!({}),
        );

        assert!(region.is_latent());
        assert!(!region.is_playable());
        assert_eq!(region.latent_status(), Some(LatentStatus::Pending));

        region.start_job("job_123".to_string());
        assert_eq!(region.latent_status(), Some(LatentStatus::Running));

        region.update_progress(0.5);
        if let Behavior::Latent { state, .. } = &region.behavior {
            assert_eq!(state.progress, 0.5);
        }

        region.resolve(ResolvedContent {
            content_hash: "abc123".to_string(),
            content_type: ContentType::Midi,
            artifact_id: "artifact_456".to_string(),
        });
        assert!(region.is_resolved());
        assert!(!region.is_playable());

        region.approve();
        assert!(region.is_approved());
        assert!(region.is_playable());
    }

    #[test]
    fn test_region_serialization() {
        let region = Region::play_audio(Beat(4.0), Beat(8.0), "hash123".to_string())
            .with_name("intro")
            .with_tag("jazzy");

        let json = serde_json::to_string(&region).unwrap();
        let decoded: Region = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.position.0, 4.0);
        assert_eq!(decoded.duration.0, 8.0);
        assert_eq!(decoded.metadata.name, Some("intro".to_string()));
        assert!(decoded.metadata.tags.contains(&"jazzy".to_string()));
    }

    #[test]
    fn test_lifecycle() {
        let mut lifecycle = Lifecycle::new(1);
        assert!(lifecycle.is_alive());
        assert!(!lifecycle.is_tombstoned());

        lifecycle.tombstone(2);
        assert!(lifecycle.is_tombstoned());
        assert!(!lifecycle.is_alive());

        lifecycle.touch(3);
        assert!(lifecycle.is_alive());

        lifecycle.set_permanent(true);
        lifecycle.tombstone(4);
        assert!(lifecycle.is_alive());
    }
}
