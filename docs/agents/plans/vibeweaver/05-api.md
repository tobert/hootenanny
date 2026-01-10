# 05-api: Python API

**File:** `crates/vibeweaver/src/api.rs`
**Dependencies:** 01-kernel, 02-session
**Unblocks:** 07-mcp, 08-async

---

## Task

Create Python-callable functions that register as the `vibeweaver` module.

## Deliverables

- `crates/vibeweaver/src/api.rs`
- PyO3 module registration
- Integration tests

## Types

```rust
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

// --- Module registration ---

#[pymodule]
fn vibeweaver(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(session, m)?)?;
    m.add_function(wrap_pyfunction!(tempo, m)?)?;
    m.add_function(wrap_pyfunction!(sample, m)?)?;
    m.add_function(wrap_pyfunction!(latent, m)?)?;
    m.add_function(wrap_pyfunction!(schedule, m)?)?;
    m.add_function(wrap_pyfunction!(audition, m)?)?;
    m.add_function(wrap_pyfunction!(play, m)?)?;
    m.add_function(wrap_pyfunction!(pause, m)?)?;
    m.add_function(wrap_pyfunction!(stop, m)?)?;
    m.add_function(wrap_pyfunction!(seek, m)?)?;
    m.add_function(wrap_pyfunction!(on_beat, m)?)?;
    m.add_function(wrap_pyfunction!(on_marker, m)?)?;
    m.add_function(wrap_pyfunction!(on_artifact, m)?)?;
    m.add_function(wrap_pyfunction!(marker, m)?)?;
    m.add_function(wrap_pyfunction!(gather, m)?)?;
    Ok(())
}

// --- Session ---

/// Create or load a session
#[pyfunction]
#[pyo3(signature = (name=None, vibe=None, load=None))]
fn session(
    py: Python<'_>,
    name: Option<String>,
    vibe: Option<String>,
    load: Option<String>,
) -> PyResult<PyObject>;

/// Set session tempo
#[pyfunction]
fn tempo(bpm: f64) -> PyResult<()>;

// --- Generation ---

/// Generate content from a space (returns awaitable)
#[pyfunction]
#[pyo3(signature = (space, prompt=None, inference=None, tags=None))]
fn sample(
    py: Python<'_>,
    space: String,
    prompt: Option<String>,
    inference: Option<Bound<'_, PyDict>>,
    tags: Option<Vec<String>>,
) -> PyResult<PyObject>;

/// Schedule latent generation with deadline (creates rule)
#[pyfunction]
#[pyo3(signature = (space, prompt, deadline, priority=None))]
fn latent(
    space: String,
    prompt: String,
    deadline: f64,
    priority: Option<String>,
) -> PyResult<PyObject>;

// --- Timeline ---

/// Schedule content at beat position
#[pyfunction]
#[pyo3(signature = (content, at, duration=None, gain=None))]
fn schedule(
    content: PyObject,
    at: f64,
    duration: Option<f64>,
    gain: Option<f64>,
) -> PyResult<()>;

/// Preview content without scheduling
#[pyfunction]
#[pyo3(signature = (content, duration=None))]
fn audition(content: PyObject, duration: Option<f64>) -> PyResult<()>;

/// Add a marker at beat position
#[pyfunction]
#[pyo3(signature = (name, beat, metadata=None))]
fn marker(name: String, beat: f64, metadata: Option<Bound<'_, PyDict>>) -> PyResult<()>;

// --- Transport ---

#[pyfunction]
fn play() -> PyResult<()>;

#[pyfunction]
fn pause() -> PyResult<()>;

#[pyfunction]
fn stop() -> PyResult<()>;

#[pyfunction]
fn seek(beat: f64) -> PyResult<()>;

// --- Decorators (return decorator functions) ---

/// Decorator: fire on beat divisor
#[pyfunction]
fn on_beat(py: Python<'_>, divisor: u32) -> PyResult<PyObject>;

/// Decorator: fire on named marker
#[pyfunction]
fn on_marker(py: Python<'_>, name: String) -> PyResult<PyObject>;

/// Decorator: fire on artifact creation
#[pyfunction]
#[pyo3(signature = (tag=None))]
fn on_artifact(py: Python<'_>, tag: Option<String>) -> PyResult<PyObject>;

// --- Async helpers ---

/// Wait for multiple awaitables
#[pyfunction]
fn gather(py: Python<'_>, futures: Vec<PyObject>) -> PyResult<PyObject>;

// --- Python classes for state access ---

/// Read-only beat state
#[pyclass]
pub struct BeatState {
    #[pyo3(get)]
    current: f64,
    #[pyo3(get)]
    tempo: f64,
}

/// Read-only transport state
#[pyclass]
pub struct TransportState {
    #[pyo3(get)]
    state: String,
    #[pyo3(get)]
    position: f64,
}

/// Marker info
#[pyclass]
pub struct MarkerInfo {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    beat: f64,
}

/// Timeline state access
#[pyclass]
pub struct Timeline {
    #[pyo3(get)]
    markers: Vec<MarkerInfo>,
}
```

## Implementation Notes

- Functions access shared state via thread-local or context
- `sample()` returns a Python awaitable (see 08-async)
- Decorators create Rule entries in the database
- State classes are injected into Python globals

## Definition of Done

```bash
cargo fmt --check -p vibeweaver
cargo clippy -p vibeweaver -- -D warnings
cargo test -p vibeweaver api::
```

## Acceptance Criteria

- [ ] `session()` creates/loads session
- [ ] `sample()` returns awaitable
- [ ] `schedule()` calls through to hootenanny
- [ ] `@on_beat(4)` decorator creates Beat trigger rule
- [ ] State classes readable from Python
