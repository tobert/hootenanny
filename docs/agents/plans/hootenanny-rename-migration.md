# Hootenanny Rename Migration Plan

Migrate references from "halfremembered-mcp" to "hootenanny" across the codebase.

## Guiding Principles

1. **Production-critical first** - Fix runtime paths and environment variables before docs
2. **Preserve history** - Archive docs can keep original names (they're historical records)
3. **Clean environment variable naming** - Use `HOOTENANNY_*` prefix
4. **User-facing paths** - `~/.hootenanny/` instead of `~/.halfremembered/`

## Phase 1: Runtime & Configuration (Critical)

### 1.1 Environment Variables (Breaking Change)

| Old | New |
|-----|-----|
| `HALFREMEMBERED_CAS_PATH` | `HOOTENANNY_CAS_PATH` |
| `HALFREMEMBERED_CAS_READONLY` | `HOOTENANNY_CAS_READONLY` |

**Files:**
- `crates/cas/src/config.rs` - Update env var names, add deprecation support
- `crates/cas/src/lib.rs` - Update documentation

**Migration strategy:** Support both old and new env vars temporarily, with warnings for old names.

### 1.2 Default Paths

| Old | New |
|-----|-----|
| `~/.halfremembered/cas` | `~/.hootenanny/cas` |
| `/tank/halfremembered/hrmcp/default` | `/tank/hootenanny/hrmcp/default` |

**Files:**
- `crates/cas/src/config.rs:46-50` - Update `default_cas_path()`
- `crates/hootenanny/src/main.rs:78` - Update fallback path

**Note:** The `/tank/halfremembered/` path is shared storage. Coordinate with actual directory rename or symlink.

### 1.3 Systemd Service Files

**Files:**
- `systemd/hootenanny.service`
- `systemd/holler.service`
- `systemd/luanette.service`
- `systemd/chaosgarden.service`

All reference `%h/src/halfremembered-mcp/` - update to `%h/src/hootenanny/`

## Phase 2: Package Metadata

### 2.1 Cargo.toml Descriptions

**Files:**
- `crates/cas/Cargo.toml:5` - "Content Addressable Storage for halfremembered"
- `crates/chaosgarden/Cargo.toml:5` - "Realtime audio daemon for halfremembered..."

Update to reference "Hootenanny" instead.

### 2.2 Library Documentation Headers

**Files:**
- `crates/cas/src/lib.rs:1` - Library doc comment
- `crates/chaosgarden/src/lib.rs:3` - "The realtime audio component of the halfremembered system"

## Phase 3: Architecture Documentation (Important)

### 3.1 Core Architecture Docs

**Files:**
- `docs/ARCHITECTURE.md:1,5` - Title and description
- `docs/design/persistence.md:9,57,180` - Various references

### 3.2 Agent Documentation

**Files:**
- `docs/agents/README.md:5,152,229` - System description
- `docs/agents/plans/chaosgarden/README.md:10`

## Phase 4: Historical/Archive Docs (Optional)

These are historical records of the project's evolution. Consider:

**Option A:** Leave as-is (they document history)
**Option B:** Add a note at the top explaining the rename
**Option C:** Update references

**Files (if updating):**
- `docs/agents/plans/00-event-duality-hello/plan.md`
- `docs/agents/plans/02-cli/README.md`
- `docs/agents/plans/04-lua/plan.md`
- `docs/agents/plans/08-rmcp-session-resilience/`
- `docs/agents/plans/archive-00-init-obsolete/`
- `docs/agents/plans/direct-mcp-with-axum/claude-review-response.md`
- `docs/agents/plans/fill-in-missing-models/README.md`
- `docs/hrcli-mcp-features-implementation.md`

## Implementation Checklist

- [ ] **Phase 1.1:** Update environment variables in `crates/cas/src/config.rs`
- [ ] **Phase 1.2:** Update default paths in `config.rs` and `main.rs`
- [ ] **Phase 1.3:** Update systemd service files
- [ ] **Phase 2.1:** Update Cargo.toml descriptions
- [ ] **Phase 2.2:** Update library doc comments
- [ ] **Phase 3.1:** Update core architecture docs
- [ ] **Phase 3.2:** Update agent README
- [ ] **Phase 4:** Decision on historical docs
- [ ] **Filesystem:** Create symlink `/tank/hootenanny` -> `/tank/halfremembered` (or rename)
- [ ] **Test:** Run full test suite after changes
- [ ] **Deploy:** Update systemd services on target machines

## Open Questions

1. **Backwards compatibility period?** How long to support old `HALFREMEMBERED_*` env vars?
2. **Shared storage path?** Is `/tank/halfremembered/` used by other systems? Symlink or full rename?
3. **Archive docs?** Preserve historical names or update everything?

## Risk Assessment

| Change | Risk | Mitigation |
|--------|------|------------|
| Env vars | Medium - breaks existing configs | Deprecation warnings, support both |
| Default paths | Low - most users use explicit config | Document migration |
| Systemd paths | Low - requires manual redeploy | Clear instructions |
| Docs | None | Just text changes |

---

*Plan created: 2024-12-13*
*Status: ✅ Complete (code/config)*

## Completed Changes

| File | Change |
|------|--------|
| `crates/cas/src/config.rs` | `HALFREMEMBERED_*` → `HOOTENANNY_*`, `~/.halfremembered` → `~/.hootenanny` |
| `crates/cas/src/lib.rs` | Updated doc comments |
| `crates/cas/Cargo.toml` | Description updated |
| `crates/chaosgarden/src/lib.rs` | "halfremembered system" → "Hootenanny system" |
| `crates/chaosgarden/Cargo.toml` | Description updated |
| `crates/hootenanny/src/main.rs` | Default state dir → `~/.local/share/hootenanny`, fallback → `/tank/hootenanny/state/default` |
| `systemd/*.service` | All paths updated to `%h/src/hootenanny/` |
| `docs/ARCHITECTURE.md` | Title and intro updated |

## Remaining (separate task)

Historical docs in `docs/agents/plans/` still reference old names - intentionally preserved as historical record.
