# 03: Error Discards

**File:** `crates/hootenanny/src/api/tools/orpheus.rs` (primary)
**Focus:** Replace `let _ =` with proper error handling
**Dependencies:** None

---

## Task

Fix 20 instances of silent error discards in orpheus.rs. All are job state transitions.

**Why this first?** Violates CLAUDE.md guidelines. Silent failures make debugging impossible.

**Deliverables:**
1. Replace silent discards with logging or propagation
2. Document any intentionally ignored results

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check -p hootenanny
cargo test -p hootenanny
```

## Out of Scope

- Changing function signatures
- Changing JobStore API

---

## Instances to Fix

```
Line 63:  let _ = job_store.mark_running(&job_id_clone);
Line 143: let _ = bc.artifact_created(&id, &hash, tags, creator).await;
Line 157: let _ = job_store.mark_complete(&job_id_clone, response);
Line 160: let _ = job_store.mark_failed(&job_id_clone, ...);
Line 165: let _ = job_store.mark_failed(&job_id_clone, ...);
Line 211: let _ = job_store.mark_running(&job_id_clone);
Line 297: let _ = job_store.mark_complete(&job_id_clone, response);
Line 300: let _ = job_store.mark_failed(&job_id_clone, ...);
Line 305: let _ = job_store.mark_failed(&job_id_clone, ...);
Line 350: let _ = job_store.mark_running(&job_id_clone);
Line 434: let _ = job_store.mark_complete(&job_id_clone, response);
Line 437: let _ = job_store.mark_failed(&job_id_clone, ...);
Line 442: let _ = job_store.mark_failed(&job_id_clone, ...);
Line 487: let _ = job_store.mark_running(&job_id_clone);
Line 558: let _ = job_store.mark_complete(&job_id_clone, response);
Line 561: let _ = job_store.mark_failed(&job_id_clone, ...);
Line 567: let _ = job_store.mark_failed(&job_id_clone, ...);
Line 627: let _ = job_store.mark_running(&job_id_clone);
Line 696: let _ = job_store.mark_complete(&job_id_clone, ...);
Line 699: let _ = job_store.mark_failed(&job_id_clone, ...);
```

---

## Fix Pattern

**Option A: Log on failure (recommended for job state)**
```rust
// Before
let _ = job_store.mark_running(&job_id_clone);

// After
if let Err(e) = job_store.mark_running(&job_id_clone) {
    tracing::warn!(job_id = %job_id_clone, error = %e, "Failed to mark job running");
}
```

**Option B: Propagate (when in Result context)**
```rust
job_store.mark_running(&job_id_clone)?;
```

**Option C: Document intentional discard**
```rust
// Intentionally discarded: notification is best-effort, failure doesn't affect job
let _ = bc.artifact_created(&id, &hash, tags, creator).await;
```

---

## Analysis

| Line | Operation | Recommendation |
|------|-----------|----------------|
| 63, 211, 350, 487, 627 | `mark_running` | Log warning |
| 157, 297, 434, 558, 696 | `mark_complete` | Log warning |
| 160, 165, 300, 305, 437, 442, 561, 567, 699 | `mark_failed` | Log warning |
| 143 | `artifact_created` | Document intentional |

---

## Acceptance Criteria

- [ ] No silent `let _ =` on job state changes
- [ ] Failures logged with context (job_id, error message)
- [ ] Any intentional discards have comment explaining why
