use crate::{Database, DeviceFingerprint, HintKind, Identity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchConfidence {
    High,
    Medium,
    Low,
    None,
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub identity: Identity,
    pub score: f64,
    pub confidence: MatchConfidence,
    pub matched_hints: Vec<(HintKind, String)>,
}

pub struct IdentityMatcher<'a> {
    db: &'a Database,
}

impl<'a> IdentityMatcher<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn best_match(&self, fingerprints: &[DeviceFingerprint]) -> anyhow::Result<Option<MatchResult>> {
        let identities = self.db.list_identities()?;
        let mut best: Option<MatchResult> = None;

        for identity in identities {
            let hints = self.db.get_hints(&identity.id.0)?;
            let mut score = 0.0;
            let mut matched = Vec::new();

            for fp in fingerprints {
                for hint in &hints {
                    if hint.kind == fp.kind && hint.value == fp.value {
                        score += hint.confidence;
                        matched.push((hint.kind, hint.value.clone()));
                    }
                }
            }

            if score > 0.0 {
                let confidence = if score >= 1.0 {
                    MatchConfidence::High
                } else if score >= 0.5 {
                    MatchConfidence::Medium
                } else {
                    MatchConfidence::Low
                };

                let result = MatchResult {
                    identity: identity.clone(),
                    score,
                    confidence,
                    matched_hints: matched,
                };

                if best.as_ref().map_or(true, |b| score > b.score) {
                    best = Some(result);
                }
            }
        }

        Ok(best)
    }

    pub fn find_all_matches(&self, fingerprints: &[DeviceFingerprint]) -> anyhow::Result<Vec<MatchResult>> {
        let identities = self.db.list_identities()?;
        let mut matches = Vec::new();

        for identity in identities {
            let hints = self.db.get_hints(&identity.id.0)?;
            let mut score = 0.0;
            let mut matched = Vec::new();

            for fp in fingerprints {
                for hint in &hints {
                    if hint.kind == fp.kind && hint.value == fp.value {
                        score += hint.confidence;
                        matched.push((hint.kind, hint.value.clone()));
                    }
                }
            }

            if score > 0.0 {
                let confidence = if score >= 1.0 {
                    MatchConfidence::High
                } else if score >= 0.5 {
                    MatchConfidence::Medium
                } else {
                    MatchConfidence::Low
                };

                matches.push(MatchResult {
                    identity: identity.clone(),
                    score,
                    confidence,
                    matched_hints: matched,
                });
            }
        }

        matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();

        db.create_identity("jdxi", "Roland JD-Xi", json!({})).unwrap();
        db.add_hint("jdxi", HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();
        db.add_hint("jdxi", HintKind::MidiName, "JD-Xi", 0.8).unwrap();
        db.add_hint("jdxi", HintKind::AlsaCard, "Roland JD-Xi", 0.6).unwrap();

        db.create_identity("keystep", "Arturia Keystep", json!({})).unwrap();
        db.add_hint("keystep", HintKind::MidiName, "Keystep Pro", 0.9).unwrap();

        db
    }

    #[test]
    fn test_exact_match() {
        let db = setup_db();
        let matcher = IdentityMatcher::new(&db);

        let fingerprints = vec![
            DeviceFingerprint { kind: HintKind::UsbDeviceId, value: "0582:0160".into() },
        ];

        let result = matcher.best_match(&fingerprints).unwrap();
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.identity.id.0, "jdxi");
        assert_eq!(m.confidence, MatchConfidence::High);
    }

    #[test]
    fn test_partial_match() {
        let db = setup_db();
        let matcher = IdentityMatcher::new(&db);

        let fingerprints = vec![
            DeviceFingerprint { kind: HintKind::MidiName, value: "JD-Xi".into() },
        ];

        let result = matcher.best_match(&fingerprints).unwrap();
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.identity.id.0, "jdxi");
        assert_eq!(m.confidence, MatchConfidence::Medium);
    }

    #[test]
    fn test_no_match() {
        let db = setup_db();
        let matcher = IdentityMatcher::new(&db);

        let fingerprints = vec![
            DeviceFingerprint { kind: HintKind::MidiName, value: "Unknown Device".into() },
        ];

        let result = matcher.best_match(&fingerprints).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_hints_combine() {
        let db = setup_db();
        let matcher = IdentityMatcher::new(&db);

        let fingerprints = vec![
            DeviceFingerprint { kind: HintKind::MidiName, value: "JD-Xi".into() },
            DeviceFingerprint { kind: HintKind::AlsaCard, value: "Roland JD-Xi".into() },
        ];

        let result = matcher.best_match(&fingerprints).unwrap();
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.identity.id.0, "jdxi");
        assert_eq!(m.confidence, MatchConfidence::High); // 0.8 + 0.6 = 1.4 >= 1.0
        assert_eq!(m.matched_hints.len(), 2);
    }

    #[test]
    fn test_find_all_matches() {
        let db = setup_db();
        let matcher = IdentityMatcher::new(&db);

        let fingerprints = vec![
            DeviceFingerprint { kind: HintKind::MidiName, value: "JD-Xi".into() },
        ];

        let matches = matcher.find_all_matches(&fingerprints).unwrap();
        assert_eq!(matches.len(), 1);
    }
}
