//! Python API surface for vibeweaver

use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::json;
use tracing::debug;

use crate::tool_bridge;

/// Read-only beat state
#[pyclass]
#[derive(Debug, Clone)]
pub struct BeatState {
    #[pyo3(get)]
    pub current: f64,
    #[pyo3(get)]
    pub tempo: f64,
}

#[pymethods]
impl BeatState {
    fn __repr__(&self) -> String {
        format!("BeatState(current={}, tempo={})", self.current, self.tempo)
    }
}

/// Read-only transport state
#[pyclass]
#[derive(Debug, Clone)]
pub struct TransportState {
    #[pyo3(get)]
    pub state: String,
    #[pyo3(get)]
    pub position: f64,
}

#[pymethods]
impl TransportState {
    fn __repr__(&self) -> String {
        format!(
            "TransportState(state='{}', position={})",
            self.state, self.position
        )
    }
}

/// Marker info
#[pyclass]
#[derive(Debug, Clone)]
pub struct MarkerInfo {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub beat: f64,
}

#[pymethods]
impl MarkerInfo {
    fn __repr__(&self) -> String {
        format!("Marker('{}' @ beat {})", self.name, self.beat)
    }
}

/// Timeline state access
#[pyclass]
#[derive(Debug, Clone)]
pub struct Timeline {
    #[pyo3(get)]
    pub markers: Vec<MarkerInfo>,
}

#[pymethods]
impl Timeline {
    fn __repr__(&self) -> String {
        format!("Timeline({} markers)", self.markers.len())
    }
}

/// Artifact result from sample()
#[pyclass]
#[derive(Debug, Clone)]
pub struct Artifact {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub content_hash: String,
    #[pyo3(get)]
    pub tags: Vec<String>,
}

#[pymethods]
impl Artifact {
    fn __repr__(&self) -> String {
        format!("Artifact(id='{}', hash='{}')", self.id, self.content_hash)
    }
}

/// Latent reference (placeholder until deadline)
#[pyclass]
#[derive(Debug, Clone)]
pub struct LatentRef {
    #[pyo3(get)]
    pub rule_id: String,
    #[pyo3(get)]
    pub space: String,
    #[pyo3(get)]
    pub deadline: f64,
}

#[pymethods]
impl LatentRef {
    fn __repr__(&self) -> String {
        format!(
            "LatentRef(space='{}', deadline={})",
            self.space, self.deadline
        )
    }

    /// Cancel the latent job
    fn cancel(&self) -> PyResult<()> {
        // TODO: Remove rule from scheduler
        Ok(())
    }

    /// Materialize early (trigger now)
    fn materialize(&self, py: Python<'_>) -> PyResult<PyObject> {
        // TODO: Trigger the rule immediately
        Ok(py.None())
    }
}

/// Session info returned from session()
#[pyclass]
#[derive(Debug, Clone)]
pub struct SessionInfo {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub vibe: Option<String>,
    #[pyo3(get)]
    pub tempo_bpm: f64,
}

#[pymethods]
impl SessionInfo {
    fn __repr__(&self) -> String {
        match &self.vibe {
            Some(v) => format!(
                "Session('{}', vibe='{}', tempo={})",
                self.name, v, self.tempo_bpm
            ),
            None => format!("Session('{}', tempo={})", self.name, self.tempo_bpm),
        }
    }
}

// --- Module functions ---
// These are stubs that will be connected to the actual implementation

/// Create or load a session
#[pyfunction]
#[pyo3(signature = (name=None, vibe=None, load=None))]
pub fn session(
    _py: Python<'_>,
    name: Option<String>,
    vibe: Option<String>,
    load: Option<String>,
) -> PyResult<SessionInfo> {
    // TODO: Connect to actual session management
    let _ = load; // Will be used for loading existing sessions
    let name = name.unwrap_or_else(|| "untitled".to_string());
    Ok(SessionInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        vibe,
        tempo_bpm: 120.0,
    })
}

/// Set session tempo
#[pyfunction]
pub fn tempo(bpm: f64) -> PyResult<()> {
    debug!("Setting tempo to {} BPM", bpm);
    tool_bridge::call_tool("garden_set_tempo", json!({ "bpm": bpm }))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(())
}

/// Generate content from a space (returns awaitable)
#[pyfunction]
#[pyo3(signature = (space, prompt=None, inference=None, tags=None))]
pub fn sample(
    py: Python<'_>,
    space: String,
    prompt: Option<String>,
    inference: Option<Bound<'_, PyDict>>,
    tags: Option<Vec<String>>,
) -> PyResult<PyObject> {
    // TODO: Return actual awaitable
    let _ = (space, prompt, inference, tags);
    Ok(py.None())
}

/// Schedule latent generation with deadline (creates rule)
#[pyfunction]
#[pyo3(signature = (space, prompt, deadline, priority=None))]
pub fn latent(
    space: String,
    prompt: String,
    deadline: f64,
    priority: Option<String>,
) -> PyResult<LatentRef> {
    // TODO: Create actual deadline rule
    let _ = (prompt, priority);
    Ok(LatentRef {
        rule_id: uuid::Uuid::new_v4().to_string(),
        space,
        deadline,
    })
}

/// Schedule content at beat position
#[pyfunction]
#[pyo3(signature = (content, at, duration=None, gain=None))]
pub fn schedule(
    content: PyObject,
    at: f64,
    duration: Option<f64>,
    gain: Option<f64>,
) -> PyResult<()> {
    // TODO: Schedule via hootenanny
    let _ = (content, at, duration, gain);
    Ok(())
}

/// Preview content without scheduling
#[pyfunction]
#[pyo3(signature = (content, duration=None))]
pub fn audition(content: PyObject, duration: Option<f64>) -> PyResult<()> {
    // TODO: Audition via hootenanny
    let _ = (content, duration);
    Ok(())
}

/// Add a marker at beat position
#[pyfunction]
#[pyo3(signature = (name, beat, metadata=None))]
pub fn marker(name: String, beat: f64, metadata: Option<Bound<'_, PyDict>>) -> PyResult<()> {
    // TODO: Add marker to database
    let _ = (name, beat, metadata);
    Ok(())
}

// --- Transport ---

#[pyfunction]
pub fn play() -> PyResult<()> {
    debug!("play()");
    tool_bridge::call_tool("garden_play", json!({}))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(())
}

#[pyfunction]
pub fn pause() -> PyResult<()> {
    debug!("pause()");
    tool_bridge::call_tool("garden_pause", json!({}))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(())
}

#[pyfunction]
pub fn stop() -> PyResult<()> {
    debug!("stop()");
    tool_bridge::call_tool("garden_stop", json!({}))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(())
}

#[pyfunction]
pub fn seek(beat: f64) -> PyResult<()> {
    debug!("seek({})", beat);
    tool_bridge::call_tool("garden_seek", json!({ "beat": beat }))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(())
}

// --- Decorators ---

/// Decorator: fire on beat divisor
#[pyfunction]
pub fn on_beat(py: Python<'_>, _divisor: u32) -> PyResult<PyObject> {
    // TODO: Return decorator that creates Beat trigger rule
    Ok(py.None())
}

/// Decorator: fire on named marker
#[pyfunction]
pub fn on_marker(py: Python<'_>, _name: String) -> PyResult<PyObject> {
    // TODO: Return decorator that creates Marker trigger rule
    Ok(py.None())
}

/// Decorator: fire on artifact creation
#[pyfunction]
#[pyo3(signature = (tag=None))]
pub fn on_artifact(py: Python<'_>, tag: Option<String>) -> PyResult<PyObject> {
    // TODO: Return decorator that creates Artifact trigger rule
    let _ = tag;
    Ok(py.None())
}

// --- Async helpers ---

/// Wait for multiple awaitables
#[pyfunction]
pub fn gather(py: Python<'_>, _futures: Vec<PyObject>) -> PyResult<PyObject> {
    // TODO: Implement gather
    Ok(py.None())
}

/// Register the vibeweaver module
#[pymodule]
pub fn vibeweaver(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Classes
    m.add_class::<BeatState>()?;
    m.add_class::<TransportState>()?;
    m.add_class::<MarkerInfo>()?;
    m.add_class::<Timeline>()?;
    m.add_class::<Artifact>()?;
    m.add_class::<LatentRef>()?;
    m.add_class::<SessionInfo>()?;

    // Functions
    m.add_function(wrap_pyfunction!(session, m)?)?;
    m.add_function(wrap_pyfunction!(tempo, m)?)?;
    m.add_function(wrap_pyfunction!(sample, m)?)?;
    m.add_function(wrap_pyfunction!(latent, m)?)?;
    m.add_function(wrap_pyfunction!(schedule, m)?)?;
    m.add_function(wrap_pyfunction!(audition, m)?)?;
    m.add_function(wrap_pyfunction!(marker, m)?)?;
    m.add_function(wrap_pyfunction!(play, m)?)?;
    m.add_function(wrap_pyfunction!(pause, m)?)?;
    m.add_function(wrap_pyfunction!(stop, m)?)?;
    m.add_function(wrap_pyfunction!(seek, m)?)?;
    m.add_function(wrap_pyfunction!(on_beat, m)?)?;
    m.add_function(wrap_pyfunction!(on_marker, m)?)?;
    m.add_function(wrap_pyfunction!(on_artifact, m)?)?;
    m.add_function(wrap_pyfunction!(gather, m)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_info_repr() {
        let info = SessionInfo {
            id: "test".to_string(),
            name: "mysession".to_string(),
            vibe: Some("dark techno".to_string()),
            tempo_bpm: 130.0,
        };
        let repr = info.__repr__();
        assert!(repr.contains("mysession"));
        assert!(repr.contains("dark techno"));
    }
}
