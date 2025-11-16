//! The musical context system, providing shared knowledge for agents.

use crate::domain::EmotionalVector;
use resonode::{Chord, Key, MusicalTime, Scale, Tempo, TimeSignature};
use rmcp::schemars;
use std::collections::BTreeMap;

/// A map of changes to a value over musical time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct TimeMap<T> {
    changes: BTreeMap<MusicalTime, T>,
}

impl<T: Clone> TimeMap<T> {
    pub fn new() -> Self {
        Self {
            changes: BTreeMap::new(),
        }
    }

    /// Get the value at a specific time.
    pub fn at(&self, time: &MusicalTime) -> Option<&T> {
        self.changes.range(..=time).next_back().map(|(_, v)| v)
    }

    /// Set the value at a specific time.
    pub fn set(&mut self, time: MusicalTime, value: T) {
        self.changes.insert(time, value);
    }

    /// Get all changes within a time range.
    pub fn range(&self, start: &MusicalTime, end: &MusicalTime) -> impl Iterator<Item = (&MusicalTime, &T)> {
        self.changes.range(start..=end)
    }
}

/// The musical context, providing shared knowledge for agents.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, rmcp::schemars::JsonSchema)]
pub struct MusicalContext {
    /// Temporal maps (things change over time)
    pub tempo_map: TimeMap<Tempo>,
    pub key_map: TimeMap<Key>,
    pub time_signature_map: TimeMap<TimeSignature>,
    pub chord_progression: TimeMap<Chord>,

    /// Current state
    pub emotional_state: EmotionalVector,
    pub energy_level: f32,
    pub complexity: f32,

    /// Constraints
    pub scale_constraints: Option<Scale>,
    // pub rhythm_constraints: Option<RhythmPattern>, // RhythmPattern not defined yet
}

impl MusicalContext {
    pub fn new() -> Self {
        Self {
            tempo_map: TimeMap::new(),
            key_map: TimeMap::new(),
            time_signature_map: TimeMap::new(),
            chord_progression: TimeMap::new(),
            emotional_state: EmotionalVector::neutral(),
            energy_level: 0.5,
            complexity: 0.5,
            scale_constraints: None,
            // rhythm_constraints: None,
        }
    }

    pub fn current_key(&self, at_time: &MusicalTime) -> Option<&Key> {
        self.key_map.at(at_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use resonode::Tempo;

    #[test]
    fn test_time_map() {
        let mut time_map = TimeMap::new();
        time_map.set(MusicalTime::new(0, 1, 0), Tempo(120.0));
        time_map.set(MusicalTime::new(4, 1, 0), Tempo(140.0));

        assert_eq!(time_map.at(&MusicalTime::new(0, 1, 0)).unwrap().0, 120.0);
        assert_eq!(time_map.at(&MusicalTime::new(2, 1, 0)).unwrap().0, 120.0);
        assert_eq!(time_map.at(&MusicalTime::new(4, 1, 0)).unwrap().0, 140.0);
        assert_eq!(time_map.at(&MusicalTime::new(8, 1, 0)).unwrap().0, 140.0);
    }
}
