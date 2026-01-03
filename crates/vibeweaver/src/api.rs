//! Python API surface for vibeweaver

use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::json;
use tracing::debug;

use crate::async_bridge::{create_job_awaitable, Artifact as AsyncArtifact, JobFuture};
use crate::broadcast::BroadcastHandler;
use crate::callbacks::CallbackRegistry;
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
///
/// Usage:
/// ```python
/// artifact = await sample("orpheus_loops", temperature=0.9)
/// schedule(artifact, at=timeline_end)
/// ```
#[pyfunction]
#[pyo3(signature = (space, prompt=None, inference=None, tags=None))]
pub fn sample<'py>(
    py: Python<'py>,
    space: String,
    prompt: Option<String>,
    inference: Option<Bound<'py, PyDict>>,
    tags: Option<Vec<String>>,
) -> PyResult<Bound<'py, PyAny>> {
    debug!("sample({}, prompt={:?})", space, prompt);

    // Build args JSON
    let mut args = json!({ "space": space });

    if let Some(p) = prompt {
        args["prompt"] = json!(p);
    }

    if let Some(t) = tags {
        args["tags"] = json!(t);
    }

    // Extract inference parameters from PyDict
    if let Some(ref inf) = inference {
        if let Ok(Some(temp)) = inf.get_item("temperature") {
            if let Ok(t) = temp.extract::<f64>() {
                args["temperature"] = json!(t);
            }
        }
        if let Ok(Some(top_p)) = inf.get_item("top_p") {
            if let Ok(p) = top_p.extract::<f64>() {
                args["top_p"] = json!(p);
            }
        }
        if let Ok(Some(top_k)) = inf.get_item("top_k") {
            if let Ok(k) = top_k.extract::<u32>() {
                args["top_k"] = json!(k);
            }
        }
        if let Ok(Some(max_tokens)) = inf.get_item("max_tokens") {
            if let Ok(m) = max_tokens.extract::<u32>() {
                args["max_tokens"] = json!(m);
            }
        }
    }

    // Call tool_bridge to start the job
    let result = tool_bridge::call_tool("sample", args)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

    // Extract job_id from response
    // Response structure: { "kind": "success", "response": { "type": "job_started", "job_id": "..." } }
    let job_id = result
        .get("response")
        .and_then(|r| r.get("job_id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "sample response missing job_id: {:?}",
                result
            ))
        })?
        .to_string();

    debug!("sample started job: {}", job_id);

    // Get broadcast handler to register waiter
    let handler = BroadcastHandler::global().ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Broadcast handler not initialized")
    })?;

    // Get runtime handle for async operations
    let handle = tokio::runtime::Handle::try_current().map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
            "No tokio runtime available: {}",
            e
        ))
    })?;

    // Register waiter and get receiver - use spawn_blocking to avoid block_in_place issues
    let handler_ref = handler;
    let job_id_clone = job_id.clone();
    let receiver = std::thread::spawn(move || {
        handle.block_on(handler_ref.wait_for_job(job_id_clone))
    })
    .join()
    .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to register job waiter"))?;

    // Return Python awaitable
    create_job_awaitable(py, job_id, receiver)
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
///
/// Usage:
/// ```python
/// artifact = await sample("orpheus_loops")
/// schedule(artifact, at=16.0, gain=0.8)
/// ```
#[pyfunction]
#[pyo3(signature = (content, at, duration=None, gain=None))]
pub fn schedule(
    py: Python<'_>,
    content: PyObject,
    at: f64,
    duration: Option<f64>,
    gain: Option<f64>,
) -> PyResult<()> {
    debug!("schedule(at={}, duration={:?}, gain={:?})", at, duration, gain);

    // Extract artifact_id from content
    // Content can be an Artifact object or a string artifact_id
    let artifact_id: String = if let Ok(id) = content.extract::<String>(py) {
        id
    } else if let Ok(artifact) = content.extract::<AsyncArtifact>(py) {
        artifact.id
    } else {
        // Try to get .id attribute
        content
            .getattr(py, "id")
            .and_then(|id| id.extract::<String>(py))
            .map_err(|_| {
                PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                    "content must be an Artifact or artifact_id string",
                )
            })?
    };

    debug!("schedule artifact_id={}", artifact_id);

    // Build args JSON
    let mut args = json!({
        "encoding": {
            "type": "audio",
            "artifact_id": artifact_id
        },
        "at": at
    });

    if let Some(d) = duration {
        args["duration"] = json!(d);
    }

    if let Some(g) = gain {
        args["gain"] = json!(g);
    }

    // Call tool_bridge to schedule
    tool_bridge::call_tool("schedule", args)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

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

/// Decorator class for beat callbacks
#[pyclass]
#[derive(Debug, Clone)]
pub struct BeatDecorator {
    divisor: u32,
}

#[pymethods]
impl BeatDecorator {
    /// Called when decorator is applied to a function
    fn __call__(&self, py: Python<'_>, func: PyObject) -> PyResult<PyObject> {
        // Register the callback
        let registry = CallbackRegistry::global();
        let mut registry_guard = registry
            .write()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let func_py: Py<PyAny> = func.clone_ref(py).into();
        let callback_id = registry_guard.register_beat(self.divisor, func_py);

        debug!(
            "Registered beat callback: divisor={}, id={}",
            self.divisor, callback_id
        );

        // Return the original function unchanged (so it can still be called normally)
        Ok(func)
    }

    fn __repr__(&self) -> String {
        format!("BeatDecorator(divisor={})", self.divisor)
    }
}

/// Decorator: fire on beat divisor
///
/// Usage:
/// ```python
/// @on_beat(16)
/// def my_callback(beat):
///     print(f"Beat {beat}")
/// ```
#[pyfunction]
pub fn on_beat(_py: Python<'_>, divisor: u32) -> PyResult<BeatDecorator> {
    Ok(BeatDecorator { divisor })
}

/// Decorator class for marker callbacks
#[pyclass]
#[derive(Debug, Clone)]
pub struct MarkerDecorator {
    name: String,
}

#[pymethods]
impl MarkerDecorator {
    fn __call__(&self, py: Python<'_>, func: PyObject) -> PyResult<PyObject> {
        let registry = CallbackRegistry::global();
        let mut registry_guard = registry
            .write()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let func_py: Py<PyAny> = func.clone_ref(py).into();
        registry_guard.register_marker(self.name.clone(), func_py);

        Ok(func)
    }

    fn __repr__(&self) -> String {
        format!("MarkerDecorator(name='{}')", self.name)
    }
}

/// Decorator: fire on named marker
#[pyfunction]
pub fn on_marker(_py: Python<'_>, name: String) -> PyResult<MarkerDecorator> {
    Ok(MarkerDecorator { name })
}

/// Decorator class for artifact callbacks
#[pyclass]
#[derive(Debug, Clone)]
pub struct ArtifactDecorator {
    tag: Option<String>,
}

#[pymethods]
impl ArtifactDecorator {
    fn __call__(&self, py: Python<'_>, func: PyObject) -> PyResult<PyObject> {
        let registry = CallbackRegistry::global();
        let mut registry_guard = registry
            .write()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        let func_py: Py<PyAny> = func.clone_ref(py).into();
        registry_guard.register_artifact(self.tag.clone(), func_py);

        Ok(func)
    }

    fn __repr__(&self) -> String {
        match &self.tag {
            Some(t) => format!("ArtifactDecorator(tag='{}')", t),
            None => "ArtifactDecorator(tag=None)".to_string(),
        }
    }
}

/// Decorator: fire on artifact creation
#[pyfunction]
#[pyo3(signature = (tag=None))]
pub fn on_artifact(_py: Python<'_>, tag: Option<String>) -> PyResult<ArtifactDecorator> {
    Ok(ArtifactDecorator { tag })
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

    // Decorator classes (for type checking/introspection)
    m.add_class::<BeatDecorator>()?;
    m.add_class::<MarkerDecorator>()?;
    m.add_class::<ArtifactDecorator>()?;

    // Async classes
    m.add_class::<JobFuture>()?;

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
