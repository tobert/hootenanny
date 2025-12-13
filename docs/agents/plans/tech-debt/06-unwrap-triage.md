# 06: Unwrap Triage

**Files:** Top 10 by unwrap count
**Focus:** Add error context to risky unwraps
**Dependencies:** After 01-05 complete

---

## Task

Run triage command:
```bash
rg -c '\.unwrap\(\)' crates/ --type rust | sort -t: -k2 -rn | head -10
```

For each high-count file, review unwraps and:
1. **Safe unwraps** (after `is_some()` check, in tests, infallible): Leave as-is
2. **Risky unwraps** (could panic in production): Replace with:
   - `.context("description")?` if in Result function
   - `.expect("reason this is safe")` if provably safe
   - `.unwrap_or_default()` or `.unwrap_or_else(|| ...)` if fallback acceptable

**Why this first?** After functional fixes (01-05), this is hardening.

**Deliverables:**
1. Risky unwraps replaced with contextual errors
2. Remaining unwraps documented as safe

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- Test code (unwrap acceptable there)
- Refactoring APIs to return Result

---

## Priority Files

Focus on non-test code in:
- Tool handlers (user-facing)
- Job processing (background tasks)
- IPC/network code

---

## Current Stats

- **938** `.unwrap()` calls across 83 files
- **66** `.expect()` calls
- **182** `.context()` calls (good practice being used)

---

## Acceptance Criteria

- [ ] No unwrap in error paths that could panic on bad input
- [ ] Remaining unwraps either in tests or have `expect()` with reason
- [ ] cargo test passes
