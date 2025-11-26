# Task 03: Identity Hint Matching System

**Status**: ✅ Complete (5 tests passing)
**Estimated effort**: 2-3 hours
**Prerequisites**: Task 01 (SQLite), Task 02 (ALSA enumeration)
**Depends on**: Database layer, device fingerprints
**Enables**: Task 04 (Trustfall joins), Task 05 (MCP tools)

## 🎯 Goal

Implement the **identity matching algorithm** that takes device fingerprints from live sources (ALSA, PipeWire, USB) and matches them to persisted identities in the database.

**Why this matters:** Devices don't have stable names. We need fuzzy matching with confidence scoring to handle:
- USB paths that change: `hw:2,0` → `hw:3,0` after reboot
- Name variations: "JD-Xi" vs "Roland JD-Xi MIDI 1"
- Missing/partial fingerprints
- Ambiguous matches (multiple candidates)

## 📋 Context

### The Matching Problem

Given a live device with fingerprints:
```rust
[
    DeviceFingerprint { kind: AlsaCard, value: "Roland JD-Xi" },
    DeviceFingerprint { kind: AlsaHw, value: "hw:2,0" },
    DeviceFingerprint { kind: MidiName, value: "JD-Xi MIDI 1" },
]
```

And database identities with hints:
```sql
-- Identity "jdxi"
hints:
  (usb_device_id, "0582:0160", confidence: 1.0)
  (midi_name, "JD-Xi", confidence: 0.9)
  (alsa_card, "Roland JD-Xi", confidence: 0.8)

-- Identity "keystep"
hints:
  (usb_device_id, "1c75:0263", confidence: 1.0)
  (midi_name, "Keystep Pro", confidence: 0.9)
```

**Goal:** Match the device to `jdxi` identity with high confidence.

### Matching Strategy

1. **Extract fingerprints** from live device (Task 02 provides this)
2. **Query database** for all identities with matching hints
3. **Score each candidate** based on:
   - Hint confidence (0.0 to 1.0)
   - Number of matching hints
   - Hint strength (USB ID > MIDI name > ALSA card)
4. **Return best match** with confidence threshold:
   - **≥ 0.9**: Auto-bind (high confidence)
   - **0.5-0.9**: Suggest to user (medium confidence)
   - **< 0.5**: Unbound (low confidence)

## 🎨 Types (add to src/types.rs)

```rust
/// Result of identity matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityMatch {
    pub identity: Identity,
    pub score: f64,           // 0.0 to 1.0
    pub matched_hints: Vec<MatchedHint>,
    pub confidence: MatchConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedHint {
    pub kind: HintKind,
    pub value: String,
    pub hint_confidence: f64,  // From database
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchConfidence {
    High,        // ≥ 0.9 - auto-bind
    Medium,      // 0.5-0.9 - ask user
    Low,         // < 0.5 - unbound
}

impl MatchConfidence {
    pub fn from_score(score: f64) -> Self {
        if score >= 0.9 {
            Self::High
        } else if score >= 0.5 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}
```

## 🔨 Implementation (src/matcher.rs)

```rust
use crate::db::Database;
use crate::types::*;
use anyhow::Result;

pub struct IdentityMatcher<'a> {
    db: &'a Database,
}

impl<'a> IdentityMatcher<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Match device fingerprints to identities
    /// Returns all candidates sorted by score (highest first)
    pub fn match_device(&self, fingerprints: &[DeviceFingerprint]) -> Result<Vec<IdentityMatch>> {
        let mut candidates: HashMap<String, Vec<MatchedHint>> = HashMap::new();

        // For each fingerprint, find matching hints in database
        for print in fingerprints {
            let matches = self.db.find_hints_by_kind_value(print.kind, &print.value)?;

            for hint in matches {
                candidates
                    .entry(hint.identity_id.clone())
                    .or_default()
                    .push(MatchedHint {
                        kind: hint.kind,
                        value: hint.value,
                        hint_confidence: hint.confidence,
                    });
            }
        }

        // Score each candidate
        let mut results = Vec::new();
        for (identity_id, matched_hints) in candidates {
            let identity = self.db.get_identity(&identity_id)?
                .ok_or_else(|| anyhow::anyhow!("Identity {} not found", identity_id))?;

            let score = self.compute_score(&matched_hints);
            let confidence = MatchConfidence::from_score(score);

            results.push(IdentityMatch {
                identity,
                score,
                matched_hints,
                confidence,
            });
        }

        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        Ok(results)
    }

    /// Compute match score from matched hints
    fn compute_score(&self, matched_hints: &[MatchedHint]) -> f64 {
        if matched_hints.is_empty() {
            return 0.0;
        }

        // Strategy: weighted average of hint confidences
        // Weight by hint strength (USB > MIDI > ALSA)
        let mut total_weight = 0.0;
        let mut weighted_sum = 0.0;

        for hint in matched_hints {
            let weight = self.hint_weight(hint.kind);
            weighted_sum += hint.hint_confidence * weight;
            total_weight += weight;
        }

        // Bonus for multiple matching hints (more evidence)
        let multi_hint_bonus = if matched_hints.len() > 1 {
            0.1 * (matched_hints.len() as f64 - 1.0).min(2.0)  // Max +0.2
        } else {
            0.0
        };

        let base_score = weighted_sum / total_weight;
        (base_score + multi_hint_bonus).min(1.0)
    }

    /// Weight hints by strength (higher = more reliable)
    ///
    /// Hierarchy for twin device disambiguation:
    /// 1. UsbSerial (gold standard - unique per device)
    /// 2. UsbPath (topology-based - "Keystep on port 3.2")
    /// 3. UsbDeviceId (VID:PID - same for identical devices!)
    fn hint_weight(&self, kind: HintKind) -> f64 {
        match kind {
            HintKind::UsbSerial => 2.5,           // Gold standard (unique per device)
            HintKind::UsbPath => 2.2,             // Topology fallback for twin devices
            HintKind::UsbDeviceId => 2.0,         // VID:PID (same for identical devices!)
            HintKind::PipewireAlsaPath => 1.5,    // Links PipeWire to ALSA
            HintKind::MidiName => 1.0,            // Medium (can be generic)
            HintKind::PipewireName => 0.9,
            HintKind::AlsaCard => 0.8,            // Weaker (can change)
            HintKind::AlsaHw => 0.5,              // Weakest (changes between boots)
        }
    }

    /// Get best match (highest confidence)
    pub fn best_match(&self, fingerprints: &[DeviceFingerprint]) -> Result<Option<IdentityMatch>> {
        let matches = self.match_device(fingerprints)?;
        Ok(matches.into_iter().next())
    }
}
```

## 🧪 Test Cases (tests/matcher_tests.rs)

```rust
use audio_graph_mcp::{db::Database, matcher::IdentityMatcher, types::*};
use serde_json::json;

#[test]
fn test_exact_match_high_confidence() {
    let db = Database::in_memory().unwrap();

    // Create identity with strong hint
    db.create_identity("jdxi", "Roland JD-Xi", json!({})).unwrap();
    db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();

    // Device with matching USB ID
    let fingerprints = vec![
        DeviceFingerprint {
            kind: HintKind::UsbDeviceId,
            value: "0582:0160".into(),
        },
    ];

    let matcher = IdentityMatcher::new(&db);
    let best = matcher.best_match(&fingerprints).unwrap().unwrap();

    assert_eq!(best.identity.id, "jdxi");
    assert!(best.score >= 0.9);
    assert_eq!(best.confidence, MatchConfidence::High);
}

#[test]
fn test_multiple_hints_boost_score() {
    let db = Database::in_memory().unwrap();

    db.create_identity("jdxi", "Roland JD-Xi", json!({})).unwrap();
    db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();
    db.add_hint("jdxi", HintKind::MidiName, "JD-Xi", 0.9).unwrap();
    db.add_hint("jdxi", HintKind::AlsaCard, "Roland JD-Xi", 0.8).unwrap();

    // Device with ALL matching hints
    let fingerprints = vec![
        DeviceFingerprint { kind: HintKind::UsbDeviceId, value: "0582:0160".into() },
        DeviceFingerprint { kind: HintKind::MidiName, value: "JD-Xi".into() },
        DeviceFingerprint { kind: HintKind::AlsaCard, value: "Roland JD-Xi".into() },
    ];

    let matcher = IdentityMatcher::new(&db);
    let best = matcher.best_match(&fingerprints).unwrap().unwrap();

    // Should have higher score than single-hint match
    assert!(best.score >= 0.95);
    assert_eq!(best.matched_hints.len(), 3);
}

#[test]
fn test_ambiguous_match_medium_confidence() {
    let db = Database::in_memory().unwrap();

    db.create_identity("synth1", "Generic Synth", json!({})).unwrap();
    db.add_hint("synth1", HintKind::MidiName, "USB MIDI", 0.6).unwrap();

    let fingerprints = vec![
        DeviceFingerprint { kind: HintKind::MidiName, value: "USB MIDI".into() },
    ];

    let matcher = IdentityMatcher::new(&db);
    let best = matcher.best_match(&fingerprints).unwrap().unwrap();

    assert_eq!(best.confidence, MatchConfidence::Medium);
    assert!(best.score >= 0.5 && best.score < 0.9);
}

#[test]
fn test_no_match_returns_none() {
    let db = Database::in_memory().unwrap();

    db.create_identity("jdxi", "Roland JD-Xi", json!({})).unwrap();
    db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();

    // Device with non-matching fingerprints
    let fingerprints = vec![
        DeviceFingerprint { kind: HintKind::UsbDeviceId, value: "AAAA:BBBB".into() },
    ];

    let matcher = IdentityMatcher::new(&db);
    let best = matcher.best_match(&fingerprints).unwrap();

    assert!(best.is_none());
}

#[test]
fn test_multiple_candidates_sorted_by_score() {
    let db = Database::in_memory().unwrap();

    // Create two identities with overlapping hints
    db.create_identity("jdxi", "Roland JD-Xi", json!({})).unwrap();
    db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();
    db.add_hint("jdxi", HintKind::MidiName, "JD-Xi", 0.9).unwrap();

    db.create_identity("generic", "Generic MIDI", json!({})).unwrap();
    db.add_hint("generic", HintKind::MidiName, "JD-Xi", 0.5).unwrap();

    let fingerprints = vec![
        DeviceFingerprint { kind: HintKind::UsbDeviceId, value: "0582:0160".into() },
        DeviceFingerprint { kind: HintKind::MidiName, value: "JD-Xi".into() },
    ];

    let matcher = IdentityMatcher::new(&db);
    let matches = matcher.match_device(&fingerprints).unwrap();

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].identity.id, "jdxi");  // Higher score
    assert!(matches[0].score > matches[1].score);
}

#[test]
fn test_twin_devices_distinguished_by_usb_path() {
    let db = Database::in_memory().unwrap();

    // Two identical Keystep Pros - same VID:PID, different USB ports
    db.create_identity("keystep_left", "Keystep Pro (Left)", json!({})).unwrap();
    db.add_hint("keystep_left", HintKind::UsbDeviceId, "1c75:0263", 1.0).unwrap();
    db.add_hint("keystep_left", HintKind::UsbPath, "usb-0000:00:14.0-3.1", 1.0).unwrap();

    db.create_identity("keystep_right", "Keystep Pro (Right)", json!({})).unwrap();
    db.add_hint("keystep_right", HintKind::UsbDeviceId, "1c75:0263", 1.0).unwrap();
    db.add_hint("keystep_right", HintKind::UsbPath, "usb-0000:00:14.0-3.2", 1.0).unwrap();

    // Device on port 3.2 should match "keystep_right"
    let fingerprints = vec![
        DeviceFingerprint { kind: HintKind::UsbDeviceId, value: "1c75:0263".into() },
        DeviceFingerprint { kind: HintKind::UsbPath, value: "usb-0000:00:14.0-3.2".into() },
    ];

    let matcher = IdentityMatcher::new(&db);
    let best = matcher.best_match(&fingerprints).unwrap().unwrap();

    assert_eq!(best.identity.id, "keystep_right");
    assert_eq!(best.confidence, MatchConfidence::High);
}
```

## ⚠️ The Twin Device Problem

**Scenario**: Two identical devices (e.g., two Arturia Keystep Pros).

| Hint Type | Device 1 | Device 2 | Unique? |
|-----------|----------|----------|---------|
| `UsbDeviceId` | 1c75:0263 | 1c75:0263 | ❌ Same |
| `UsbSerial` | (none) | (none) | ❌ Many devices lack serial |
| `UsbPath` | usb-0000:00:14.0-3.1 | usb-0000:00:14.0-3.2 | ✅ Different ports! |

**Solution hierarchy**:
1. **UsbSerial** - If the device has a unique serial, use it (gold standard)
2. **UsbPath** - USB topology distinguishes "Keystep on port 3.1" vs "port 3.2"
3. **UsbDeviceId** - Same for identical devices, but still useful for matching type

**Caveat**: `UsbPath` changes if you move the device to a different USB port. The user must re-bind when rearranging hardware. This is unavoidable without a serial number.

**UnboundDevice exposure**: When showing unbound devices, always include `usb_path` so the user can distinguish twins:
```
UnboundDevice {
    raw_name: "Keystep Pro",
    fingerprints: [
        { kind: "usb_device_id", value: "1c75:0263" },
        { kind: "usb_path", value: "usb-0000:00:14.0-3.2" },  // <-- Show this!
    ],
    ...
}
```

## ✅ Acceptance Criteria

When this task is complete:

1. ✅ `IdentityMatcher::match_device()` returns scored candidates
2. ✅ Single strong hint (USB ID) → High confidence (≥ 0.9)
3. ✅ Multiple matching hints → Boosted score
4. ✅ Weak hints (ALSA hw) → Lower score
5. ✅ No matches → Returns empty list
6. ✅ Ambiguous matches → Sorted by score
7. ✅ `MatchConfidence` enum correctly categorizes scores
8. ✅ **Twin devices** distinguished by `UsbPath` hint
9. ✅ `UsbSerial` has highest weight when available

## 🔍 Database Extension

Add this method to `src/db/hints.rs`:

```rust
/// Find all hints matching (kind, value)
pub fn find_hints_by_kind_value(&self, kind: HintKind, value: &str) -> Result<Vec<IdentityHint>> {
    let mut stmt = self.conn.prepare(
        "SELECT identity_id, hint_kind, hint_value, confidence
         FROM identity_hints
         WHERE hint_kind = ?1 AND hint_value = ?2"
    )?;

    let hints = stmt.query_map(params![kind.as_str(), value], |row| {
        Ok(IdentityHint {
            identity_id: row.get(0)?,
            kind: HintKind::from_str(row.get::<_, String>(1)?.as_str()).unwrap(),
            value: row.get(2)?,
            confidence: row.get(3)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(hints)
}
```

## 💡 Implementation Tips

1. **Start with exact matching**: Ignore fuzzy matching initially (substring, Levenshtein)
2. **Test scoring thoroughly**: Edge cases matter (0 hints, 10 hints, all low confidence)
3. **Log match details**: Debug output showing why a match scored X
4. **Consider tie-breaking**: If two identities have same score, prefer most recently used
5. **Future enhancement**: Substring matching for names ("JD-Xi" matches "Roland JD-Xi MIDI 1")

## 🚧 Out of Scope (for this task)

- ❌ Fuzzy string matching (Levenshtein distance)
- ❌ Machine learning-based matching
- ❌ User confirmation UI
- ❌ Auto-binding based on confidence

Focus ONLY on the scoring algorithm. Task 05 (MCP tools) will handle user interaction.

## 📚 References

- Scoring inspiration: fuzzy file search (fzf), git commit matching
- Confidence thresholds: common ML practice (0.9 = high, 0.5 = threshold)

## 🎬 Next Task

After matching works: **[Task 04: Trustfall GraphQL Adapter](task-04-trustfall-adapter.md)**

We'll integrate the matcher into Trustfall's edge resolution, enabling queries like:
```graphql
{ AlsaMidiDevice { identity { name } } }
```
