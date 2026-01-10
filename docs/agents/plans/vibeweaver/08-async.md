# 08-async: Async Patterns

**File:** `crates/vibeweaver/src/async_bridge.rs`
**Dependencies:** 05-api
**Unblocks:** None

---

## Task

Bridge Python async with Rust async using pyo3-async-runtimes.

## Deliverables

- `crates/vibeweaver/src/async_bridge.rs`
- Python awaitable wrappers
- `gather()` implementation

## Types

```rust
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use tokio::sync::oneshot;
use std::sync::Arc;
use crate::broadcast::ArtifactInfo;
use anyhow::Result;

/// Create a Python awaitable that resolves when a job completes
pub fn create_job_awaitable(
    py: Python<'_>,
    job_id: String,
    receiver: oneshot::Receiver<ArtifactInfo>,
) -> PyResult<PyObject>;

/// Create a Python awaitable from a Rust future
pub fn future_to_awaitable<F, T>(py: Python<'_>, future: F) -> PyResult<PyObject>
where
    F: std::future::Future<Output = Result<T>> + Send + 'static,
    T: IntoPy<PyObject> + Send + 'static;

/// Implement Python gather() - wait for multiple awaitables
pub fn gather_impl(py: Python<'_>, awaitables: Vec<PyObject>) -> PyResult<PyObject>;

// --- Python-visible result types ---

/// Artifact result returned from sample()
#[pyclass]
pub struct Artifact {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub content_hash: String,
    #[pyo3(get)]
    pub tags: Vec<String>,
}

impl From<ArtifactInfo> for Artifact {
    fn from(info: ArtifactInfo) -> Self;
}

/// Latent reference (placeholder until deadline)
#[pyclass]
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
    /// Cancel the latent job
    fn cancel(&self) -> PyResult<()>;

    /// Materialize early (trigger now)
    fn materialize(&self, py: Python<'_>) -> PyResult<PyObject>;
}
```

## Implementation Notes

### Job Awaitable Pattern

```rust
pub fn create_job_awaitable(
    py: Python<'_>,
    job_id: String,
    receiver: oneshot::Receiver<ArtifactInfo>,
) -> PyResult<PyObject> {
    future_into_py(py, async move {
        let info = receiver.await
            .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Job cancelled"))?;
        Ok(Artifact::from(info))
    })
}
```

### Gather Pattern

```rust
pub fn gather_impl(py: Python<'_>, awaitables: Vec<PyObject>) -> PyResult<PyObject> {
    // Convert Python awaitables to Rust futures
    // Use futures::future::join_all
    // Return list of results
    future_into_py(py, async move {
        let mut results = Vec::new();
        for awaitable in awaitables {
            // Each awaitable needs to be polled...
            // This is complex - may need asyncio integration
        }
        Ok(results)
    })
}
```

### Alternative: asyncio Integration

If `gather()` proves difficult, integrate with Python's asyncio:

```python
# In Python code executed by kernel
import asyncio
results = await asyncio.gather(sample(...), sample(...))
```

This would require:
1. Running an asyncio event loop in the background
2. Registering our awaitables with asyncio

## Dependencies

```toml
pyo3-async-runtimes = { version = "0.23", features = ["tokio-runtime"] }
```

## Definition of Done

```bash
cargo fmt --check -p vibeweaver
cargo clippy -p vibeweaver -- -D warnings
cargo test -p vibeweaver async_bridge::
```

## Acceptance Criteria

- [ ] `await sample(...)` resolves when job completes
- [ ] Cancelled jobs raise Python exception
- [ ] `LatentRef.materialize()` triggers early
- [ ] Either native `gather()` or asyncio integration works
- [ ] No deadlocks between Python GIL and Rust async
