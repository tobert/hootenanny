# CLAUDE.md - Agent Context for Hootenanny

Hootenanny is an MCP server that exposes music creation tools to AI agents. It uses ZeroMQ for service communication, Cap'n Proto for serialization, and PipeWire for audio.

## Spirit of the Project

This is an instrument, not just infrastructure.
We're building tools for ears - human and AI alike.

**Sound is the purpose.** Every abstraction should make musical sense.
A `Beat` is a moment in time. A `Region` is a phrase waiting to be heard.
An `Artifact` is a creative act with lineage.

**Exploration is the method.** This is research into what's possible when
AI and humans make music together. Try things. Question assumptions.
The best patterns emerge from play.

**Collaboration is the medium.** Many minds touch this code - human and AI,
Claude and Gemini and whatever comes next. Write for the contributor who
follows you. Leave the codebase more welcoming than you found it.

**Expression matters.** Beautiful code isn't vanity - it's clarity.
Rich types, clear names, thoughtful structure. The code should teach as it runs.

Make it work. Make it clear. Make it sing.

## Quick Orientation

**Crate structure:**
- `holler` — MCP gateway, routes JSON tool calls to ZMQ backends
- `hootenanny` — Control plane: jobs, artifacts, GPU service clients
- `hooteproto` — Wire protocol: Cap'n Proto schemas, Rust types, serialization
- `chaosgarden` — Audio daemon: PipeWire, timeline, transport
- `vibeweaver` — Python kernel via PyO3
- `cas` — Content-addressed storage (BLAKE3)
- `abc` — ABC notation parser
- `audio-graph-mcp` — Trustfall query adapter
- `hooteconf` — Configuration loading

**Key files when adding tools:**
- `crates/hooteproto/schemas/tools.capnp` — Schema definitions
- `crates/hooteproto/src/request.rs` — Rust request types
- `crates/holler/src/dispatch.rs` — JSON → protocol conversion
- `crates/hootenanny/src/api/typed_dispatcher.rs` — Tool execution
- `crates/holler/src/tools_registry.rs` — MCP tool schemas

## Development Guidelines

### Error Handling

- Use `anyhow::Result` for fallible operations
- Never use `unwrap()` — propagate with `?`
- Add context: `.context("what we were trying to do")`
- Never discard errors with `let _ =`

### Code Style

- Correctness and clarity over performance
- No summary comments — code should be self-explanatory
- Comments only for non-obvious "why"
- Add to existing files unless it's a new logical component
- Avoid `mod.rs` — use `src/module_name.rs`
- Full words for names, no abbreviations
- Prefer newtypes over primitives: `struct JobId(Uuid)` not `Uuid`
- Use enums for states and variants
- Define traits for shared capabilities

### Version Control

- **Never `git add .` or `git add -A`** — always explicit paths
- Review with `git status` before and after staging
- Use `git diff --staged` before committing
- Commit frequently with clear messages

### Commit Attribution

Include Co-Authored-By for all contributors:
- `🤖 Claude <claude@anthropic.com>`
- `💎 Gemini <gemini@google.com>`

## Tool System

### Tool Prefixes

| Prefix | Domain |
|--------|--------|
| `orpheus_*` | MIDI generation |
| `abc_*` | ABC notation |
| `midi_*` | MIDI operations |
| `audio_*` | Audio I/O |
| `musicgen_*` | Text→audio |
| `yue_*` | Lyrics→song |
| `beats_detect` | Rhythm analysis |
| `audio_analyze` | CLAP embeddings |
| `timeline_*` | Timeline regions |
| `play/pause/stop/seek/tempo` | Transport |
| `artifact_*` | Storage |
| `job_*` | Job management |
| `graph_*` | Trustfall queries |
| `kernel_*` | Python kernel |
| `config/status/storage_stats` | System |
| `help` | Documentation |

### Adding a New Tool

1. **Schema** — `crates/hooteproto/schemas/tools.capnp`
   - Add request struct
   - Add variant to `ToolRequest` union

2. **Rust Types** — `crates/hooteproto/src/request.rs`
   - Add request struct with serde derives
   - Add enum variant
   - Implement `tool_name()` and `timing()`

3. **Serialization** — `crates/hooteproto/src/conversion.rs`
   - Add to `request_to_capnp_tool_request()`
   - Add to `capnp_tool_request_to_request()`

4. **MCP Dispatch** — `crates/holler/src/dispatch.rs`
   - Add JSON args struct
   - Add match arm in `json_to_payload()`

5. **Execution** — `crates/hootenanny/src/api/typed_dispatcher.rs`
   - Add match arm in `dispatch_async()` or `dispatch_fire_and_forget()`

6. **Discovery** — `crates/holler/src/tools_registry.rs`
   - Add to `list_tools()` with JSON schema

### Cap'n Proto Rebuilds

If schema changes don't trigger rebuilds:
```bash
cargo clean -p hooteproto && cargo build -p hooteproto
```

## Trustfall Queries

All graph queries use `graph_query()` with Trustfall syntax:

```graphql
# Find MIDI artifacts
{ Artifact(tag: "type:midi") { id @output creator @output } }

# Traverse lineage
{ Artifact(id: "abc123") { id parent { id parent { id } } } }

# Find devices
{ Identity { name hints { value @filter(op: "has_substring", value: ["roland"]) } } }
```

Queryable types: `Artifact`, `Identity`, `PipeWireNode`, `Region`

## Timeline Timing

All timeline positions use **beats**, not seconds:

```javascript
timeline_region_create({
  position: 0,    // beat 0
  duration: 4,    // 4 beats
  behavior_type: "play_audio",
  content_id: "artifact_123"
})
```

## Async Jobs

Generation tools return immediately with `job_id`:

```javascript
job = orpheus_generate({temperature: 1.0})
result = job_poll({job_ids: [job.job_id], timeout_ms: 60000})
// result.artifact_id contains the output
```

## ZeroMQ Patterns

Services use Lazy Pirate pattern:
- `connect()` is non-blocking — peers don't need to exist yet
- Timeout and retry on failures
- Services can start in any order
- Never destroy sockets — let ZMQ handle reconnection

## Artifacts

Prefer artifact IDs over raw CAS hashes:
- Artifacts track lineage (parent chains)
- Artifacts have tags and metadata
- Use CAS hashes only for direct file access (ffmpeg, etc.)

```
GET /artifact/{id}       → Content stream
GET /artifact/{id}/meta  → JSON metadata
GET /artifacts           → List (filterable)
```

## Reference

- Architecture: `docs/ARCHITECTURE.md`
- ZeroMQ Guide: [zguide.zeromq.org](https://zguide.zeromq.org) (Chapter 4 for Lazy Pirate)
- Trustfall: [docs.rs/trustfall](https://docs.rs/trustfall/latest/trustfall/)
