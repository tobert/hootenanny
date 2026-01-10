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

All tools use consistent prefixes for discoverability. Use `help()` to browse all 51 tools.

| Prefix/Name | Domain | Examples |
|-------------|--------|----------|
| `orpheus_*` | MIDI generation | `orpheus_generate`, `orpheus_continue`, `orpheus_bridge` |
| `abc_*` | ABC notation | `abc_validate`, `abc_to_midi` |
| `midi_*` | MIDI operations | `midi_render`, `midi_classify`, `midi_info` |
| `audio_*` | Audio I/O | `audio_output_attach`, `audio_input_attach`, `audio_monitor` |
| `beats_detect` | Beat detection | `beats_detect` |
| `audio_analyze` | CLAP analysis | `audio_analyze` |
| `timeline_*` | Timeline regions | `timeline_region_create`, `timeline_region_list`, `timeline_clear` |
| `job_*` | Job management | `job_poll`, `job_cancel`, `job_list` |
| `graph_*` | Audio routing | `graph_bind`, `graph_connect`, `graph_query`, `graph_context` |
| `kernel_*` | Python kernel | `kernel_eval`, `kernel_session`, `kernel_reset` |
| `artifact_*` | Artifacts | `artifact_upload`, `artifact_list`, `artifact_get` |
| bare verbs | Playback | `play`, `pause`, `stop`, `seek`, `tempo`, `status` |
| `help` | Documentation | `help` (call with `tool:` or `category:` params) |

### Adding New Tools & Cap'n Proto Schemas

When adding new tools or modifying the protocol, follow this checklist to ensure cargo properly rebuilds:

#### Adding a New Schema File

1. Create `crates/hooteproto/schemas/newschema.capnp`
2. **Update `build.rs`** - Add both the file reference AND the rerun directive:
   ```rust
   // In the schemas array:
   "schemas/newschema.capnp",
   ```
3. Add the generated module to `lib.rs`:
   ```rust
   pub mod newschema_capnp {
       include!(concat!(env!("OUT_DIR"), "/newschema_capnp.rs"));
   }
   ```

#### Modifying Existing Schemas

When you change a `.capnp` file, cargo should automatically rebuild thanks to `build.rs` watching each file individually. If it doesn't rebuild:

```bash
# Force rebuild of hooteproto
cargo clean -p hooteproto && cargo build -p hooteproto
```

**Why this matters:** Cargo's directory watching (`rerun-if-changed=schemas/`) only detects file additions/removals, not content changes. We explicitly list each schema file to ensure content changes trigger rebuilds.

#### Adding a New Tool (Full Checklist)

1. **Schema** (`crates/hooteproto/schemas/tools.capnp`)
   - Add request struct (e.g., `struct MyToolRequest { ... }`)
   - Add variant to `ToolRequest` union with next available ordinal

2. **Rust Types** (`crates/hooteproto/src/request.rs`)
   - Add `MyToolRequest` struct with serde derives
   - Add `MyTool(MyToolRequest)` variant to `ToolRequest` enum
   - Implement `tool_name()` and `timing()` for the variant

3. **Serialization** (`crates/hooteproto/src/conversion.rs`)
   - Add serialization in `request_to_capnp_tool_request()`
   - Add deserialization in `capnp_tool_request_to_request()`

4. **Response** (if tool returns data)
   - Add response struct to `schemas/responses.capnp`
   - Add Rust type to `crates/hooteproto/src/responses.rs`
   - Add serialization/deserialization in `conversion.rs`

5. **MCP Dispatch** (`crates/holler/src/dispatch.rs`)
   - Add JSON args struct (e.g., `struct MyToolArgs { ... }`)
   - Add match arm in `json_to_payload()` for `"my_tool"`

6. **Typed Dispatcher** (`crates/hootenanny/src/api/typed_dispatcher.rs`)
   - Add match arm in `dispatch_async()` or `dispatch_fire_and_forget()` based on timing

7. **Tool Schema** (`crates/hootenanny/src/api/tools_registry.rs`)
   - Add to `list_tools()` with JSON schema for MCP discovery

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

### Garden Query Timing

All Region timing in chaosgarden uses **beats** (not seconds or samples):

| Concept | `schedule` tool | Region schema | Unit |
|---------|-----------------|---------------|------|
| Start position | `at` | `position` | beats |
| Length | `duration` | `duration` | beats |
| End position | (computed) | `end` | beats |

Example: `schedule(at=0, duration=4)` creates a Region with `position=0`, `duration=4`, `end=4`.

Query regions with Trustfall:
```graphql
{ Region { id @output position @output duration @output behavior_type @output } }
```

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

## 📚 ZeroMQ Reference Material

Local clones of authoritative ZeroMQ documentation (for protocol work):

| Repo | Path | Key Files |
|------|------|-----------|
| ZeroMQ Guide | `~/src/zguide/` | `site/content/docs/chapter4.md` (Paranoid Pirate, Majordomo, heartbeating) |
| ZeroMQ RFCs | `~/src/rfc/` | `content/docs/rfcs/7/README.md` (MDP 0.1), `content/docs/rfcs/18/README.md` (MDP 0.2) |

Our protocol (`HOOT01`) is inspired by MDP but simplified for our use case. See
`docs/ARCHITECTURE.md` for the system design.

### Lazy Pirate Pattern

All ZMQ DEALER clients must follow the **Lazy Pirate** pattern from zguide Chapter 4
(`~/src/zguide/site/content/docs/chapter4.md`):

- **connect() is non-blocking** - ZMQ handles reconnection automatically, peers don't need to exist
- **Retry failed requests** - Timeout and retry up to `max_retries` times before failing
- **Track health via responses** - "Connected" means peer is responding, not that socket is connected
- **Never destroy sockets** - Let ZMQ handle reconnection; destroying sockets loses queued messages

Services can start in any order. `hooteproto::HootClient` implements this pattern.

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

