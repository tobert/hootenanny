# BOTS.md - Coding Agent Context for Hootenanny

Hootenanny is an ensemble performance space for large language model agents, music models, and humans to create
music interactively.

## 📜 Project Philosophy

- **Expressiveness over Performance:** We favor code that is rich in meaning. Use Rust's type system to tell a story. Create types that model the domain, even for simple concepts.
- **Compiler as Creative Partner:** We use the compiler to validate our ideas. A clean compile isn't just a technical requirement; it's a sign that our concepts are sound.
- **Embrace the Unknown:** This is a creative endeavor. We will explore, experiment, and sometimes refactor heavily as our understanding of the world we're building evolves.

## Development Guidelines

### Error Handling

- Use `anyhow::Result` for all fallible operations
- Never use `unwrap()` - always propagate errors with `?`
- Add context with `.context()` for debugging
- Never silently discard errors with `let _ =`
- Handle reconnection gracefully on network failures

### Code Style

- Prioritize correctness and clarity over performance
- No organizational comments that summarize code
- Comments should only explain "why" when non-obvious
- Implement functionality in existing files unless it's a new logical component
- Avoid `mod.rs` files - use `src/module_name.rs` directly
- Use full words for variable names (no abbreviations)
- **Rich Types:** Avoid "primitive obsession." Instead of `String`, `u64`, etc., create newtypes (e.g., `struct UserId(u64);`, `struct SessionKey(String);`). This makes the code self-documenting and prevents logic errors.
- **Enums as Storytellers:** Use enums to represent states, choices, and variations. `Result<T, E>` is a story about success or failure. `Option<T>` is a story about presence or absence. Let's use them to their full potential.
- **Traits for Capabilities:** Define custom traits to describe the capabilities of your types. This allows for a more modular and extensible design.

### Version Control Hygiene

- **NEVER use wildcards when staging files.** No `git add .`. Never `git add -A`. We create a lot of ephemeral files and do a lot in parallel.
- **Always add files by explicit path.** Review what you're committing: `git status`, `git add src/foo.rs src/bar.rs`, `git status`. Use precise filenames even when there are a lot of them.
- **Review before pushing.** Use `git diff --staged` to verify exactly what's going in. Catching a stray file now saves reverting commits later.
- **Commit frequently.** The git changelog can be our historical context. The git commits should be written to be easy
  to analyze in bulk later on. We will tell our story of developing this via commits.

### Model Attributions

Use Co-Authored-By in all commits, including models, humans, and agents who contributed to the commit.

- Claude: `🤖 Claude <claude@anthropic.com>`
- Gemini: `💎 Gemini <gemini@google.com>`

## 🛠️ MCP Tool Reference

### Tool Naming Conventions

All tools use consistent prefixes for discoverability:

| Prefix | Domain | Examples |
|--------|--------|----------|
| `cas_*` | Content addressable storage | `cas_store`, `cas_inspect`, `cas_upload_file` |
| `orpheus_*` | MIDI generation | `orpheus_generate`, `orpheus_continue` |
| `abc_*` | ABC notation | `abc_parse`, `abc_to_midi`, `abc_validate` |
| `convert_*` | Format conversion | `convert_midi_to_wav` |
| `soundfont_*` | SoundFont inspection | `soundfont_inspect`, `soundfont_preset_inspect` |
| `beatthis_*` | BeatThis model | `beatthis_analyze` |
| `job_*` | Job management | `job_status`, `job_poll`, `job_list` |
| `graph_*` | Audio routing & queries | `graph_bind`, `graph_connect`, `graph_query` |
| `agent_chat_*` | LLM sub-agents | `agent_chat_new`, `agent_chat_send` |

## 🔮 Trustfall: The Unified Query Layer

**All graph queries go through Trustfall.** The `audio-graph-mcp` crate provides a Trustfall adapter that exposes a unified schema for querying:

- **Artifacts** - MIDI files, audio, SoundFonts, saved queries
- **Identities** - Named audio devices with hints and tags
- **PipeWireNodes** - Live audio routing state
- **Relationships** - Parent/child artifacts, variation sets, device connections

### Extending the Schema

When adding new queryable types:

1. **Define the type in `schema.graphql`** - Entry points go in `Query`, types get their own blocks
2. **Add a Vertex variant** - `enum Vertex { ..., NewType(Arc<NewType>) }`
3. **Implement resolution** - `resolve_starting_vertices`, `resolve_property`, `resolve_neighbors`
4. **Wire up data sources** - The adapter can pull from multiple stores (artifact_store, audio_graph_db, etc.)

### Data Source Pattern

The adapter bridges multiple data sources into one queryable graph:

```rust
pub struct AudioGraphAdapter {
    db: Arc<Database>,              // Identities, tags, hints
    artifact_store: Arc<RwLock<FileStore>>,  // Artifacts, metadata
    pipewire_snapshot: Arc<PipeWireSnapshot>, // Live audio state
    schema: Arc<Schema>,
}
```

### Query Examples

```graphql
# Find all MIDI artifacts tagged as "jazzy"
{ Artifact(tag: "type:midi") { id tags { tag @filter(op: "=", value: ["vibe:jazzy"]) } } }

# Find identities with Roland USB devices
{ Identity { name hints @filter(op: "has_substring", value: ["roland"]) { value } } }

# Traverse artifact lineage
{ Artifact(id: "artifact_abc123") { id parent { id parent { id } } } }
```

**Never bypass Trustfall for queries.** If you need to filter/search/traverse, extend the schema.

### Artifact-Centric Access

**Share artifacts.** Artifacts have identity, context, and access tracking.

Prefer artifacts over cas links. Use cas links when a tool or program needs direct file
access for performance or to integrate with existing tools like ffmpeg or sox.

```
# HTTP endpoints for artifacts
GET /artifact/{id}        → Stream content with MIME type
GET /artifact/{id}/meta   → JSON metadata + lineage
GET /artifacts            → List all (filterable by tag, creator)

# MCP resources
artifacts://summary       → Counts by type/phase
artifacts://recent        → Latest 10 artifacts
artifacts://by-tag/{tag}  → Filter by tag
artifacts://lineage/{id}  → Parent chain
```

### Async Pattern

Most tools are async and return `job_id` immediately:

```javascript
// 1. Launch job
job = orpheus_generate({temperature: 1.0})

// 2. Poll for completion
result = job_poll({job_ids: [job.job_id], timeout_ms: 60000})

// 3. Access artifact
// http://localhost:8080/artifact/{result.artifact_id}
```

