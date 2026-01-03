//! Callback registry for Python beat/marker/artifact callbacks
//!
//! Stores Python callable objects that fire when triggers match.

use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use tracing::{debug, error, info, warn};

/// Global callback registry
static REGISTRY: OnceLock<Arc<RwLock<CallbackRegistry>>> = OnceLock::new();

/// Type of callback trigger
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallbackType {
    Beat,
    Marker,
    Artifact,
}

/// A registered callback
#[derive(Debug)]
pub struct Callback {
    pub id: String,
    pub callback_type: CallbackType,
    /// For Beat: the divisor; for others: unused
    pub divisor: u32,
    /// For Marker: the name; for Artifact: the tag filter
    pub name: Option<String>,
    /// The Python callable (stored as Py<PyAny> for thread-safety)
    pub func: Py<PyAny>,
}

/// Registry of callbacks by type
#[derive(Default)]
pub struct CallbackRegistry {
    /// Beat callbacks by divisor
    beat_callbacks: HashMap<u32, Vec<Callback>>,
    /// Marker callbacks by name
    marker_callbacks: HashMap<String, Vec<Callback>>,
    /// Artifact callbacks (None key = all artifacts)
    artifact_callbacks: Vec<Callback>,
}

impl CallbackRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the global registry, initializing if needed
    pub fn global() -> Arc<RwLock<CallbackRegistry>> {
        REGISTRY
            .get_or_init(|| Arc::new(RwLock::new(CallbackRegistry::new())))
            .clone()
    }

    /// Register a beat callback
    pub fn register_beat(&mut self, divisor: u32, func: Py<PyAny>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let callback = Callback {
            id: id.clone(),
            callback_type: CallbackType::Beat,
            divisor,
            name: None,
            func,
        };
        self.beat_callbacks
            .entry(divisor)
            .or_default()
            .push(callback);
        info!("Registered beat callback: divisor={}, id={}", divisor, id);
        id
    }

    /// Register a marker callback
    pub fn register_marker(&mut self, name: String, func: Py<PyAny>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let callback = Callback {
            id: id.clone(),
            callback_type: CallbackType::Marker,
            divisor: 0,
            name: Some(name.clone()),
            func,
        };
        self.marker_callbacks
            .entry(name.clone())
            .or_default()
            .push(callback);
        info!("Registered marker callback: name={}, id={}", name, id);
        id
    }

    /// Register an artifact callback
    pub fn register_artifact(&mut self, tag: Option<String>, func: Py<PyAny>) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let callback = Callback {
            id: id.clone(),
            callback_type: CallbackType::Artifact,
            divisor: 0,
            name: tag.clone(),
            func,
        };
        self.artifact_callbacks.push(callback);
        info!("Registered artifact callback: tag={:?}, id={}", tag, id);
        id
    }

    /// Get all beat callbacks that should fire for a given beat
    pub fn get_beat_callbacks(&self, beat: f64) -> Vec<&Callback> {
        let beat_int = beat.floor() as u32;
        let mut result = Vec::new();

        for (divisor, callbacks) in &self.beat_callbacks {
            if *divisor > 0 && beat_int % divisor == 0 {
                // Also check we're near an integer beat (not mid-beat)
                if (beat - beat.floor()).abs() < 0.1 {
                    result.extend(callbacks.iter());
                }
            }
        }

        result
    }

    /// Get marker callbacks for a given marker name
    pub fn get_marker_callbacks(&self, name: &str) -> Vec<&Callback> {
        self.marker_callbacks
            .get(name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Get artifact callbacks that match tags
    pub fn get_artifact_callbacks(&self, tags: &[String]) -> Vec<&Callback> {
        self.artifact_callbacks
            .iter()
            .filter(|cb| {
                match &cb.name {
                    Some(tag_filter) => tags.iter().any(|t| t == tag_filter),
                    None => true, // No filter = match all
                }
            })
            .collect()
    }

    /// Remove a callback by ID
    pub fn remove(&mut self, id: &str) -> bool {
        // Check beat callbacks
        for callbacks in self.beat_callbacks.values_mut() {
            if let Some(pos) = callbacks.iter().position(|c| c.id == id) {
                callbacks.remove(pos);
                info!("Removed beat callback: id={}", id);
                return true;
            }
        }

        // Check marker callbacks
        for callbacks in self.marker_callbacks.values_mut() {
            if let Some(pos) = callbacks.iter().position(|c| c.id == id) {
                callbacks.remove(pos);
                info!("Removed marker callback: id={}", id);
                return true;
            }
        }

        // Check artifact callbacks
        if let Some(pos) = self.artifact_callbacks.iter().position(|c| c.id == id) {
            self.artifact_callbacks.remove(pos);
            info!("Removed artifact callback: id={}", id);
            return true;
        }

        false
    }

    /// Clear all callbacks
    pub fn clear(&mut self) {
        self.beat_callbacks.clear();
        self.marker_callbacks.clear();
        self.artifact_callbacks.clear();
        info!("Cleared all callbacks");
    }

    /// Get callback counts for debugging
    pub fn counts(&self) -> (usize, usize, usize) {
        let beat_count: usize = self.beat_callbacks.values().map(|v| v.len()).sum();
        let marker_count: usize = self.marker_callbacks.values().map(|v| v.len()).sum();
        let artifact_count = self.artifact_callbacks.len();
        (beat_count, marker_count, artifact_count)
    }
}

/// Fire callbacks for a beat tick
pub fn fire_beat_callbacks(beat: f64) {
    let registry = CallbackRegistry::global();
    let registry_guard = match registry.read() {
        Ok(g) => g,
        Err(e) => {
            error!("Failed to lock callback registry: {}", e);
            return;
        }
    };

    let callbacks = registry_guard.get_beat_callbacks(beat);
    if callbacks.is_empty() {
        return;
    }

    debug!("Firing {} beat callbacks for beat {}", callbacks.len(), beat);

    Python::with_gil(|py| {
        for callback in callbacks {
            match callback.func.call1(py, (beat,)) {
                Ok(_) => debug!("Beat callback {} fired successfully", callback.id),
                Err(e) => {
                    warn!("Beat callback {} failed: {}", callback.id, e);
                    e.print(py);
                }
            }
        }
    });
}

/// Fire callbacks for a marker
pub fn fire_marker_callbacks(name: &str, beat: f64) {
    let registry = CallbackRegistry::global();
    let registry_guard = match registry.read() {
        Ok(g) => g,
        Err(e) => {
            error!("Failed to lock callback registry: {}", e);
            return;
        }
    };

    let callbacks = registry_guard.get_marker_callbacks(name);
    if callbacks.is_empty() {
        return;
    }

    debug!(
        "Firing {} marker callbacks for '{}' at beat {}",
        callbacks.len(),
        name,
        beat
    );

    Python::with_gil(|py| {
        for callback in callbacks {
            match callback.func.call1(py, (name, beat)) {
                Ok(_) => debug!("Marker callback {} fired successfully", callback.id),
                Err(e) => {
                    warn!("Marker callback {} failed: {}", callback.id, e);
                    e.print(py);
                }
            }
        }
    });
}

/// Fire callbacks for an artifact creation
pub fn fire_artifact_callbacks(artifact_id: &str, content_hash: &str, tags: &[String]) {
    let registry = CallbackRegistry::global();
    let registry_guard = match registry.read() {
        Ok(g) => g,
        Err(e) => {
            error!("Failed to lock callback registry: {}", e);
            return;
        }
    };

    let callbacks = registry_guard.get_artifact_callbacks(tags);
    if callbacks.is_empty() {
        return;
    }

    debug!(
        "Firing {} artifact callbacks for {}",
        callbacks.len(),
        artifact_id
    );

    Python::with_gil(|py| {
        for callback in callbacks {
            // Create artifact info dict
            let artifact_info = pyo3::types::PyDict::new(py);
            let _ = artifact_info.set_item("id", artifact_id);
            let _ = artifact_info.set_item("content_hash", content_hash);
            let _ = artifact_info.set_item("tags", tags.to_vec());

            match callback.func.call1(py, (artifact_info,)) {
                Ok(_) => debug!("Artifact callback {} fired successfully", callback.id),
                Err(e) => {
                    warn!("Artifact callback {} failed: {}", callback.id, e);
                    e.print(py);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_matching() {
        let registry = CallbackRegistry::new();

        // We can't easily test with real Python callbacks without Python runtime
        // but we can test the matching logic

        // Test get_beat_callbacks returns empty for no registrations
        assert!(registry.get_beat_callbacks(0.0).is_empty());
        assert!(registry.get_beat_callbacks(4.0).is_empty());
    }

    #[test]
    fn test_callback_counts() {
        let registry = CallbackRegistry::new();
        assert_eq!(registry.counts(), (0, 0, 0));
    }
}
