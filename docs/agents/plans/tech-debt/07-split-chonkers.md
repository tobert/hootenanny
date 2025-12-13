# 07: Split Chonkers

**Files:** Large files that hurt agent token efficiency
**Focus:** Split files where agents waste tokens reading irrelevant code
**Dependencies:** After 01-06 (avoid merge conflicts)

---

## Task

Split large files **only when it improves agent productivity**. Files with data (queries, schemas, generated code) are fine large. Files with mixed concerns waste tokens when agents read the whole thing to find one function.

**Why this first?** After all functional fixes, this is maintenance optimization.

**Deliverables:**
1. Split files where logical boundaries exist
2. Maintain API compatibility via re-exports

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
cargo doc
```

## Out of Scope

- Changing public APIs
- Renaming types
- Performance optimization

---

## Candidate Files (1000+ lines)

| File | Lines | Split? | Rationale |
|------|-------|--------|-----------|
| `chaosgarden/src/query.rs` | 1507 | Maybe | If mostly Trustfall queries, leave. If mixed adapter+resolvers+helpers, split |
| `chaosgarden/src/capabilities.rs` | 1338 | Review | Check if logical domains exist |
| `chaosgarden/src/primitives.rs` | 1238 | Review | Types vs builders vs conversions |
| `chaosgarden/src/playback.rs` | 1150 | Yes | Engine, scheduling, buffers are distinct |
| `chaosgarden/src/daemon.rs` | 1136 | Yes | Daemon, handlers, state are distinct |
| `hooteproto/src/lib.rs` | 1026 | Yes | Types, Payload enum, conversion are distinct |

---

## Decision Criteria

**Split if:**
- File has 3+ distinct logical domains
- Agent often reads entire file to find one function
- Tests frequently modified alongside unrelated code

**Don't split if:**
- File is mostly data (queries, schemas, constants)
- Types are tightly coupled and always used together
- Split would require lots of cross-module imports

---

## Split Strategy

1. Identify logical boundaries within file
2. Create new module files
3. Move types/functions to appropriate modules
4. Re-export from parent for API compatibility:
   ```rust
   // lib.rs
   mod types;
   mod payload;
   pub use types::*;
   pub use payload::*;
   ```
5. Update imports throughout codebase

---

## Recommended Splits

**hooteproto/lib.rs (1026 lines):**
- Keep `lib.rs` as re-export hub
- `types.rs` - ToolInfo, ToolOutput, ToolError, JobInfo, etc.
- `payload.rs` - Payload enum (large but cohesive)
- `conversion.rs` - tool_to_payload functions (already exists, expand)

**chaosgarden/daemon.rs (1136 lines):**
- `daemon.rs` - GardenDaemon struct, lifecycle
- `handlers.rs` - IPC message handlers
- `state.rs` - SharedState, timeline management

---

## Acceptance Criteria

- [ ] No file over 800 lines unless justified (data files OK)
- [ ] All existing tests pass
- [ ] Public API unchanged (re-exports maintain compatibility)
- [ ] `cargo doc` generates clean documentation
