# Plan 11: Open Source Preparation

**Goal**: Prepare hootenanny-mcp for public release under MIT license.

## Decision: Baton Stays in Monorepo

**Recommendation**: Keep `baton` in the workspace.

**Rationale**:
- Baton already has `license = "MIT OR Apache-2.0"` (standard Rust dual-license)
- Can publish to crates.io independently while living in same repo (like tokio, serde)
- Simpler development - they're being built together
- Lower maintenance overhead vs separate repo
- Can spin off later if baton gains independent traction

## Current State

```
crates/
├── hootenanny/     # Main MCP server - musical ensemble space
├── hrcli/          # CLI client
├── baton/          # MCP server library (new, WIP)
├── resonode/       # Audio/resonance utilities
└── audio-graph-mcp/ # Audio graph management
```

## Tasks

### Task 1: License & Legal
- [ ] Add `LICENSE` file (MIT) to repo root
- [ ] Add license headers or ensure Cargo.toml has license field for all crates
- [ ] Audit for any third-party code that needs attribution

### Task 2: Repository Rename
- [ ] Rename directory: `halfremembered-mcp` → `hootenanny-mcp`
- [ ] Rename GitHub repo (if exists)
- [ ] Update MCP server configuration
- [ ] Update all `/tank/halfremembered/` paths to `/tank/hootenanny/` or make configurable

### Task 3: Clean Sensitive/Personal References
- [ ] Audit for hardcoded personal paths
- [ ] Check for any API keys, tokens, or secrets
- [ ] Review `.gitignore` for sensitive patterns
- [ ] Check for personal email addresses that shouldn't be public

### Task 4: Documentation Refresh
- [ ] Rewrite `README.md` for public audience
  - What is hootenanny-mcp?
  - Quick start
  - Features
  - Architecture overview
  - How to contribute
- [ ] Rename/update `BOTS.md` → consider keeping as `CLAUDE.md` or `AGENTS.md`
- [ ] Update `docs/ARCHITECTURE.md` - replace halfremembered references
- [ ] Create `CONTRIBUTING.md`
- [ ] Decide fate of `docs/agents/` - this is fascinating content, consider keeping!

### Task 5: Cargo Metadata
- [ ] Add to workspace `Cargo.toml`:
  - `repository = "https://github.com/tobert/hootenanny-mcp"`
  - `homepage`
  - `documentation`
- [ ] Ensure all crates have:
  - `license`
  - `description`
  - `readme` (if applicable)
  - `keywords`
  - `categories`

### Task 6: CI/CD Setup
- [ ] Create `.github/workflows/ci.yml`
  - `cargo build`
  - `cargo test`
  - `cargo clippy`
  - `cargo fmt --check`
- [ ] Add badges to README

### Task 7: Code Cleanup
- [ ] Run `cargo clippy` and fix warnings
- [ ] Run `cargo fmt`
- [ ] Review TODOs and FIXMEs
- [ ] Remove dead code / unused dependencies

### Task 8: Git History Decision
Options:
1. **Keep full history** - shows the collaborative agent development process (educational!)
2. **Squash to clean start** - simpler but loses the story
3. **Interactive rebase** - clean up sensitive commits only

**Recommendation**: Keep history. The agent collaboration history IS part of the project's value.

### Task 9: Final Review
- [ ] Fresh clone test: `git clone && cargo build && cargo test`
- [ ] Review all files one more time for anything missed
- [ ] Test MCP integration with Claude Code from clean state
- [ ] Create release tag v0.1.0

## What Makes This Project Interesting for Open Source

1. **Agent Memory System** (`docs/agents/`) - Novel approach to multi-agent collaboration
2. **Jujutsu Workflow** - Using jj as agent memory/handoff system
3. **Musical Domain Modeling** - Rich type system for collaborative music
4. **Baton** - Reusable MCP server library for Rust
5. **Agent Collaboration Docs** - Real examples of Claude/Gemini working together

## Files to Highlight in README

- `CLAUDE.md` / `BOTS.md` - How to work with this codebase as an AI
- `docs/agents/MEMORY_PROTOCOL.md` - Agent memory system
- `docs/agents/plans/` - Real agent collaboration plans
- The jj workflow integration

## Post-Release

- [ ] Announce on social media
- [ ] Write blog post about agent collaboration approach
- [ ] Consider crates.io publish for `baton` when stable
