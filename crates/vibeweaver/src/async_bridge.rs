//! Async bridge between Python and Rust

use pyo3::prelude::*;
use tokio::sync::oneshot;

use crate::state::ArtifactInfo;

/// Create a Python awaitable that resolves when a job completes
pub fn create_job_awaitable(
    py: Python<'_>,
    _job_id: String,
    receiver: oneshot::Receiver<ArtifactInfo>,
) -> PyResult<Bound<'_, PyAny>> {
    // Use pyo3-async-runtimes to convert Rust future to Python awaitable
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let info = receiver
            .await
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Job cancelled"))?;

        Ok(Artifact {
            id: info.id,
            content_hash: info.content_hash,
            tags: info.tags,
        })
    })
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
