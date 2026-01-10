# 08: Cleanup and Verification

**Files:** All crates
**Focus:** Remove MsgPack, verify migration complete
**Dependencies:** 07-holler-bridge
**Unblocks:** None (final task)

---

## Task

Remove MsgPack dependencies and legacy code. Verify the migration is complete and nothing is broken.

**Deliverables:**
1. `rmp-serde` removed from all Cargo.toml
2. Old serde-based Payload types removed or deprecated
3. All tests pass
4. Python can load schemas

**Definition of Done:**
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Verify no rmp-serde in lock file
! grep -q "rmp-serde" Cargo.lock
```

## Out of Scope

- ❌ New features — just cleanup

---

## Checklist

### 1. Remove MsgPack Dependencies

```bash
# Check which crates use rmp-serde
grep -r "rmp-serde" crates/*/Cargo.toml
```

Remove from each:
- `crates/hooteproto/Cargo.toml`
- `crates/hootenanny/Cargo.toml`
- `crates/chaosgarden/Cargo.toml`
- `crates/holler/Cargo.toml` (if present)

### 2. Remove Legacy Types

In `hooteproto/src/lib.rs`, the old serde-based types can be:
- Deleted entirely, OR
- Marked `#[deprecated]` with migration note

If other code still references them, fix those references first.

### 3. Update Frame Protocol

In `frame.rs`, remove:
- `ContentType::MsgPack` (if kept as alias)
- `HootFrame::request()` MsgPack version
- `HootFrame::payload()` MsgPack reader

Or keep them deprecated for one release cycle.

### 4. Clean Up Imports

Search for orphaned imports:
```bash
grep -r "rmp_serde" crates/
grep -r "use.*Payload" crates/  # Check if old Payload is used
```

### 5. Run Full Test Suite

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo doc --workspace --no-deps
```

### 6. Verify Python Access

```python
# test_schema.py
import capnp
capnp.remove_import_hook()

common = capnp.load('crates/hooteproto/schemas/common.capnp')
tools = capnp.load('crates/hooteproto/schemas/tools.capnp')

# Create a message
msg = tools.ToolRequest.new_message()
orpheus = msg.init('orpheusGenerate')
orpheus.temperature = 0.9
print("Schema loads and works!")
```

### 7. Update Documentation

- Update CLAUDE.md if it mentions MsgPack
- Update any architecture docs
- Add note about capnp schema location

---

## Verification Queries

```bash
# No MsgPack in dependencies
! grep -q "rmp-serde" Cargo.lock

# No MsgPack usage in code
! grep -rq "rmp_serde::" crates/

# All tests pass
cargo test --workspace

# No warnings
cargo clippy --workspace -- -D warnings

# Schemas are valid
capnp compile -o- crates/hooteproto/schemas/*.capnp
```

---

## Rollback Plan

If something is broken and we need to rollback:
1. `git revert` the cleanup commit
2. Keep both MsgPack and capnp temporarily
3. Fix the issue
4. Re-attempt cleanup

---

## Acceptance Criteria

- [ ] No `rmp-serde` in Cargo.lock
- [ ] No `rmp_serde::` in codebase
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace` clean
- [ ] Python can load and use schemas
- [ ] Documentation updated
