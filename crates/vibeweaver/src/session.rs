//! Session types and IDs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique session identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique rule identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleId(pub String);

impl RuleId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RuleId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique marker identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarkerId(pub String);

impl MarkerId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MarkerId {
    fn default() -> Self {
        Self::new()
    }
}

/// A vibeweaver session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub vibe: Option<String>,
    pub tempo_bpm: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Session {
    pub fn new(name: impl Into<String>, vibe: Option<String>, tempo_bpm: f64) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::new(),
            name: name.into(),
            vibe,
            tempo_bpm,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Trigger conditions for rules
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Trigger {
    /// Fire every N beats
    Beat { divisor: u32 },
    /// Fire when named marker is reached
    Marker { name: String },
    /// Must complete by this beat
    Deadline { beat: f64 },
    /// Fire when artifact with optional tag is created
    Artifact { tag: Option<String> },
    /// Fire when specific job completes
    JobComplete { job_id: String },
    /// Fire on transport state change
    Transport { state: String },
}

impl Trigger {
    pub fn trigger_type(&self) -> &'static str {
        match self {
            Trigger::Beat { .. } => "beat",
            Trigger::Marker { .. } => "marker",
            Trigger::Deadline { .. } => "deadline",
            Trigger::Artifact { .. } => "artifact",
            Trigger::JobComplete { .. } => "job_complete",
            Trigger::Transport { .. } => "transport",
        }
    }
}

/// Actions to execute when trigger fires
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    Sample {
        space: String,
        prompt: Option<String>,
        inference: serde_json::Value,
    },
    Schedule {
        content_hash: String,
        at: f64,
        duration: Option<f64>,
        gain: f64,
    },
    SampleAndSchedule {
        space: String,
        prompt: Option<String>,
        at: f64,
    },
    Play,
    Pause,
    Stop,
    Seek {
        beat: f64,
    },
    Audition {
        content_hash: String,
        duration: f64,
    },
    Notify {
        message: String,
    },
}

/// Rule priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Priority {
    Critical = 0,
    High = 1,
    #[default]
    Normal = 2,
    Low = 3,
    Idle = 4,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Critical => "critical",
            Priority::High => "high",
            Priority::Normal => "normal",
            Priority::Low => "low",
            Priority::Idle => "idle",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "critical" => Some(Priority::Critical),
            "high" => Some(Priority::High),
            "normal" => Some(Priority::Normal),
            "low" => Some(Priority::Low),
            "idle" => Some(Priority::Idle),
            _ => None,
        }
    }

    /// Safety margin multiplier for deadline scheduling
    pub fn safety_margin(&self) -> f64 {
        match self {
            Priority::Critical => 1.5,
            Priority::High => 1.2,
            _ => 1.0,
        }
    }
}

/// A scheduled rule (trigger-action pair)
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Rule {
    pub fn new(session_id: SessionId, trigger: Trigger, action: Action) -> Self {
        Self {
            id: RuleId::new(),
            session_id,
            trigger,
            action,
            priority: Priority::default(),
            enabled: true,
            one_shot: false,
            fired_count: 0,
            last_fired_at: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn one_shot(mut self) -> Self {
        self.one_shot = true;
        self
    }
}

/// Timeline marker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub id: MarkerId,
    pub session_id: SessionId,
    pub beat: f64,
    pub name: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl Marker {
    pub fn new(session_id: SessionId, name: impl Into<String>, beat: f64) -> Self {
        Self {
            id: MarkerId::new(),
            session_id,
            beat,
            name: name.into(),
            metadata: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// History entry for context restoration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub session_id: SessionId,
    pub action: String,
    pub params: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub success: bool,
    pub created_at: DateTime<Utc>,
}
