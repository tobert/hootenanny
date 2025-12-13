# 05: Dead Code Cleanup

**Files:**
- `crates/chaosgarden/src/playback.rs` (5 annotations)
- `crates/hootenanny/src/artifact_store.rs` (6 annotations)
- `crates/holler/src/telemetry.rs` (2 annotations)

**Focus:** Remove dead code or justify with comment
**Dependencies:** None

---

## Task

For each `#[allow(dead_code)]` annotation, determine:
1. Is it used elsewhere? → Remove annotation
2. Is it planned for imminent use? → Add TODO comment with feature name
3. Is it obsolete? → **Delete the code**

**Why this first?** Reduces noise, improves maintainability, clarifies what's intentional vs forgotten.

**Deliverables:**
1. Dead code either deleted or documented
2. No unexplained `#[allow(dead_code)]` annotations

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- Implementing the features that would use the dead code

---

## playback.rs Analysis

```rust
// Line 29-30: routes field in struct
#[allow(dead_code)]
routes: Vec<Route>,

// Line 37-38: signal_type field
#[allow(dead_code)]
signal_type: SignalType,

// Line 42-47: entire Route struct
#[allow(dead_code)]
struct Route { ... }

// Line 168-173: ActiveCrossfade struct
#[allow(dead_code)]
struct ActiveCrossfade { ... }
```

**Decision:** These are for upcoming routing/crossfade features. Either:
- Delete if routing is deferred indefinitely
- Add `// TODO(routing): Used when audio routing is implemented` comment

---

## artifact_store.rs Analysis

Read file to identify the 6 dead code annotations and their purpose.

---

## telemetry.rs Analysis

Read file to identify the 2 dead code annotations.

---

## Acceptance Criteria

- [ ] No `#[allow(dead_code)]` without justifying comment
- [ ] Obsolete code deleted (git history preserves it)
- [ ] `cargo clippy` passes
