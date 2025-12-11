# 08: Capabilities

**File:** `src/capabilities.rs`
**Focus:** Participant capability declaration, registration, and querying
**Dependencies:** `uuid`, `chrono`, `serde`, `serde_json`

---

## Task

Create `crates/chaosgarden/src/capabilities.rs` implementing a capability system where participants (humans, models, nodes, devices, agents) declare what they can do using URI-namespaced capabilities.

**Why this matters:** Chaosgarden is a performance space where diverse participants collaborate as equals. For participants to compose workflows and coordinate, they must discover each other's capabilities. This module provides that discovery.

**Design decisions:**
- Participants declare capabilities (no negotiation between participants)
- URI namespacing allows extension without central coordination
- Flat URIs with inferred relationships (not hierarchical)
- Pull-based discovery (participants poll the registry)

**Deliverables:**
1. `capabilities.rs` with types and registry
2. Integration with Trustfall query layer (types exposed via 06-query)
3. Tests covering registration, update, and query scenarios

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ Capability negotiation — participants declare, synthesis aligns
- ❌ GraphQL resolvers — that's task 06-query
- ❌ MCP tool exposure — that's hootenanny integration
- ❌ Persistence — registry is in-memory for now

Focus ONLY on the capability type system and registry.

---

## Integration with Other Modules

This module is the central registry other modules depend on:

| Module | Integration |
|--------|-------------|
| **01-primitives** | `NodeCapabilities.to_capability_uris()` converts to URI capabilities |
| **02-graph** | `Graph::with_capability_registry()` auto-registers nodes as Participants |
| **03-latent** | `LatentManager.find_providers()` queries registry for generation capabilities |
| **03-latent** | Approval decisions track `decided_by: Uuid` (Participant ID) |
| **06-query** | Trustfall exposes `Participant`, `Capability`, `Constraint` types |

---

## Capability URI Vocabulary

URIs namespace capabilities. No central registry needed — any participant can define new URIs.

```
# Core audio capabilities
audio:realtime              - can meet audio deadlines
audio:offline               - can do offline processing
audio:channels:stereo       - supports stereo
audio:channels:surround     - supports 5.1/7.1
audio:sample_rate:48000     - supports 48kHz

# Generation capabilities
gen:midi                    - can generate MIDI
gen:audio                   - can generate audio
gen:continuation            - can continue existing material
gen:bridge                  - can create transitions
gen:variation               - can create variations

# Model-specific capabilities
model:orpheus               - Orpheus model available
model:notochord             - Notochord available
model:rave:<name>           - RAVE model available
model:musicgen              - MusicGen available

# Human-in-the-loop capabilities
hitl:approve                - can approve content
hitl:reject                 - can reject content
hitl:annotate               - can add annotations
hitl:play                   - can play instruments
hitl:voice                  - can provide voice commands

# Classification capabilities
classify:genre              - can classify genre
classify:mood               - can classify mood
classify:beats              - can detect beats
classify:chords             - can detect chords

# Hardware capabilities
hw:midi:input               - has MIDI input
hw:midi:output              - has MIDI output
hw:audio:input              - has audio input
hw:audio:output             - has audio output
hw:gpu:cuda                 - has CUDA GPU
hw:gpu:vulkan               - has Vulkan compute
```

---

## Types

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

// =============================================================================
// LIFECYCLE TRACKING
// =============================================================================

/// Generation counter for grooming. Incremented each "session" or logical epoch.
/// Items track which generation they were created/last-touched in.
/// Grooming can filter by generation to find stale items.
pub type Generation = u64;

/// Lifecycle state for any entity that can be groomed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lifecycle {
    pub created_at: DateTime<Utc>,
    pub created_generation: Generation,
    pub last_touched_at: DateTime<Utc>,
    pub last_touched_generation: Generation,
    pub tombstoned_at: Option<DateTime<Utc>>,
    pub tombstoned_generation: Option<Generation>,
    pub permanent: bool,  // if true, never tombstone/prune
}

impl Lifecycle {
    pub fn new(generation: Generation) -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            created_generation: generation,
            last_touched_at: now,
            last_touched_generation: generation,
            tombstoned_at: None,
            tombstoned_generation: None,
            permanent: false,
        }
    }

    pub fn touch(&mut self, generation: Generation) {
        self.last_touched_at = Utc::now();
        self.last_touched_generation = generation;
        // Touching rescues from tombstone
        if self.tombstoned_at.is_some() {
            self.tombstoned_at = None;
            self.tombstoned_generation = None;
        }
    }

    pub fn tombstone(&mut self, generation: Generation) {
        if !self.permanent {
            self.tombstoned_at = Some(Utc::now());
            self.tombstoned_generation = Some(generation);
        }
    }

    pub fn set_permanent(&mut self, permanent: bool) {
        self.permanent = permanent;
        if permanent {
            // Clear tombstone if marked permanent
            self.tombstoned_at = None;
            self.tombstoned_generation = None;
        }
    }

    pub fn is_tombstoned(&self) -> bool {
        self.tombstoned_at.is_some()
    }

    pub fn is_alive(&self) -> bool {
        !self.is_tombstoned()
    }
}

/// A capability URI - strongly typed for safety
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityUri(pub String);

impl CapabilityUri {
    pub fn new(uri: impl Into<String>) -> Self;

    /// Check if this URI starts with a prefix
    pub fn matches_prefix(&self, prefix: &str) -> bool;

    /// Get the namespace (before first ':')
    pub fn namespace(&self) -> &str;
}

/// Constraint on a capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub key: String,
    pub kind: ConstraintKind,
    pub value: ConstraintValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    Exact,
    Range,
    Enum,
    Min,
    Max,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConstraintValue {
    Integer(i64),
    Float(f64),
    String(String),
    IntRange { min: i64, max: i64 },
    FloatRange { min: f64, max: f64 },
    Enum(Vec<String>),
}

/// A declared capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub uri: CapabilityUri,
    pub name: String,
    pub description: Option<String>,
    pub available: bool,
    pub confidence: Option<f64>,
    pub constraints: Vec<Constraint>,
}

/// Participant kinds in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantKind {
    Human,
    Model,
    Node,
    Device,
    Agent,
}

/// Hints for re-identifying a participant when devices reconnect.
/// All fields optional - used for candidate matching and repair, not rigid keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityHints {
    pub product_name: Option<String>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,   // Best signal when present
    pub usb_product_id: Option<u16>,
    pub usb_vendor_id: Option<u16>,
    pub alsa_card_name: Option<String>,
    pub mac_address: Option<String>,     // For network devices
    pub ipv4_address: Option<String>,    // For network devices
    pub ipv6_address: Option<String>,    // For network devices (UDP streaming, etc.)
    pub user_label: Option<String>,      // Human-assigned: "atobey's eurorack"
}

/// A participant in the performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: Uuid,
    pub kind: ParticipantKind,
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub online: bool,
    pub last_seen: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub lifecycle: Lifecycle,

    /// Hints for re-identifying this participant when devices reconnect
    /// Not a rigid key - used for candidate matching and repair
    pub identity_hints: IdentityHints,

    /// Arbitrary labels for organization: "backup", "locked", "week-2", "overture"
    pub tags: Vec<String>,
}

/// A requirement for a capability with constraints
#[derive(Debug, Clone)]
pub struct CapabilityRequirement {
    pub uri: CapabilityUri,
    pub constraints: Vec<Constraint>,
}

// =============================================================================
// IDENTITY RECONCILIATION
// =============================================================================

/// Result of attempting to match a new device to existing participants
#[derive(Debug, Clone)]
pub enum IdentityMatch {
    /// High confidence match to existing participant
    Exact(Uuid),
    /// Possible matches with confidence scores (0.0-1.0)
    Candidates(Vec<IdentityCandidate>),
    /// No plausible matches - genuinely new device
    NoMatch,
}

#[derive(Debug, Clone)]
pub struct IdentityCandidate {
    pub participant_id: Uuid,
    pub participant_name: String,
    pub score: f32,
    pub matching_hints: Vec<String>,  // Which hints matched: "serial_number", "usb_ids", etc.
    pub last_seen: Option<DateTime<Utc>>,
}

/// Result of checking if requirements can be satisfied
#[derive(Debug, Clone)]
pub struct SatisfactionResult {
    pub satisfied: bool,
    pub missing: Vec<CapabilityUri>,
    pub providers: Vec<Participant>,
}
```

---

## Methods to Implement

### CapabilityUri

- `new(uri: impl Into<String>) -> Self`
- `matches_prefix(&self, prefix: &str) -> bool`
- `namespace(&self) -> &str`

### Constraint

- `satisfies(&self, requirement: &Constraint) -> bool` — check if this constraint satisfies a requirement

### Capability

- `new(uri: CapabilityUri, name: impl Into<String>) -> Self` — create with defaults
- `with_description(self, desc: impl Into<String>) -> Self` — builder
- `with_constraint(self, constraint: Constraint) -> Self` — builder

### Participant

- `new(kind: ParticipantKind, name: impl Into<String>) -> Self` — create with new UUID
- `has_capability(&self, uri: &CapabilityUri) -> bool` — check if has available capability
- `capabilities_matching(&self, prefix: &str) -> Vec<&Capability>` — filter by URI prefix
- `can_satisfy(&self, requirements: &[CapabilityRequirement]) -> bool` — check all requirements
- `add_capability(&mut self, capability: Capability)` — add a capability
- `set_capability_available(&mut self, uri: &CapabilityUri, available: bool)` — toggle availability
- `with_identity_hints(self, hints: IdentityHints) -> Self` — builder for identity hints
- `with_tag(self, tag: impl Into<String>) -> Self` — builder for tags
- `has_tag(&self, tag: &str) -> bool` — check if has tag

### IdentityHints

- `new() -> Self` — empty hints (Default)
- `with_usb(self, vendor_id: u16, product_id: u16) -> Self` — builder
- `with_serial(self, serial: impl Into<String>) -> Self` — builder
- `with_user_label(self, label: impl Into<String>) -> Self` — builder
- `match_score(&self, other: &IdentityHints) -> f32` — 0.0-1.0 similarity score

### CapabilityRequirement

- `new(uri: CapabilityUri) -> Self` — create without constraints
- `with_constraint(self, constraint: Constraint) -> Self` — builder
- `constraints_satisfied_by(&self, capability: &Capability) -> bool`

---

## CapabilityRegistry

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Central registry for participant capabilities
pub struct CapabilityRegistry {
    participants: Arc<RwLock<HashMap<Uuid, Participant>>>,
    current_generation: AtomicU64,
}

impl CapabilityRegistry {
    pub fn new() -> Self;

    // =========================================================================
    // GENERATION MANAGEMENT
    // =========================================================================

    /// Get current generation
    pub fn generation(&self) -> Generation;

    /// Advance to next generation (call at session boundaries, major events, etc.)
    pub fn advance_generation(&self) -> Generation;

    // =========================================================================
    // REGISTRATION
    // =========================================================================

    /// Register a participant with their capabilities
    /// Sets lifecycle.created_generation to current generation
    pub async fn register(&self, participant: Participant);

    /// Unregister a participant
    pub async fn unregister(&self, participant_id: Uuid);

    /// Get a participant by ID
    pub async fn get(&self, participant_id: Uuid) -> Option<Participant>;

    /// Update a participant's capabilities (full replacement)
    pub async fn update_capabilities(
        &self,
        participant_id: Uuid,
        capabilities: Vec<Capability>,
    ) -> anyhow::Result<()>;

    /// Mark a specific capability as available/unavailable
    pub async fn set_capability_available(
        &self,
        participant_id: Uuid,
        uri: &CapabilityUri,
        available: bool,
    ) -> anyhow::Result<()>;

    /// Mark participant as online/offline
    pub async fn set_online(&self, participant_id: Uuid, online: bool) -> anyhow::Result<()>;

    /// Find participants that can satisfy all requirements
    pub async fn find_satisfying(
        &self,
        requirements: &[CapabilityRequirement],
    ) -> Vec<Participant>;

    /// Query capabilities by URI prefix across all participants
    pub async fn query_capabilities(
        &self,
        prefix: &str,
    ) -> Vec<(Participant, Vec<Capability>)>;

    /// Get all online participants
    pub async fn online_participants(&self) -> Vec<Participant>;

    /// Get all participants of a specific kind
    pub async fn participants_by_kind(&self, kind: ParticipantKind) -> Vec<Participant>;

    /// Snapshot of all participants (for Trustfall queries)
    pub async fn snapshot(&self) -> Vec<Participant>;

    // =========================================================================
    // LIFECYCLE MANAGEMENT
    // =========================================================================

    /// Touch a participant (update last_touched to current generation)
    /// Call this when participant is actively used
    pub async fn touch(&self, participant_id: Uuid) -> anyhow::Result<()>;

    /// Mark a participant as permanent (immune to tombstoning)
    pub async fn set_permanent(&self, participant_id: Uuid, permanent: bool) -> anyhow::Result<()>;

    /// Tombstone a participant (soft delete, can be rescued by touch)
    pub async fn tombstone(&self, participant_id: Uuid) -> anyhow::Result<()>;

    // =========================================================================
    // GROOMING (for future use - not implemented yet)
    // =========================================================================

    /// Find participants not touched since given generation
    pub async fn stale_since(&self, generation: Generation) -> Vec<Participant>;

    /// Find tombstoned participants
    pub async fn tombstoned(&self) -> Vec<Participant>;

    /// Snapshot of only alive (non-tombstoned) participants
    pub async fn snapshot_alive(&self) -> Vec<Participant>;

    // Future: prune tombstoned participants older than threshold
    // pub async fn prune(&self, older_than_generation: Generation) -> Vec<Participant>;

    // =========================================================================
    // IDENTITY RECONCILIATION
    // =========================================================================

    /// Find existing participants that might match these identity hints
    /// Returns candidates sorted by match score (highest first)
    pub async fn find_identity_matches(&self, hints: &IdentityHints) -> IdentityMatch;

    /// Link a new device to an existing participant (repair operation)
    /// Updates the participant's identity_hints with new information
    pub async fn link_to_participant(
        &self,
        participant_id: Uuid,
        new_hints: &IdentityHints,
    ) -> anyhow::Result<()>;

    /// Find participants by tag
    pub async fn find_by_tag(&self, tag: &str) -> Vec<Participant>;

    /// Find participants by user label (exact match on identity_hints.user_label)
    pub async fn find_by_user_label(&self, label: &str) -> Option<Participant>;
}
```

---

## Example Usage

```rust
// Create a model participant
let mut orpheus = Participant::new(ParticipantKind::Model, "orpheus");
orpheus.add_capability(
    Capability::new(CapabilityUri::new("gen:midi"), "Generate MIDI")
        .with_description("Generate MIDI sequences using Orpheus model")
        .with_constraint(Constraint {
            key: "latency_ms".into(),
            kind: ConstraintKind::Range,
            value: ConstraintValue::IntRange { min: 100, max: 5000 },
        })
);
orpheus.add_capability(
    Capability::new(CapabilityUri::new("gen:continuation"), "Continue MIDI")
);
orpheus.add_capability(
    Capability::new(CapabilityUri::new("model:orpheus"), "Orpheus Model")
);

// Create a human participant
let mut human = Participant::new(ParticipantKind::Human, "alice");
human.add_capability(
    Capability::new(CapabilityUri::new("hitl:approve"), "Approve Content")
);
human.add_capability(
    Capability::new(CapabilityUri::new("hitl:annotate"), "Add Annotations")
);

// Register with registry
let registry = CapabilityRegistry::new();
registry.register(orpheus).await;
registry.register(human).await;

// Find who can generate MIDI
let generators = registry.find_satisfying(&[
    CapabilityRequirement::new(CapabilityUri::new("gen:midi")),
]).await;

// Find who can approve content
let approvers = registry.find_satisfying(&[
    CapabilityRequirement::new(CapabilityUri::new("hitl:approve")),
]).await;
```

---

## Acceptance Criteria

- [ ] `CapabilityUri::matches_prefix` correctly filters URIs
- [ ] `Participant::can_satisfy` checks all requirements
- [ ] `Constraint::satisfies` handles Exact, Range, Min, Max, Enum cases
- [ ] `CapabilityRegistry::register` adds participant
- [ ] `CapabilityRegistry::find_satisfying` returns correct participants
- [ ] `CapabilityRegistry::query_capabilities` filters by prefix
- [ ] Serialization round-trips correctly for all types
- [ ] Thread-safe access via `RwLock`
- [ ] Tests cover: registration, update, query, constraint matching
- [ ] `IdentityHints::match_score` returns sensible scores (serial match > usb_ids match > name match)
- [ ] `find_identity_matches` returns Exact for high-confidence matches
- [ ] `find_identity_matches` returns Candidates sorted by score
- [ ] `link_to_participant` merges new hints with existing
- [ ] `find_by_tag` returns participants with matching tag
- [ ] `Participant.tags` serializes correctly
