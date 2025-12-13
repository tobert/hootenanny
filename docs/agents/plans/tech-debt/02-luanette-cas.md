# 02: Luanette CAS Integration

**File:** `crates/luanette/src/dispatch.rs`
**Focus:** Implement CAS script fetching at line 120
**Dependencies:** `cas` crate

---

## Task

The `job_execute` function at line 114-134 creates a job but doesn't actually fetch/execute the script from CAS. Implement:

1. Accept a CAS hash for the script
2. Fetch script content from CAS
3. Execute via LuaRuntime
4. Track job status properly

**Why this first?** Blocks lua scripting feature - scripts can't be stored/executed from CAS.

**Deliverables:**
1. CAS client integration in Dispatcher
2. Script fetch and execution
3. Proper job status tracking

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check -p luanette
cargo test -p luanette
```

## Out of Scope

- New Lua bindings
- ZMQ server changes

---

## Current Code (line 112-134)

```rust
pub async fn job_execute(
    &self,
    script_hash: &str,
    _params: Value,
    _tags: Option<Vec<String>>,
) -> Payload {
    // TODO: Fetch script from CAS and execute
    debug!("job_execute not fully implemented yet - needs CAS integration");
    let job_id = self.jobs.create_job(script_hash.to_string());
    // Returns without actually running anything
}
```

---

## Implementation

```rust
pub async fn job_execute(
    &self,
    script_hash: &str,
    params: Value,
    tags: Option<Vec<String>>,
) -> Payload {
    // 1. Fetch from CAS
    let script_content = match self.cas.get(script_hash).await {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => return Payload::Error {
                code: "invalid_script".into(),
                message: format!("Script is not valid UTF-8: {}", e),
                details: None,
            },
        },
        Err(e) => return Payload::Error {
            code: "cas_fetch_failed".into(),
            message: e.to_string(),
            details: None,
        },
    };

    // 2. Create job and spawn execution
    let job_id = self.jobs.create_job(script_hash.to_string());

    // 3. Spawn async execution
    let runtime = self.runtime.clone();
    let jobs = self.jobs.clone();
    tokio::spawn(async move {
        if let Err(e) = jobs.mark_running(&job_id) {
            tracing::warn!(job_id = %job_id, error = %e, "Failed to mark job running");
        }
        match runtime.execute(&script_content, params).await {
            Ok(result) => {
                if let Err(e) = jobs.mark_complete(&job_id, serde_json::to_value(result).unwrap_or_default()) {
                    tracing::warn!(job_id = %job_id, error = %e, "Failed to mark job complete");
                }
            }
            Err(e) => {
                if let Err(e2) = jobs.mark_failed(&job_id, format_lua_error(&e)) {
                    tracing::warn!(job_id = %job_id, error = %e2, "Failed to mark job failed");
                }
            }
        }
    });

    Payload::Success { result: serde_json::json!({"job_id": job_id.to_string()}) }
}
```

---

## Required Changes

1. Add `cas: Arc<CasClient>` to `Dispatcher` struct
2. Update `Dispatcher::new()` to accept CAS client
3. Implement the fetch-and-execute logic

---

## Acceptance Criteria

- [ ] Scripts stored in CAS can be executed via `job_execute`
- [ ] Invalid hash returns appropriate error
- [ ] Non-UTF8 content returns appropriate error
- [ ] Job status updates correctly (pending → running → complete/failed)
