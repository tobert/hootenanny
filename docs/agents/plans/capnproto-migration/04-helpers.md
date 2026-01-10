# 04: Ergonomic Helpers

**Files:** `crates/hooteproto/src/builders.rs`, `crates/hooteproto/src/readers.rs`
**Focus:** Rust API ergonomics
**Dependencies:** 02-codegen-setup
**Unblocks:** 05-hootenanny, 06-chaosgarden

---

## Task

Create helper functions that make building and reading Cap'n Proto messages ergonomic. Raw capnp API is verbose; helpers make migration smoother.

**Deliverables:**
1. `builders.rs` — Functions to build common messages
2. `readers.rs` — Functions to extract data from messages
3. Re-export from lib.rs

**Definition of Done:**
```bash
cargo test -p hooteproto builders readers
```

## Out of Scope

- ❌ Every possible message type — focus on common patterns
- ❌ Macro magic — keep it simple, explicit functions

---

## builders.rs

```rust
//! Helper functions for building Cap'n Proto messages

use crate::capnp_gen::{common_capnp, envelope_capnp, tools_capnp};
use capnp::message::Builder;

/// Create a Timestamp from current time
pub fn timestamp_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// Build a simple success response
pub fn build_success(result_json: &str) -> Builder<capnp::message::HeapAllocator> {
    let mut message = Builder::new_default();
    {
        let mut envelope = message.init_root::<envelope_capnp::envelope::Builder>();
        // ... set fields
    }
    message
}

/// Build an OrpheusGenerate request
pub fn build_orpheus_generate(
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    num_variations: Option<u32>,
) -> Builder<capnp::message::HeapAllocator> {
    let mut message = Builder::new_default();
    {
        let mut req = message.init_root::<tools_capnp::tool_request::Builder>();
        let mut orpheus = req.init_orpheus_generate();
        if let Some(t) = temperature {
            orpheus.set_temperature(t);
        }
        if let Some(p) = top_p {
            orpheus.set_top_p(p);
        }
        if let Some(m) = max_tokens {
            orpheus.set_max_tokens(m);
        }
        if let Some(n) = num_variations {
            orpheus.set_num_variations(n);
        }
    }
    message
}

// Add more builders as needed during migration...
```

---

## readers.rs

```rust
//! Helper functions for reading Cap'n Proto messages

use crate::capnp_gen::{common_capnp, envelope_capnp, tools_capnp, jobs_capnp};
use capnp::message::Reader;
use capnp::serialize::SliceSegments;

/// Extract job status from a JobInfo reader
pub fn read_job_status(reader: &jobs_capnp::job_info::Reader) -> crate::JobStatus {
    match reader.get_status().unwrap() {
        common_capnp::JobStatus::Pending => crate::JobStatus::Pending,
        common_capnp::JobStatus::Running => crate::JobStatus::Running,
        common_capnp::JobStatus::Complete => crate::JobStatus::Complete,
        common_capnp::JobStatus::Failed => crate::JobStatus::Failed,
        common_capnp::JobStatus::Cancelled => crate::JobStatus::Cancelled,
    }
}

/// Check which tool request variant this is
pub fn tool_request_kind(reader: &tools_capnp::tool_request::Reader) -> &'static str {
    use tools_capnp::tool_request::Which;
    match reader.which().unwrap() {
        Which::CasStore(_) => "cas_store",
        Which::OrpheusGenerate(_) => "orpheus_generate",
        // ... etc
    }
}

// Add more readers as needed during migration...
```

---

## lib.rs Additions

```rust
pub mod builders;
pub mod readers;
```

---

## Design Note

These helpers are **not** meant to hide capnp entirely. They:
1. Reduce boilerplate for common operations
2. Provide type-safe conversions to/from our existing types
3. Make the migration incremental

Callers can still use raw capnp API when needed.

---

## Acceptance Criteria

- [ ] `builders` module compiles
- [ ] `readers` module compiles
- [ ] At least 3 builder functions for common messages
- [ ] At least 3 reader functions for common extractions
- [ ] Tests demonstrate usage patterns
