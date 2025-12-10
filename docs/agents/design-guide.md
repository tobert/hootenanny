# Agent Design Document Guide

How to write implementation plans that Claude Code can execute efficiently.

---

## Philosophy

**Focus over length.** The problem isn't token count — it's domain bleeding. When a primitives doc mentions rendering, Claude starts thinking about rendering, which expands context. Keep each document in its lane.

**Signatures, not implementations.** Type definitions and method signatures tell Claude *what* to build. Full implementations tell Claude what you *already* built. Let the agent write the code.

**Clear prompts for handoff.** Each task file should work as a standalone prompt. "Read 01-primitives.md and proceed" should be enough.

---

## Document Structure

```
docs/agents/plans/{feature}/
├── README.md      # Activation, tracking, architecture
├── DETAIL.md      # Design rationale (read during revisions)
├── 01-first.md    # Task 1
├── 02-second.md   # Task 2
└── ...
```

| Document | Purpose | When to Read |
|----------|---------|--------------|
| README.md | Orient, track progress, see architecture | Every session |
| DETAIL.md | Understand *why* decisions were made | Revision sessions |
| NN-task.md | Execute one implementation task | Implementing that module |

---

## README.md Template

```markdown
# Feature Name

**Location:** `crates/feature`
**Status:** Design Complete | In Progress | Complete

---

## Progress Tracking

| Task | Status | Assignee | Notes |
|------|--------|----------|-------|
| 01-primitives | complete | Claude | - |
| 02-graph | in_progress | - | blocked on X |
| 03-resolution | pending | - | - |

## Current Status

- **Completed**: ✅ 01-primitives
- **In Progress**: 🔄 02-graph
- **Next Up**: 03-resolution
- **Blocked**: None

## Success Metrics

We'll know we've succeeded when:
- [ ] Graph compiles and renders a simple timeline
- [ ] Offline rendering produces valid WAV
- [ ] Trustfall queries return correct results

## Signoffs & Decisions

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-12-08 | Use petgraph | StableGraph handles node removal |
| - | - | - |

## Session Notes

_Context for future sessions._

- Started with 7 task files
- Task 03 may need splitting

## Open Questions

- [ ] Should regions support overlapping?
- [ ] How to handle tempo changes mid-render?

---

## What This Is

[2-3 sentences explaining the feature]

## Architecture

```
[ASCII box diagram showing layers/components]
```

## Crate Structure

```
crates/feature/
├── src/
│   ├── lib.rs
│   ├── module1.rs    # 01: description
│   └── module2.rs    # 02: description
└── Cargo.toml
```

## Dependencies

```toml
[dependencies]
# list with versions
```

## Documents

| Document | Focus | Read When |
|----------|-------|-----------|
| [DETAIL.md](./DETAIL.md) | Design rationale | Revisions |
| [01-task](./01-task.md) | Module 1 | Implementing module1.rs |
```

**Target: ~100 lines**

---

## DETAIL.md Template

```markdown
# Feature Design Rationale

**Purpose:** Deep context for revision sessions. Read when you need to understand *why*.

---

## Why [Key Decision 1]?

[Explanation of the decision, alternatives considered, trade-offs]

## Why [Key Decision 2]?

[...]

## Cross-Cutting Concerns

### [Concern 1]

[How this affects multiple modules]

### [Concern 2]

[...]

## Open Questions

| Question | Context | Status |
|----------|---------|--------|
| - | - | - |

## Rejected Alternatives

| Alternative | Why Rejected |
|-------------|--------------|
| X | Caused Y problem |
```

**Target: Unlimited — only read during deep revision sessions**

---

## Task File Template

```markdown
# NN: Task Name

**File:** `src/module.rs`
**Focus:** [One domain only]
**Dependencies:** `crate1`, `crate2`

---

## Task

[Clear instruction: what to create, what file(s) to write]

**Why this first?** [Explain ordering rationale — what this enables, what depends on it]

**Deliverables:**
1. [Specific file with specific contents]
2. [Specific functionality working]
3. [Specific tests passing]

**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```

## Out of Scope

- ❌ [Adjacent work] — that's task NN
- ❌ [Other adjacent work] — that's task MM

Focus ONLY on [this domain].

---

## [Relevant External Crate] Patterns

```rust
// Key API usage the agent needs to know
use external_crate::Thing;
let x = Thing::new();
```

---

## Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyType {
    pub field: Type,
}

pub enum MyEnum {
    Variant1,
    Variant2 { data: String },
}

pub trait MyTrait: Send + Sync {
    fn method(&self, arg: Type) -> Result<Output>;
}
```

---

## Methods to Implement

**Construction:**
- `new(...) -> Self`
- `from_x(...) -> Self`

**Core:**
- `do_thing(&self, ...) -> Result<...>`
- `other_thing(&mut self, ...)`

**Queries:**
- `get_x(&self) -> &X`
- `find_by_y(&self, y: Y) -> Option<&Z>`

---

## Acceptance Criteria

- [ ] Types compile with derives as shown
- [ ] [Specific behavior] works correctly
- [ ] Tests cover [specific scenarios]
- [ ] [Edge case] handled
```

**Target: 100-200 lines**

---

## What Goes Where

| Content | Location | Example |
|---------|----------|---------|
| Type definitions | Task file | `pub struct Foo { ... }` |
| Trait definitions | Task file | `pub trait Bar { ... }` |
| Method signatures | Task file | `fn baz(&self) -> Result<X>` |
| Method bodies | Nowhere | Agent writes these |
| External crate patterns | Task file | `toposort(&graph, None)` |
| Design rationale | DETAIL.md | "We chose X because Y" |
| Rejected alternatives | DETAIL.md | "We didn't use Z because..." |
| Cross-cutting concerns | DETAIL.md | "Error handling affects all modules" |
| Progress tracking | README.md | Status table |
| Architecture overview | README.md | ASCII diagram |

---

## Focus Rules

### One Domain Per File

Each task file owns one domain. If you find yourself explaining another domain to make sense of this one, stop — you're bleeding.

**Good:** "Takes a `Graph` as input"
**Bad:** "Traverses the petgraph StableGraph using neighbors_directed..."

### Scope Boundaries

Every task file needs a "Do not" section:

```markdown
**Do not** implement buffer management — that's task 04.
**Do not** implement actual MCP calls — use dependency injection.
```

### Reference, Don't Explain

When task 04 depends on task 01's types:

```markdown
**Dependencies:** `primitives` (task 01)
```

Don't re-explain the types. The agent will read task 01 if needed.

---

## External Context

Use Exa or web search to find key patterns for dependencies, then embed them in task files:

```rust
// petgraph: topological sort
use petgraph::algo::toposort;
let order: Result<Vec<NodeIndex>, _> = toposort(&graph, None);

// trustfall: adapter trait
pub trait Adapter<'vertex> {
    fn resolve_starting_vertices(...) -> VertexIterator<...>;
    fn resolve_property(...) -> ContextOutcomeIterator<...>;
    fn resolve_neighbors(...) -> ContextOutcomeIterator<...>;
    fn resolve_coercion(...) -> ContextOutcomeIterator<...>;
}
```

This saves agents from searching and keeps them focused.

---

## Definition of Done

Every task file must include:

```markdown
**Definition of Done:**
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo check
cargo test
```
```

For feature-gated code, add both paths:

```bash
cargo check
cargo check --features feature_name
```

---

## Acceptance Criteria

Use checkboxes for functional requirements:

```markdown
## Acceptance Criteria

- [ ] `new()` creates valid instance
- [ ] Serialization round-trips correctly
- [ ] Error case returns `Err`, not panic
- [ ] Edge case X handled
```

These are *what* must work. Definition of Done is *how* to verify the code is ready.

---

## Token Budget Guidelines

| Document | Target Lines | Why |
|----------|--------------|-----|
| README.md | ~100 | Read every session |
| DETAIL.md | Unlimited | Only for revisions |
| Task files | 100-200 each | One per implementation session |
| Total plan | ~1500 lines | Full context in one read |

If a task file exceeds 200 lines, consider:
1. Are you including implementations? Remove them.
2. Are you explaining other domains? Stop.
3. Is this actually two tasks? Split it.

---

## Checklist Before Finalizing Plan

**README.md:**
- [ ] Progress tracking table
- [ ] Current status section
- [ ] Success metrics (checkboxes)
- [ ] Signoffs & decisions table
- [ ] Open questions (checkboxes)
- [ ] Architecture diagram

**DETAIL.md:**
- [ ] Explains *why* for key decisions
- [ ] Documents rejected alternatives

**Task Files:**
- [ ] Task section with clear prompt
- [ ] "Why this first?" rationale
- [ ] Definition of Done (fmt/clippy/check/test)
- [ ] Out of Scope section
- [ ] Acceptance Criteria checkboxes
- [ ] External crate patterns embedded

**Overall:**
- [ ] No full implementations anywhere
- [ ] No domain bleeding between task files
