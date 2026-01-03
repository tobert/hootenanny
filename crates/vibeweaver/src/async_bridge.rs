//! Async bridge between Python and Rust
//!
//! Provides a JobFuture class that can be awaited in Python async code,
//! or polled synchronously via .result() for non-async contexts.

use pyo3::prelude::*;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use crate::state::ArtifactInfo;

/// Shared state for job completion
struct JobState {
    receiver: Option<oneshot::Receiver<ArtifactInfo>>,
    result: Option<Result<ArtifactInfo, String>>,
}

/// Python-visible job future that can be awaited or polled
#[pyclass]
pub struct JobFuture {
    job_id: String,
    state: Arc<Mutex<JobState>>,
}

#[pymethods]
impl JobFuture {
    /// Get job ID
    #[getter]
    fn job_id(&self) -> &str {
        &self.job_id
    }

    /// Check if result is ready (non-blocking)
    fn ready(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.result.is_some()
    }

    /// Poll for result (blocking with timeout in seconds)
    #[pyo3(signature = (timeout=None))]
    fn poll(&self, timeout: Option<f64>) -> PyResult<Option<Artifact>> {
        let timeout_ms = timeout.map(|t| (t * 1000.0) as u64).unwrap_or(100);

        // Try to receive with timeout
        let mut state = self.state.lock().unwrap();

        // Already have result?
        if let Some(ref result) = state.result {
            return match result {
                Ok(info) => Ok(Some(Artifact::from(info.clone()))),
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.clone())),
            };
        }

        // Try to receive
        if let Some(mut receiver) = state.receiver.take() {
            // Use try_recv for non-blocking check, or spawn thread for timeout
            match receiver.try_recv() {
                Ok(info) => {
                    state.result = Some(Ok(info.clone()));
                    return Ok(Some(Artifact::from(info)));
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    // Put receiver back and wait with timeout
                    state.receiver = Some(receiver);

                    // For blocking wait, we need to drop the lock and use a thread
                    drop(state);

                    let state_clone = Arc::clone(&self.state);
                    let _handle = std::thread::spawn(move || {
                        let mut state = state_clone.lock().unwrap();
                        if let Some(rx) = state.receiver.take() {
                            // Block on receiving (in the spawned thread)
                            match rx.blocking_recv() {
                                Ok(info) => {
                                    state.result = Some(Ok(info));
                                }
                                Err(_) => {
                                    state.result = Some(Err("Job cancelled".to_string()));
                                }
                            }
                        }
                    });

                    // Wait for thread with timeout
                    std::thread::sleep(std::time::Duration::from_millis(timeout_ms));

                    // Check if we got a result
                    let state = self.state.lock().unwrap();
                    if let Some(ref result) = state.result {
                        match result {
                            Ok(info) => return Ok(Some(Artifact::from(info.clone()))),
                            Err(e) => {
                                return Err(pyo3::exceptions::PyRuntimeError::new_err(e.clone()))
                            }
                        }
                    }

                    // Not ready yet
                    return Ok(None);
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    state.result = Some(Err("Job cancelled".to_string()));
                    return Err(pyo3::exceptions::PyRuntimeError::new_err("Job cancelled"));
                }
            }
        }

        // No receiver and no result - shouldn't happen
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Job in invalid state",
        ))
    }

    /// Block until result is ready (for sync usage)
    #[pyo3(signature = (timeout=None))]
    fn result(&self, py: Python<'_>, timeout: Option<f64>) -> PyResult<Artifact> {
        let timeout_secs = timeout.unwrap_or(60.0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(timeout_secs);

        // Release GIL while polling
        py.allow_threads(|| {
            loop {
                // Check if we have a result
                {
                    let mut state = self.state.lock().unwrap();
                    if let Some(ref result) = state.result {
                        return match result {
                            Ok(info) => Ok(Artifact::from(info.clone())),
                            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.clone())),
                        };
                    }

                    // Try to receive
                    if let Some(mut receiver) = state.receiver.take() {
                        match receiver.try_recv() {
                            Ok(info) => {
                                state.result = Some(Ok(info.clone()));
                                return Ok(Artifact::from(info));
                            }
                            Err(oneshot::error::TryRecvError::Empty) => {
                                state.receiver = Some(receiver);
                            }
                            Err(oneshot::error::TryRecvError::Closed) => {
                                state.result = Some(Err("Job cancelled".to_string()));
                                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                                    "Job cancelled",
                                ));
                            }
                        }
                    }
                }

                if std::time::Instant::now() > deadline {
                    return Err(pyo3::exceptions::PyTimeoutError::new_err("Job timed out"));
                }

                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })
    }

    /// Make this class awaitable - returns self for __await__
    fn __await__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Iterator protocol for await - polls until complete
    fn __next__(&self, _py: Python<'_>) -> PyResult<Option<Artifact>> {
        // Check if result ready
        let state = self.state.lock().unwrap();
        if let Some(ref result) = state.result {
            return match result {
                Ok(info) => Err(pyo3::exceptions::PyStopIteration::new_err(Artifact::from(
                    info.clone(),
                ))),
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.clone())),
            };
        }
        drop(state);

        // Poll briefly and return None to continue iteration
        match self.poll(Some(0.01))? {
            Some(artifact) => Err(pyo3::exceptions::PyStopIteration::new_err(artifact)),
            None => Ok(None), // Continue iterating
        }
    }

    fn __repr__(&self) -> String {
        let ready = self.ready();
        format!("JobFuture(job_id='{}', ready={})", self.job_id, ready)
    }
}

/// Create a Python awaitable that resolves when a job completes
pub fn create_job_awaitable(
    py: Python<'_>,
    job_id: String,
    receiver: oneshot::Receiver<ArtifactInfo>,
) -> PyResult<Bound<'_, PyAny>> {
    let state = Arc::new(Mutex::new(JobState {
        receiver: Some(receiver),
        result: None,
    }));

    let future = JobFuture { job_id, state };
    Ok(Py::new(py, future)?.into_bound(py).into_any())
}

/// Python-visible artifact result
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

impl From<ArtifactInfo> for Artifact {
    fn from(info: ArtifactInfo) -> Self {
        Self {
            id: info.id,
            content_hash: info.content_hash,
            tags: info.tags,
        }
    }
}

/// Gather implementation - wait for multiple awaitables
///
/// This delegates to Python's asyncio.gather for proper coroutine handling.
#[pyfunction]
pub fn gather_impl(py: Python<'_>, awaitables: Vec<PyObject>) -> PyResult<Bound<'_, PyAny>> {
    // Get asyncio module
    let asyncio = py.import("asyncio")?;

    // Call asyncio.gather with our awaitables
    let gather = asyncio.getattr("gather")?;
    gather.call1((awaitables,))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifact_from_info() {
        use chrono::Utc;

        let info = ArtifactInfo {
            id: "art_123".to_string(),
            content_hash: "abc123".to_string(),
            tags: vec!["drums".to_string()],
            created_at: Utc::now(),
        };

        let artifact = Artifact::from(info);
        assert_eq!(artifact.id, "art_123");
        assert_eq!(artifact.content_hash, "abc123");
    }
}
