# 07: Code Organization Cleanup

**Files:** Files with organizational debt from organic growth
**Focus:** Improve code locality, remove dead weight, extract overgrown modules
**Dependencies:** After 01-06 (avoid merge conflicts)

---

## Task

Clean up organizational debt identified through code review. Focus on:
1. **Locality** - Related code should live together
2. **Dead weight** - Remove unused fields, stale comments
3. **Abstraction boundaries** - Extract modules that outgrew their homes
4. **Consistency** - Shared patterns deserve shared types

**Why this matters:** Good organization reduces cognitive load for both humans and agents navigating the codebase.

**Deliverables:**
1. Four files cleaned up with specific improvements
2. One module extraction (Region → region.rs)
3. One shared type added (ArtifactMetadata)

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
- Splitting files purely by line count

---

## File-Specific Tasks

### 1. daemon.rs - Dead Weight Removal

**File:** `crates/chaosgarden/src/daemon.rs`
**Grade:** B+ → A

**Tasks:**
- [ ] Remove unused `config` field from `GardenDaemon` struct (line 54)
- [ ] Delete section marker comments (lines ~113, 151, 225, 306, 571)
  - These violate "no organizational comments" guideline
  - The code structure already provides grouping
- [ ] Remove TODO comment on line 301 or create tracking issue

**Validation:**
```bash
cargo check -p chaosgarden
cargo test -p chaosgarden
```

---

### 2. artifact_store.rs - Trait Impl Consolidation

**File:** `crates/hootenanny/src/artifact_store.rs`
**Grade:** B+ → A

**Tasks:**
- [ ] Move `impl ArtifactStore for FileStore` (lines 548-576) immediately after `impl ArtifactSource for FileStore` (before FileStoreSource)
- [ ] Group all FileStore trait impls together for better locality

**Current structure:**
```
impl FileStore { new, save }
impl ArtifactSource for FileStore { ... }
struct FileStoreSource { ... }
impl ArtifactSource for FileStoreSource { ... }
impl ArtifactStore for FileStore { ... }  // ← 90 lines away!
```

**Target structure:**
```
impl FileStore { new, save }
impl ArtifactSource for FileStore { ... }
impl ArtifactStore for FileStore { ... }  // ← Right after ArtifactSource
// --- Wrappers Section ---
struct FileStoreSource { ... }
impl ArtifactSource for FileStoreSource { ... }
```

**Validation:**
```bash
cargo check -p hootenanny
cargo test -p hootenanny
```

---

### 3. hooteproto/lib.rs - Shared Type + Documentation

**File:** `crates/hooteproto/src/lib.rs`
**Grade:** B+ → A-

**Tasks:**
- [ ] Add `ArtifactMetadata` shared type for lineage tracking fields:
  ```rust
  /// Common artifact lineage tracking fields.
  ///
  /// Embed this in tool variants that create artifacts to maintain lineage.
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
  pub struct ArtifactMetadata {
      pub variation_set_id: Option<String>,
      pub parent_id: Option<String>,
      #[serde(default)]
      pub tags: Vec<String>,
      pub creator: Option<String>,
  }
  ```

- [ ] Add comprehensive doc comment to `Payload` enum explaining organization:
  ```rust
  /// All message types in the Hootenanny system.
  ///
  /// Organized by domain:
  /// - **Core Protocol**: Worker lifecycle, job management, responses
  /// - **Content Ops**: CAS, artifacts, graph queries
  /// - **Music Generation**: AI models (Orpheus, Musicgen, YuE)
  /// - **Music Processing**: ABC, MIDI conversion, soundfonts
  /// - **Analysis**: Audio analysis (BeatThis, CLAP)
  /// - **Playback**: Chaosgarden timeline and transport
  /// - **MCP Compatibility**: Resources, prompts, completions
  /// - **Lua**: Script execution and storage
  ///
  /// Many variants include artifact tracking fields. See `ArtifactMetadata`.
  ```

- [ ] Reorganize Payload variant groupings with consistent section comments

**Note:** Don't refactor existing variants to use ArtifactMetadata yet - that's a larger change. Just add the type for new code.

**Validation:**
```bash
cargo check -p hooteproto
cargo test -p hooteproto
cargo doc -p hooteproto
```

---

### 4. primitives.rs - Extract Region Module

**File:** `crates/chaosgarden/src/primitives.rs`
**Grade:** B+ → A

**Tasks:**
- [ ] Create `crates/chaosgarden/src/region.rs`
- [ ] Move from primitives.rs to region.rs:
  - `ContentType` enum
  - `PlaybackParams` struct
  - `CurveType` enum
  - `CurvePoint` struct
  - `LatentStatus` enum
  - `ResolvedContent` struct
  - `LatentState` struct
  - `Behavior` enum
  - `TriggerKind` enum (+ its Serialize/Deserialize impls from line 954)
  - `RegionMetadata` struct
  - `Region` struct + all impls
  - Related tests
- [ ] Add re-exports in primitives.rs:
  ```rust
  mod region;
  pub use region::{
      Behavior, ContentType, CurvePoint, CurveType,
      LatentState, LatentStatus, PlaybackParams,
      Region, RegionMetadata, ResolvedContent, TriggerKind,
  };
  ```
- [ ] Update any imports in chaosgarden that directly reference these types

**Impact:** Reduces primitives.rs from ~1238 → ~650 lines

**Validation:**
```bash
cargo check -p chaosgarden
cargo test -p chaosgarden
cargo doc -p chaosgarden
```

---

## Execution Order

These can run in parallel since they touch different files:

```
Group 1 (parallel):
├── Agent A: daemon.rs cleanup
├── Agent B: artifact_store.rs consolidation
├── Agent C: hooteproto/lib.rs improvements
└── Agent D: primitives.rs → region.rs extraction
```

---

## Acceptance Criteria

- [ ] All four files improved per specifications above
- [ ] `cargo test` passes in all affected crates
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo doc` generates clean documentation
- [ ] No public API changes (re-exports maintain compatibility)
- [ ] Region extraction reduces primitives.rs to <700 lines

---

## Review Findings Summary

From organizational review on 2025-12-13:

| File | Issue | Impact |
|------|-------|--------|
| daemon.rs | Unused `config` field | Dead code noise |
| daemon.rs | Section marker comments | Violates project philosophy |
| artifact_store.rs | Scattered FileStore impls | Reduces locality |
| hooteproto/lib.rs | No shared ArtifactMetadata | Risk of inconsistency |
| hooteproto/lib.rs | Payload grouping unclear | Navigation difficulty |
| primitives.rs | Region outgrew file | 330 lines of non-primitive code |
