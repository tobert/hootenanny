//! PyO3 Python interpreter with persistent globals

use anyhow::{Context, Result};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::api;

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

            // Ensure user site-packages is in sys.path (for pip install --user packages)
            Self::ensure_user_site_packages(py)?;

            Self::inject_vibeweaver_api(py, &globals)?;

            Ok(Self {
                globals: globals.unbind(),
            })
        })
        .context("Failed to initialize Python kernel")
    }

    /// Add user site-packages to sys.path if it exists
    ///
    /// PyO3's embedded Python doesn't automatically include user site-packages,
    /// so packages installed with `pip install --user` aren't visible.
    /// This uses Python's site module to detect and add the correct path.
    fn ensure_user_site_packages(py: Python<'_>) -> PyResult<()> {
        let site = py.import("site")?;
        let sys = py.import("sys")?;

        // Get user site-packages directory from site module
        let user_site = site.call_method0("getusersitepackages")?;
        let user_site_str: String = user_site.extract()?;

        // Only add if the directory exists
        if std::path::Path::new(&user_site_str).exists() {
            let sys_path = sys.getattr("path")?;
            let path_list: Vec<String> = sys_path.extract()?;

            // Only add if not already in path
            if !path_list.contains(&user_site_str) {
                sys_path.call_method1("append", (&user_site_str,))?;
                tracing::info!("Added user site-packages to sys.path: {}", user_site_str);
            }
        }

        Ok(())
    }

    /// Inject vibeweaver API functions into globals
    fn inject_vibeweaver_api(py: Python<'_>, globals: &Bound<'_, PyDict>) -> PyResult<()> {
        // Create the vibeweaver module
        let module = PyModule::new(py, "vibeweaver")?;
        api::vibeweaver(&module)?;

        // Add module to globals so `import vibeweaver` works
        globals.set_item("vibeweaver", &module)?;

        // Also inject commonly-used functions directly into globals for convenience
        // This allows `session()` instead of `vibeweaver.session()`
        globals.set_item("session", module.getattr("session")?)?;
        globals.set_item("tempo", module.getattr("tempo")?)?;
        globals.set_item("sample", module.getattr("sample")?)?;
        globals.set_item("latent", module.getattr("latent")?)?;
        globals.set_item("schedule", module.getattr("schedule")?)?;
        globals.set_item("audition", module.getattr("audition")?)?;
        globals.set_item("marker", module.getattr("marker")?)?;
        globals.set_item("play", module.getattr("play")?)?;
        globals.set_item("pause", module.getattr("pause")?)?;
        globals.set_item("stop", module.getattr("stop")?)?;
        globals.set_item("seek", module.getattr("seek")?)?;
        globals.set_item("on_beat", module.getattr("on_beat")?)?;
        globals.set_item("on_marker", module.getattr("on_marker")?)?;
        globals.set_item("on_artifact", module.getattr("on_artifact")?)?;
        globals.set_item("gather", module.getattr("gather")?)?;

        Ok(())
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

    /// Clear all globals (for reset), preserving builtins and vibeweaver API
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

            #[allow(clippy::needless_borrow)]
            Self::inject_vibeweaver_api(py, &globals)?;

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

            // Handle execution result with proper traceback
            match exec_result {
                Ok(_) => Ok((stdout, stderr)),
                Err(e) => {
                    // Get the full Python traceback
                    let traceback = e.traceback(py);
                    let tb_str = traceback
                        .map(|tb| {
                            tb.format()
                                .unwrap_or_else(|_| "Failed to format traceback".to_string())
                        })
                        .unwrap_or_default();

                    anyhow::bail!(
                        "Python error: {}\n{}",
                        e,
                        tb_str
                    )
                }
            }
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
