//! PyO3 Python interpreter with persistent globals

use anyhow::{Context, Result};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Persistent Python interpreter with globals that survive across evals
pub struct Kernel {
    globals: Py<PyDict>,
}

impl Kernel {
    /// Create new interpreter with empty globals
    pub fn new() -> Result<Self> {
        Python::with_gil(|py| -> PyResult<Self> {
            let globals = PyDict::new(py);

            // Import builtins so basic functions work
            let builtins = py.import("builtins")?;
            globals.set_item("__builtins__", builtins)?;

            Ok(Self {
                globals: globals.unbind(),
            })
        })
        .context("Failed to initialize Python kernel")
    }

    /// Evaluate Python expression, returning the result
    pub fn eval(&self, code: &str) -> Result<PyObject> {
        Python::with_gil(|py| {
            let globals = self.globals.bind(py);

            // Use Python's eval via the builtins
            let builtins = py.import("builtins")?;
            let eval_fn = builtins.getattr("eval")?;

            let result = eval_fn
                .call1((code, globals))
                .with_context(|| format!("Failed to eval: {}", code))?;

            Ok(result.unbind())
        })
    }

    /// Execute Python statements (no return value)
    pub fn exec(&self, code: &str) -> Result<()> {
        Python::with_gil(|py| {
            let globals = self.globals.bind(py);

            // Use Python's exec via the builtins
            let builtins = py.import("builtins")?;
            let exec_fn = builtins.getattr("exec")?;

            exec_fn
                .call1((code, globals))
                .with_context(|| format!("Failed to exec: {}", code))?;

            Ok(())
        })
    }

    /// Inject a Rust value into Python globals
    #[allow(deprecated)]
    pub fn inject<T: IntoPy<PyObject>>(&self, name: &str, value: T) -> Result<()> {
        Python::with_gil(|py| {
            let globals = self.globals.bind(py);
            globals
                .set_item(name, value.into_py(py))
                .with_context(|| format!("Failed to inject: {}", name))?;
            Ok(())
        })
    }

    /// Extract a value from Python globals
    pub fn extract<'py, T: FromPyObject<'py>>(&self, py: Python<'py>, name: &str) -> Result<T> {
        let globals = self.globals.bind(py);
        let value = globals
            .get_item(name)?
            .with_context(|| format!("Key not found: {}", name))?;
        value
            .extract()
            .with_context(|| format!("Failed to extract: {}", name))
    }

    /// Get globals dict bound to the GIL
    pub fn globals_bound<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        self.globals.bind(py).clone()
    }

    /// Clear all globals (for reset), preserving builtins
    pub fn clear(&self) -> Result<()> {
        Python::with_gil(|py| {
            let globals = self.globals.bind(py);

            // Save builtins
            let builtins = globals.get_item("__builtins__")?;

            // Clear everything
            globals.clear();

            // Restore builtins
            if let Some(builtins) = builtins {
                globals.set_item("__builtins__", builtins)?;
            } else {
                // Re-import if somehow missing
                let builtins = py.import("builtins")?;
                globals.set_item("__builtins__", builtins)?;
            }

            Ok(())
        })
    }

    /// Execute code and capture stdout/stderr
    pub fn exec_with_capture(&self, code: &str) -> Result<(String, String)> {
        Python::with_gil(|py| {
            let globals = self.globals.bind(py);

            // Set up capture
            let io = py.import("io")?;
            let sys = py.import("sys")?;

            let stdout_capture = io.call_method0("StringIO")?;
            let stderr_capture = io.call_method0("StringIO")?;

            let old_stdout = sys.getattr("stdout")?;
            let old_stderr = sys.getattr("stderr")?;

            sys.setattr("stdout", &stdout_capture)?;
            sys.setattr("stderr", &stderr_capture)?;

            // Execute using Python's exec builtin
            let builtins = py.import("builtins")?;
            let exec_fn = builtins.getattr("exec")?;
            let exec_result = exec_fn.call1((code, &globals));

            // Restore
            sys.setattr("stdout", &old_stdout)?;
            sys.setattr("stderr", &old_stderr)?;

            // Get captured output
            let stdout: String = stdout_capture.call_method0("getvalue")?.extract()?;
            let stderr: String = stderr_capture.call_method0("getvalue")?.extract()?;

            exec_result.with_context(|| format!("Failed to exec: {}", code))?;

            Ok((stdout, stderr))
        })
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new().expect("Failed to create Python kernel")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_simple() {
        let kernel = Kernel::new().unwrap();
        Python::with_gil(|py| {
            let result = kernel.eval("1 + 1").unwrap();
            let value: i64 = result.extract(py).unwrap();
            assert_eq!(value, 2);
        });
    }

    #[test]
    fn test_inject_and_eval() {
        let kernel = Kernel::new().unwrap();
        kernel.inject("x", 42i64).unwrap();
        Python::with_gil(|py| {
            let result = kernel.eval("x * 2").unwrap();
            let value: i64 = result.extract(py).unwrap();
            assert_eq!(value, 84);
        });
    }

    #[test]
    fn test_persistent_globals() {
        let kernel = Kernel::new().unwrap();
        kernel.exec("y = 10").unwrap();
        Python::with_gil(|py| {
            let result = kernel.eval("y + 5").unwrap();
            let value: i64 = result.extract(py).unwrap();
            assert_eq!(value, 15);
        });
    }

    #[test]
    fn test_clear_preserves_builtins() {
        let kernel = Kernel::new().unwrap();
        kernel.inject("z", 100i64).unwrap();
        kernel.clear().unwrap();

        // z should be gone
        let result = kernel.eval("z");
        assert!(result.is_err());

        // But builtins should work
        Python::with_gil(|py| {
            let result = kernel.eval("len([1, 2, 3])").unwrap();
            let value: i64 = result.extract(py).unwrap();
            assert_eq!(value, 3);
        });
    }

    #[test]
    fn test_exec_with_capture() {
        let kernel = Kernel::new().unwrap();
        let (stdout, stderr) = kernel.exec_with_capture("print('hello')").unwrap();
        assert_eq!(stdout.trim(), "hello");
        assert!(stderr.is_empty());
    }
}
