//! Participant capability declaration, registration, and querying
//!
//! Chaosgarden is a performance space where diverse participants collaborate as equals.
//! This module provides capability discovery so participants can compose workflows
//! and coordinate effectively.
//!
//! Key concepts:
//! - Participants declare capabilities (no negotiation)
//! - URI namespacing allows extension without central coordination
//! - Pull-based discovery (participants poll the registry)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::primitives::{Generation, Lifecycle};

/// A capability URI - strongly typed for safety
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityUri(pub String);

impl CapabilityUri {
    pub fn new(uri: impl Into<String>) -> Self {
        Self(uri.into())
    }

    /// Check if this URI starts with a prefix
    pub fn matches_prefix(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }

    /// Get the namespace (before first ':')
    pub fn namespace(&self) -> &str {
        self.0.split(':').next().unwrap_or(&self.0)
    }

    /// Get the full URI string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CapabilityUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Kind of constraint on a capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    Exact,
    Range,
    Enum,
    Min,
    Max,
}

/// Value for a constraint
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

/// Constraint on a capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub key: String,
    pub kind: ConstraintKind,
    pub value: ConstraintValue,
}

impl Constraint {
    /// Check if this constraint satisfies a requirement constraint
    pub fn satisfies(&self, requirement: &Constraint) -> bool {
        if self.key != requirement.key {
            return false;
        }

        match (
            &self.kind,
            &requirement.kind,
            &self.value,
            &requirement.value,
        ) {
            // Exact match
            (ConstraintKind::Exact, ConstraintKind::Exact, v1, v2) => v1 == v2,

            // Range satisfies exact if value is in range
            (
                ConstraintKind::Range,
                ConstraintKind::Exact,
                ConstraintValue::IntRange { min, max },
                ConstraintValue::Integer(v),
            ) => *v >= *min && *v <= *max,
            (
                ConstraintKind::Range,
                ConstraintKind::Exact,
                ConstraintValue::FloatRange { min, max },
                ConstraintValue::Float(v),
            ) => *v >= *min && *v <= *max,

            // Range satisfies range if contained
            (
                ConstraintKind::Range,
                ConstraintKind::Range,
                ConstraintValue::IntRange {
                    min: s_min,
                    max: s_max,
                },
                ConstraintValue::IntRange {
                    min: r_min,
                    max: r_max,
                },
            ) => s_min <= r_min && s_max >= r_max,
            (
                ConstraintKind::Range,
                ConstraintKind::Range,
                ConstraintValue::FloatRange {
                    min: s_min,
                    max: s_max,
                },
                ConstraintValue::FloatRange {
                    min: r_min,
                    max: r_max,
                },
            ) => s_min <= r_min && s_max >= r_max,

            // Min constraint
            (ConstraintKind::Min, _, ConstraintValue::Integer(s), ConstraintValue::Integer(r)) => {
                s >= r
            }
            (ConstraintKind::Min, _, ConstraintValue::Float(s), ConstraintValue::Float(r)) => {
                s >= r
            }

            // Max constraint
            (ConstraintKind::Max, _, ConstraintValue::Integer(s), ConstraintValue::Integer(r)) => {
                s <= r
            }
            (ConstraintKind::Max, _, ConstraintValue::Float(s), ConstraintValue::Float(r)) => {
                s <= r
            }

            // Enum satisfies if all required values are present
            (
                ConstraintKind::Enum,
                ConstraintKind::Enum,
                ConstraintValue::Enum(s),
                ConstraintValue::Enum(r),
            ) => r.iter().all(|v| s.contains(v)),

            // Enum satisfies exact if value is in enum
            (
                ConstraintKind::Enum,
                ConstraintKind::Exact,
                ConstraintValue::Enum(s),
                ConstraintValue::String(r),
            ) => s.contains(r),

            _ => false,
        }
    }
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

impl Capability {
    pub fn new(uri: CapabilityUri, name: impl Into<String>) -> Self {
        Self {
            uri,
            name: name.into(),
            description: None,
            available: true,
            confidence: None,
            constraints: Vec::new(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    /// Check if this capability satisfies all constraints of a requirement
    pub fn satisfies_constraints(&self, requirement_constraints: &[Constraint]) -> bool {
        requirement_constraints
            .iter()
            .all(|req| self.constraints.iter().any(|c| c.satisfies(req)))
    }
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

/// Hints for re-identifying a participant when devices reconnect
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityHints {
    pub product_name: Option<String>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
    pub usb_product_id: Option<u16>,
    pub usb_vendor_id: Option<u16>,
    pub alsa_card_name: Option<String>,
    pub mac_address: Option<String>,
    pub ipv4_address: Option<String>,
    pub ipv6_address: Option<String>,
    pub user_label: Option<String>,
}

impl IdentityHints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_usb(mut self, vendor_id: u16, product_id: u16) -> Self {
        self.usb_vendor_id = Some(vendor_id);
        self.usb_product_id = Some(product_id);
        self
    }

    pub fn with_serial(mut self, serial: impl Into<String>) -> Self {
        self.serial_number = Some(serial.into());
        self
    }

    pub fn with_user_label(mut self, label: impl Into<String>) -> Self {
        self.user_label = Some(label.into());
        self
    }

    pub fn with_product_name(mut self, name: impl Into<String>) -> Self {
        self.product_name = Some(name.into());
        self
    }

    pub fn with_manufacturer(mut self, manufacturer: impl Into<String>) -> Self {
        self.manufacturer = Some(manufacturer.into());
        self
    }

    /// Compute a match score (0.0-1.0) against another set of hints
    /// Higher scores mean better matches
    /// Only considers fields that BOTH sides have set
    pub fn match_score(&self, other: &IdentityHints) -> f32 {
        let mut score = 0.0f32;
        let mut max_score = 0.0f32;

        // Serial number is the strongest signal (weight: 0.5)
        // Only counts if BOTH have serial
        if self.serial_number.is_some() && other.serial_number.is_some() {
            max_score += 0.5;
            if self.serial_number == other.serial_number {
                score += 0.5;
            }
        }

        // USB IDs together are strong (weight: 0.25)
        // Only counts if BOTH have USB IDs
        let self_has_usb = self.usb_vendor_id.is_some() && self.usb_product_id.is_some();
        let other_has_usb = other.usb_vendor_id.is_some() && other.usb_product_id.is_some();
        if self_has_usb && other_has_usb {
            max_score += 0.25;
            if self.usb_vendor_id == other.usb_vendor_id
                && self.usb_product_id == other.usb_product_id
            {
                score += 0.25;
            }
        }

        // User label is medium (weight: 0.15)
        if self.user_label.is_some() && other.user_label.is_some() {
            max_score += 0.15;
            if self.user_label == other.user_label {
                score += 0.15;
            }
        }

        // MAC address (weight: 0.05)
        if self.mac_address.is_some() && other.mac_address.is_some() {
            max_score += 0.05;
            if self.mac_address == other.mac_address {
                score += 0.05;
            }
        }

        // Product name (weight: 0.025)
        if self.product_name.is_some() && other.product_name.is_some() {
            max_score += 0.025;
            if self.product_name == other.product_name {
                score += 0.025;
            }
        }

        // Manufacturer (weight: 0.025)
        if self.manufacturer.is_some() && other.manufacturer.is_some() {
            max_score += 0.025;
            if self.manufacturer == other.manufacturer {
                score += 0.025;
            }
        }

        if max_score > 0.0 {
            score / max_score
        } else {
            0.0
        }
    }

    /// Merge hints from another set (non-destructive - only fills in missing values)
    pub fn merge(&mut self, other: &IdentityHints) {
        if self.product_name.is_none() {
            self.product_name = other.product_name.clone();
        }
        if self.manufacturer.is_none() {
            self.manufacturer = other.manufacturer.clone();
        }
        if self.serial_number.is_none() {
            self.serial_number = other.serial_number.clone();
        }
        if self.usb_product_id.is_none() {
            self.usb_product_id = other.usb_product_id;
        }
        if self.usb_vendor_id.is_none() {
            self.usb_vendor_id = other.usb_vendor_id;
        }
        if self.alsa_card_name.is_none() {
            self.alsa_card_name = other.alsa_card_name.clone();
        }
        if self.mac_address.is_none() {
            self.mac_address = other.mac_address.clone();
        }
        if self.ipv4_address.is_none() {
            self.ipv4_address = other.ipv4_address.clone();
        }
        if self.ipv6_address.is_none() {
            self.ipv6_address = other.ipv6_address.clone();
        }
        if self.user_label.is_none() {
            self.user_label = other.user_label.clone();
        }
    }

    /// Get hints that matched (for diagnostics)
    pub fn matching_hints(&self, other: &IdentityHints) -> Vec<String> {
        let mut matches = Vec::new();

        if self.serial_number.is_some() && self.serial_number == other.serial_number {
            matches.push("serial_number".to_string());
        }
        if self.usb_vendor_id.is_some()
            && self.usb_vendor_id == other.usb_vendor_id
            && self.usb_product_id == other.usb_product_id
        {
            matches.push("usb_ids".to_string());
        }
        if self.user_label.is_some() && self.user_label == other.user_label {
            matches.push("user_label".to_string());
        }
        if self.mac_address.is_some() && self.mac_address == other.mac_address {
            matches.push("mac_address".to_string());
        }
        if self.product_name.is_some() && self.product_name == other.product_name {
            matches.push("product_name".to_string());
        }
        if self.manufacturer.is_some() && self.manufacturer == other.manufacturer {
            matches.push("manufacturer".to_string());
        }

        matches
    }
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
    pub identity_hints: IdentityHints,
    pub tags: Vec<String>,
}

impl Participant {
    pub fn new(kind: ParticipantKind, name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            name: name.into(),
            capabilities: Vec::new(),
            online: true,
            last_seen: Some(Utc::now()),
            metadata: HashMap::new(),
            lifecycle: Lifecycle::new(0),
            identity_hints: IdentityHints::default(),
            tags: Vec::new(),
        }
    }

    /// Check if participant has a capability (available)
    pub fn has_capability(&self, uri: &CapabilityUri) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.uri == *uri && c.available)
    }

    /// Get capabilities matching a URI prefix
    pub fn capabilities_matching(&self, prefix: &str) -> Vec<&Capability> {
        self.capabilities
            .iter()
            .filter(|c| c.uri.matches_prefix(prefix))
            .collect()
    }

    /// Check if participant can satisfy all requirements
    pub fn can_satisfy(&self, requirements: &[CapabilityRequirement]) -> bool {
        requirements.iter().all(|req| {
            self.capabilities.iter().any(|cap| {
                cap.uri == req.uri && cap.available && cap.satisfies_constraints(&req.constraints)
            })
        })
    }

    /// Add a capability
    pub fn add_capability(&mut self, capability: Capability) {
        self.capabilities.push(capability);
    }

    /// Set a capability's availability
    pub fn set_capability_available(&mut self, uri: &CapabilityUri, available: bool) {
        for cap in &mut self.capabilities {
            if cap.uri == *uri {
                cap.available = available;
            }
        }
    }

    /// Builder for identity hints
    pub fn with_identity_hints(mut self, hints: IdentityHints) -> Self {
        self.identity_hints = hints;
        self
    }

    /// Builder for adding a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Check if participant has a tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// Add a tag (if not already present)
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        let tag = tag.into();
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// Remove a tag
    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.retain(|t| t != tag);
    }
}

/// A requirement for a capability with constraints
#[derive(Debug, Clone)]
pub struct CapabilityRequirement {
    pub uri: CapabilityUri,
    pub constraints: Vec<Constraint>,
}

impl CapabilityRequirement {
    pub fn new(uri: CapabilityUri) -> Self {
        Self {
            uri,
            constraints: Vec::new(),
        }
    }

    pub fn with_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Check if a capability satisfies this requirement
    pub fn satisfied_by(&self, capability: &Capability) -> bool {
        capability.uri == self.uri
            && capability.available
            && capability.satisfies_constraints(&self.constraints)
    }
}

/// Result of attempting to match a new device to existing participants
#[derive(Debug, Clone)]
pub enum IdentityMatch {
    /// High confidence match to existing participant
    Exact(Uuid),
    /// Possible matches with confidence scores
    Candidates(Vec<IdentityCandidate>),
    /// No plausible matches
    NoMatch,
}

/// A candidate match for identity reconciliation
#[derive(Debug, Clone)]
pub struct IdentityCandidate {
    pub participant_id: Uuid,
    pub participant_name: String,
    pub score: f32,
    pub matching_hints: Vec<String>,
    pub last_seen: Option<DateTime<Utc>>,
}

/// Result of checking if requirements can be satisfied
#[derive(Debug, Clone)]
pub struct SatisfactionResult {
    pub satisfied: bool,
    pub missing: Vec<CapabilityUri>,
    pub providers: Vec<Participant>,
}

/// Central registry for participant capabilities
pub struct CapabilityRegistry {
    participants: Arc<RwLock<HashMap<Uuid, Participant>>>,
    current_generation: AtomicU64,
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            participants: Arc::new(RwLock::new(HashMap::new())),
            current_generation: AtomicU64::new(0),
        }
    }

    /// Get current generation
    pub fn generation(&self) -> Generation {
        self.current_generation.load(Ordering::Acquire)
    }

    /// Advance to next generation
    pub fn advance_generation(&self) -> Generation {
        self.current_generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Register a participant
    pub async fn register(&self, mut participant: Participant) {
        let gen = self.generation();
        participant.lifecycle = Lifecycle::new(gen);
        participant.last_seen = Some(Utc::now());

        let mut participants = self.participants.write().await;
        participants.insert(participant.id, participant);
    }

    /// Unregister a participant
    pub async fn unregister(&self, participant_id: Uuid) {
        let mut participants = self.participants.write().await;
        participants.remove(&participant_id);
    }

    /// Get a participant by ID
    pub async fn get(&self, participant_id: Uuid) -> Option<Participant> {
        let participants = self.participants.read().await;
        participants.get(&participant_id).cloned()
    }

    /// Update a participant's capabilities (full replacement)
    pub async fn update_capabilities(
        &self,
        participant_id: Uuid,
        capabilities: Vec<Capability>,
    ) -> anyhow::Result<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or_else(|| anyhow::anyhow!("Participant not found: {}", participant_id))?;

        participant.capabilities = capabilities;
        participant.lifecycle.touch(self.generation());
        Ok(())
    }

    /// Mark a specific capability as available/unavailable
    pub async fn set_capability_available(
        &self,
        participant_id: Uuid,
        uri: &CapabilityUri,
        available: bool,
    ) -> anyhow::Result<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or_else(|| anyhow::anyhow!("Participant not found: {}", participant_id))?;

        participant.set_capability_available(uri, available);
        participant.lifecycle.touch(self.generation());
        Ok(())
    }

    /// Mark participant as online/offline
    pub async fn set_online(&self, participant_id: Uuid, online: bool) -> anyhow::Result<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or_else(|| anyhow::anyhow!("Participant not found: {}", participant_id))?;

        participant.online = online;
        if online {
            participant.last_seen = Some(Utc::now());
        }
        participant.lifecycle.touch(self.generation());
        Ok(())
    }

    /// Find participants that can satisfy all requirements
    pub async fn find_satisfying(
        &self,
        requirements: &[CapabilityRequirement],
    ) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.online && p.lifecycle.is_alive() && p.can_satisfy(requirements))
            .cloned()
            .collect()
    }

    /// Query capabilities by URI prefix across all participants
    pub async fn query_capabilities(&self, prefix: &str) -> Vec<(Participant, Vec<Capability>)> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.lifecycle.is_alive())
            .filter_map(|p| {
                let matching: Vec<_> = p
                    .capabilities
                    .iter()
                    .filter(|c| c.uri.matches_prefix(prefix))
                    .cloned()
                    .collect();
                if matching.is_empty() {
                    None
                } else {
                    Some((p.clone(), matching))
                }
            })
            .collect()
    }

    /// Get all online participants
    pub async fn online_participants(&self) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.online && p.lifecycle.is_alive())
            .cloned()
            .collect()
    }

    /// Get all participants of a specific kind
    pub async fn participants_by_kind(&self, kind: ParticipantKind) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.kind == kind && p.lifecycle.is_alive())
            .cloned()
            .collect()
    }

    /// Snapshot of all participants (for Trustfall queries)
    pub async fn snapshot(&self) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants.values().cloned().collect()
    }

    /// Snapshot of only alive participants
    pub async fn snapshot_alive(&self) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.lifecycle.is_alive())
            .cloned()
            .collect()
    }

    /// Touch a participant (update last_touched)
    pub async fn touch(&self, participant_id: Uuid) -> anyhow::Result<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or_else(|| anyhow::anyhow!("Participant not found: {}", participant_id))?;

        participant.lifecycle.touch(self.generation());
        participant.last_seen = Some(Utc::now());
        Ok(())
    }

    /// Mark a participant as permanent
    pub async fn set_permanent(&self, participant_id: Uuid, permanent: bool) -> anyhow::Result<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or_else(|| anyhow::anyhow!("Participant not found: {}", participant_id))?;

        participant.lifecycle.set_permanent(permanent);
        Ok(())
    }

    /// Tombstone a participant
    pub async fn tombstone(&self, participant_id: Uuid) -> anyhow::Result<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or_else(|| anyhow::anyhow!("Participant not found: {}", participant_id))?;

        participant.lifecycle.tombstone(self.generation());
        Ok(())
    }

    /// Find participants not touched since given generation
    pub async fn stale_since(&self, generation: Generation) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.lifecycle.last_touched_generation < generation && p.lifecycle.is_alive())
            .cloned()
            .collect()
    }

    /// Find tombstoned participants
    pub async fn tombstoned(&self) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.lifecycle.is_tombstoned())
            .cloned()
            .collect()
    }

    /// Find existing participants that might match identity hints
    pub async fn find_identity_matches(&self, hints: &IdentityHints) -> IdentityMatch {
        let participants = self.participants.read().await;

        let mut candidates: Vec<_> = participants
            .values()
            .filter_map(|p| {
                let score = p.identity_hints.match_score(hints);
                if score > 0.0 {
                    Some(IdentityCandidate {
                        participant_id: p.id,
                        participant_name: p.name.clone(),
                        score,
                        matching_hints: p.identity_hints.matching_hints(hints),
                        last_seen: p.last_seen,
                    })
                } else {
                    None
                }
            })
            .collect();

        if candidates.is_empty() {
            return IdentityMatch::NoMatch;
        }

        // Sort by score descending
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        // If top score is very high (>0.9), treat as exact match
        if candidates[0].score > 0.9 {
            return IdentityMatch::Exact(candidates[0].participant_id);
        }

        IdentityMatch::Candidates(candidates)
    }

    /// Link a new device to an existing participant
    pub async fn link_to_participant(
        &self,
        participant_id: Uuid,
        new_hints: &IdentityHints,
    ) -> anyhow::Result<()> {
        let mut participants = self.participants.write().await;
        let participant = participants
            .get_mut(&participant_id)
            .ok_or_else(|| anyhow::anyhow!("Participant not found: {}", participant_id))?;

        participant.identity_hints.merge(new_hints);
        participant.lifecycle.touch(self.generation());
        participant.last_seen = Some(Utc::now());
        Ok(())
    }

    /// Find participants by tag
    pub async fn find_by_tag(&self, tag: &str) -> Vec<Participant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .filter(|p| p.has_tag(tag) && p.lifecycle.is_alive())
            .cloned()
            .collect()
    }

    /// Find participant by user label
    pub async fn find_by_user_label(&self, label: &str) -> Option<Participant> {
        let participants = self.participants.read().await;
        participants
            .values()
            .find(|p| {
                p.identity_hints
                    .user_label
                    .as_ref()
                    .map(|l| l == label)
                    .unwrap_or(false)
                    && p.lifecycle.is_alive()
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_uri_creation() {
        let uri = CapabilityUri::new("gen:midi");
        assert_eq!(uri.as_str(), "gen:midi");
        assert_eq!(uri.namespace(), "gen");
    }

    #[test]
    fn test_capability_uri_matches_prefix() {
        let uri = CapabilityUri::new("gen:midi:continuation");
        assert!(uri.matches_prefix("gen:"));
        assert!(uri.matches_prefix("gen:midi"));
        assert!(!uri.matches_prefix("audio:"));
    }

    #[test]
    fn test_constraint_exact_match() {
        let c1 = Constraint {
            key: "latency".into(),
            kind: ConstraintKind::Exact,
            value: ConstraintValue::Integer(100),
        };
        let c2 = Constraint {
            key: "latency".into(),
            kind: ConstraintKind::Exact,
            value: ConstraintValue::Integer(100),
        };
        let c3 = Constraint {
            key: "latency".into(),
            kind: ConstraintKind::Exact,
            value: ConstraintValue::Integer(200),
        };

        assert!(c1.satisfies(&c2));
        assert!(!c1.satisfies(&c3));
    }

    #[test]
    fn test_constraint_range_satisfies_exact() {
        let range = Constraint {
            key: "latency".into(),
            kind: ConstraintKind::Range,
            value: ConstraintValue::IntRange { min: 50, max: 200 },
        };
        let exact_in = Constraint {
            key: "latency".into(),
            kind: ConstraintKind::Exact,
            value: ConstraintValue::Integer(100),
        };
        let exact_out = Constraint {
            key: "latency".into(),
            kind: ConstraintKind::Exact,
            value: ConstraintValue::Integer(300),
        };

        assert!(range.satisfies(&exact_in));
        assert!(!range.satisfies(&exact_out));
    }

    #[test]
    fn test_constraint_enum_satisfies() {
        let enum_cap = Constraint {
            key: "format".into(),
            kind: ConstraintKind::Enum,
            value: ConstraintValue::Enum(vec!["wav".into(), "mp3".into(), "flac".into()]),
        };
        let exact_in = Constraint {
            key: "format".into(),
            kind: ConstraintKind::Exact,
            value: ConstraintValue::String("wav".into()),
        };
        let exact_out = Constraint {
            key: "format".into(),
            kind: ConstraintKind::Exact,
            value: ConstraintValue::String("ogg".into()),
        };

        assert!(enum_cap.satisfies(&exact_in));
        assert!(!enum_cap.satisfies(&exact_out));
    }

    #[test]
    fn test_capability_creation() {
        let cap = Capability::new(CapabilityUri::new("gen:midi"), "Generate MIDI")
            .with_description("Generate MIDI sequences")
            .with_confidence(0.95);

        assert_eq!(cap.name, "Generate MIDI");
        assert_eq!(cap.description, Some("Generate MIDI sequences".to_string()));
        assert_eq!(cap.confidence, Some(0.95));
        assert!(cap.available);
    }

    #[test]
    fn test_participant_creation() {
        let p = Participant::new(ParticipantKind::Model, "orpheus");
        assert_eq!(p.name, "orpheus");
        assert_eq!(p.kind, ParticipantKind::Model);
        assert!(p.online);
    }

    #[test]
    fn test_participant_capabilities() {
        let mut p = Participant::new(ParticipantKind::Model, "orpheus");
        p.add_capability(Capability::new(
            CapabilityUri::new("gen:midi"),
            "Generate MIDI",
        ));
        p.add_capability(Capability::new(
            CapabilityUri::new("gen:continuation"),
            "Continue MIDI",
        ));

        assert!(p.has_capability(&CapabilityUri::new("gen:midi")));
        assert!(!p.has_capability(&CapabilityUri::new("gen:audio")));

        let gen_caps = p.capabilities_matching("gen:");
        assert_eq!(gen_caps.len(), 2);
    }

    #[test]
    fn test_participant_can_satisfy() {
        let mut p = Participant::new(ParticipantKind::Model, "orpheus");
        p.add_capability(
            Capability::new(CapabilityUri::new("gen:midi"), "Generate MIDI").with_constraint(
                Constraint {
                    key: "latency".into(),
                    kind: ConstraintKind::Range,
                    value: ConstraintValue::IntRange { min: 50, max: 500 },
                },
            ),
        );

        let req = CapabilityRequirement::new(CapabilityUri::new("gen:midi")).with_constraint(
            Constraint {
                key: "latency".into(),
                kind: ConstraintKind::Exact,
                value: ConstraintValue::Integer(100),
            },
        );

        assert!(p.can_satisfy(&[req]));

        let req_fail = CapabilityRequirement::new(CapabilityUri::new("gen:midi")).with_constraint(
            Constraint {
                key: "latency".into(),
                kind: ConstraintKind::Exact,
                value: ConstraintValue::Integer(1000),
            },
        );

        assert!(!p.can_satisfy(&[req_fail]));
    }

    #[test]
    fn test_participant_tags() {
        let p = Participant::new(ParticipantKind::Device, "keyboard")
            .with_tag("primary")
            .with_tag("midi");

        assert!(p.has_tag("primary"));
        assert!(p.has_tag("midi"));
        assert!(!p.has_tag("audio"));
    }

    #[test]
    fn test_identity_hints_match_score() {
        let hints1 = IdentityHints::new()
            .with_serial("ABC123")
            .with_usb(0x1234, 0x5678);

        let hints2 = IdentityHints::new()
            .with_serial("ABC123")
            .with_usb(0x1234, 0x5678);

        let hints3 = IdentityHints::new()
            .with_serial("XYZ789")
            .with_usb(0x1234, 0x5678);

        let hints4 = IdentityHints::new().with_serial("DEF456");

        // Same serial + USB should be 1.0
        assert!((hints1.match_score(&hints2) - 1.0).abs() < 0.01);

        // Same USB, different serial
        let score = hints1.match_score(&hints3);
        assert!(score > 0.0 && score < 1.0);

        // Different serial, no USB overlap
        let score2 = hints1.match_score(&hints4);
        assert!(score2 < score);
    }

    #[test]
    fn test_identity_hints_merge() {
        let mut hints1 = IdentityHints::new().with_serial("ABC123");
        let hints2 = IdentityHints::new()
            .with_usb(0x1234, 0x5678)
            .with_user_label("My Keyboard");

        hints1.merge(&hints2);

        assert_eq!(hints1.serial_number, Some("ABC123".to_string()));
        assert_eq!(hints1.usb_vendor_id, Some(0x1234));
        assert_eq!(hints1.usb_product_id, Some(0x5678));
        assert_eq!(hints1.user_label, Some("My Keyboard".to_string()));
    }

    #[tokio::test]
    async fn test_registry_register_and_get() {
        let registry = CapabilityRegistry::new();

        let p = Participant::new(ParticipantKind::Model, "orpheus");
        let id = p.id;
        registry.register(p).await;

        let retrieved = registry.get(id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "orpheus");
    }

    #[tokio::test]
    async fn test_registry_find_satisfying() {
        let registry = CapabilityRegistry::new();

        let mut orpheus = Participant::new(ParticipantKind::Model, "orpheus");
        orpheus.add_capability(Capability::new(
            CapabilityUri::new("gen:midi"),
            "Generate MIDI",
        ));
        registry.register(orpheus).await;

        let mut human = Participant::new(ParticipantKind::Human, "alice");
        human.add_capability(Capability::new(
            CapabilityUri::new("hitl:approve"),
            "Approve Content",
        ));
        registry.register(human).await;

        let generators = registry
            .find_satisfying(&[CapabilityRequirement::new(CapabilityUri::new("gen:midi"))])
            .await;
        assert_eq!(generators.len(), 1);
        assert_eq!(generators[0].name, "orpheus");

        let approvers = registry
            .find_satisfying(&[CapabilityRequirement::new(CapabilityUri::new(
                "hitl:approve",
            ))])
            .await;
        assert_eq!(approvers.len(), 1);
        assert_eq!(approvers[0].name, "alice");
    }

    #[tokio::test]
    async fn test_registry_query_capabilities() {
        let registry = CapabilityRegistry::new();

        let mut p = Participant::new(ParticipantKind::Model, "orpheus");
        p.add_capability(Capability::new(
            CapabilityUri::new("gen:midi"),
            "Generate MIDI",
        ));
        p.add_capability(Capability::new(
            CapabilityUri::new("gen:continuation"),
            "Continue",
        ));
        p.add_capability(Capability::new(
            CapabilityUri::new("model:orpheus"),
            "Orpheus Model",
        ));
        registry.register(p).await;

        let gen_caps = registry.query_capabilities("gen:").await;
        assert_eq!(gen_caps.len(), 1);
        assert_eq!(gen_caps[0].1.len(), 2);
    }

    #[tokio::test]
    async fn test_registry_online_participants() {
        let registry = CapabilityRegistry::new();

        let p1 = Participant::new(ParticipantKind::Model, "orpheus");
        let id1 = p1.id;
        registry.register(p1).await;

        let p2 = Participant::new(ParticipantKind::Human, "alice");
        let id2 = p2.id;
        registry.register(p2).await;

        registry.set_online(id1, false).await.unwrap();

        let online = registry.online_participants().await;
        assert_eq!(online.len(), 1);
        assert_eq!(online[0].id, id2);
    }

    #[tokio::test]
    async fn test_registry_participants_by_kind() {
        let registry = CapabilityRegistry::new();

        registry
            .register(Participant::new(ParticipantKind::Model, "orpheus"))
            .await;
        registry
            .register(Participant::new(ParticipantKind::Model, "musicgen"))
            .await;
        registry
            .register(Participant::new(ParticipantKind::Human, "alice"))
            .await;

        let models = registry.participants_by_kind(ParticipantKind::Model).await;
        assert_eq!(models.len(), 2);

        let humans = registry.participants_by_kind(ParticipantKind::Human).await;
        assert_eq!(humans.len(), 1);
    }

    #[tokio::test]
    async fn test_registry_lifecycle() {
        let registry = CapabilityRegistry::new();

        let p = Participant::new(ParticipantKind::Device, "keyboard");
        let id = p.id;
        registry.register(p).await;

        registry.advance_generation();
        registry.advance_generation();

        let stale = registry.stale_since(1).await;
        assert_eq!(stale.len(), 1);

        registry.touch(id).await.unwrap();

        let stale_after = registry.stale_since(1).await;
        assert_eq!(stale_after.len(), 0);
    }

    #[tokio::test]
    async fn test_registry_tombstone() {
        let registry = CapabilityRegistry::new();

        let p = Participant::new(ParticipantKind::Device, "keyboard");
        let id = p.id;
        registry.register(p).await;

        registry.tombstone(id).await.unwrap();

        let tombstoned = registry.tombstoned().await;
        assert_eq!(tombstoned.len(), 1);

        let alive = registry.snapshot_alive().await;
        assert_eq!(alive.len(), 0);

        // Touch rescues from tombstone
        registry.touch(id).await.unwrap();

        let alive_after = registry.snapshot_alive().await;
        assert_eq!(alive_after.len(), 1);
    }

    #[tokio::test]
    async fn test_registry_find_identity_matches() {
        let registry = CapabilityRegistry::new();

        let p = Participant::new(ParticipantKind::Device, "keyboard").with_identity_hints(
            IdentityHints::new()
                .with_serial("ABC123")
                .with_usb(0x1234, 0x5678),
        );
        registry.register(p).await;

        // Exact match with serial (both have serial, they match -> score 1.0)
        let hints = IdentityHints::new().with_serial("ABC123");
        let result = registry.find_identity_matches(&hints).await;
        assert!(matches!(result, IdentityMatch::Exact(_)));

        // Exact match with USB (both have USB, they match -> score 1.0)
        let hints2 = IdentityHints::new().with_usb(0x1234, 0x5678);
        let result2 = registry.find_identity_matches(&hints2).await;
        assert!(matches!(result2, IdentityMatch::Exact(_)));

        // No match - different serial (both have serial, they don't match)
        let hints3 = IdentityHints::new().with_serial("XYZ999");
        let result3 = registry.find_identity_matches(&hints3).await;
        assert!(matches!(result3, IdentityMatch::NoMatch));

        // Partial match - serial matches but USB doesn't
        let hints4 = IdentityHints::new()
            .with_serial("ABC123")
            .with_usb(0xAAAA, 0xBBBB);
        let result4 = registry.find_identity_matches(&hints4).await;
        // Score: 0.5/0.75 = 0.67, which is < 0.9 so it's Candidates
        assert!(matches!(result4, IdentityMatch::Candidates(_)));
    }

    #[tokio::test]
    async fn test_registry_find_by_tag() {
        let registry = CapabilityRegistry::new();

        registry
            .register(Participant::new(ParticipantKind::Device, "keyboard1").with_tag("primary"))
            .await;
        registry
            .register(Participant::new(ParticipantKind::Device, "keyboard2").with_tag("backup"))
            .await;

        let primary = registry.find_by_tag("primary").await;
        assert_eq!(primary.len(), 1);
        assert_eq!(primary[0].name, "keyboard1");
    }

    #[tokio::test]
    async fn test_registry_find_by_user_label() {
        let registry = CapabilityRegistry::new();

        registry
            .register(
                Participant::new(ParticipantKind::Device, "keyboard")
                    .with_identity_hints(IdentityHints::new().with_user_label("atobey's eurorack")),
            )
            .await;

        let found = registry.find_by_user_label("atobey's eurorack").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "keyboard");

        let not_found = registry.find_by_user_label("nonexistent").await;
        assert!(not_found.is_none());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut p = Participant::new(ParticipantKind::Model, "orpheus")
            .with_tag("primary")
            .with_identity_hints(IdentityHints::new().with_serial("ABC123"));

        p.add_capability(
            Capability::new(CapabilityUri::new("gen:midi"), "Generate MIDI")
                .with_description("Generate MIDI sequences")
                .with_constraint(Constraint {
                    key: "latency".into(),
                    kind: ConstraintKind::Range,
                    value: ConstraintValue::IntRange { min: 50, max: 500 },
                }),
        );

        let json = serde_json::to_string(&p).unwrap();
        let loaded: Participant = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.name, "orpheus");
        assert_eq!(loaded.capabilities.len(), 1);
        assert!(loaded.has_tag("primary"));
        assert_eq!(
            loaded.identity_hints.serial_number,
            Some("ABC123".to_string())
        );
    }
}
