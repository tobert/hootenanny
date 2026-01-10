# 01-kernel: PyO3 Interpreter

**File:** `crates/vibeweaver/src/kernel.rs`
**Dependencies:** None
**Unblocks:** 05-api

---

## Task

Create the Python interpreter wrapper that manages persistent globals across evaluations.

## Deliverables

- `crates/vibeweaver/src/kernel.rs`
- Unit tests for eval, inject, extract

## Types

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;
use anyhow::Result;

/// Persistent Python interpreter with globals that survive across evals
pub struct Kernel {
    /// Main module globals dict
    globals: Py<PyDict>,
}

impl Kernel {
    /// Create new interpreter with empty globals
    pub fn new() -> Result<Self>;

    /// Evaluate Python code, returning the result
    pub fn eval(&self, code: &str) -> Result<PyObject>;

    /// Execute Python statements (no return value)
    pub fn exec(&self, code: &str) -> Result<()>;

    /// Inject a Rust value into Python globals
    pub fn inject<T: IntoPy<PyObject>>(&self, name: &str, value: T) -> Result<()>;

    /// Extract a value from Python globals
    pub fn extract<'py, T: FromPyObject<'py>>(&self, py: Python<'py>, name: &str) -> Result<T>;

    /// Get a reference to globals for direct manipulation
    pub fn globals<'py>(&self, py: Python<'py>) -> &Bound<'py, PyDict>;

    /// Clear all globals (for reset)
    pub fn clear(&self) -> Result<()>;
}
```

## Implementation Notes

- Use `Python::with_gil()` for all operations
- Store globals as `Py<PyDict>` for thread-safe reference
- The `__builtins__` key must be preserved when clearing
- Consider adding `sys.path` manipulation for imports

## Definition of Done

```bash
cargo fmt --check -p vibeweaver
cargo clippy -p vibeweaver -- -D warnings
cargo test -p vibeweaver kernel::
```

## Acceptance Criteria

- [ ] `eval("1 + 1")` returns Python int 2
- [ ] `inject("x", 42)` followed by `eval("x * 2")` returns 84
- [ ] Globals persist across multiple eval calls
- [ ] `clear()` resets state but keeps builtins
