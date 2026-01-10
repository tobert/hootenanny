# Sync/Async Dispatch Layer

**Status:** Proposed
**Location:** `crates/hooteproto`, `crates/hootenanny/src/api/dispatch.rs`

## Problem

The current architecture conflates operation semantics with execution timing. Different consumers need different timing:

| Consumer | Needs |
|----------|-------|
| Chaosgarden | Fire-and-forget, event subscriptions, NEVER block |
| Luanette scripts | Sync facades for linear code |
| MCP/LLM clients | Request-response, but long jobs should return job_id |

## Design Principles

1. **hooteproto describes semantics, not timing**
2. **Return type signals timing** — `JobId` = async, direct result = sync
3. **IO can always stall** — file operations are async
4. **Pure compute can be sync** — bounded CPU, no IO
5. **Long-running tools return job_id even to MCP** — 20-minute operations shouldn't block
6. **Fire-and-forget returns dispatch errors, not execution errors** — chaosgarden logs failures, never waits

## Tool Categories

### Sync (no job system, immediate return)

Pure compute or in-memory queries, bounded work:

| Tool | Rationale |
|------|-----------|
| `abc_parse` | Parse string to AST |
| `abc_validate` | Validate string |
| `abc_transpose` | Transform AST |
| `garden_status` | Read transport state |
| `garden_get_regions` | Read region list |
| `job_status` | Lookup in job store |
| `job_list` | Iterate job store |
| `config_get` | Read config value |
| `graph_find` | Query in-memory graph |
| `graph_context` | Format cached data |
| `artifact_get` | Metadata lookup |
| `artifact_list` | Iterate store |
| `soundfont_inspect` | Cached metadata |
| `orpheus_classify` | Small/fast inference |

### Async Short (job system, MCP polls, ~30-120s timeout)

IO-bound or moderate GPU work:

| Tool | Timeout | Rationale |
|------|---------|-----------|
| `cas_store` | 30s | File write, could be large |
| `cas_upload_file` | 30s | File read + write |
| `artifact_upload` | 30s | Same |
| `convert_midi_to_wav` | 60s | CPU + disk |
| `orpheus_generate` | 120s | GPU, typically fast |
| `orpheus_continue` | 120s | GPU |
| `orpheus_bridge` | 120s | GPU |
| `orpheus_loops` | 120s | GPU |
| `abc_to_midi` | 30s | Creates artifact (IO) |

### Async Long (job system, MCP returns job_id immediately)

Long-running operations where MCP clients manage their own polling:

| Tool | Typical Duration | Rationale |
|------|------------------|-----------|
| `musicgen_generate` | 5-10 minutes | GPU, audio synthesis |
| `yue_generate` | 10-20 minutes | GPU, full song |
| `clap_analyze` | varies | Depends on audio length |
| `beatthis_analyze` | varies | Depends on audio length |

For these, MCP returns `{ "job_id": "..." }` and the client calls `job_poll` or `job_status`.

### Fire-and-Forget (dispatch errors only, execution errors go to logs/iopub)

Control commands with immediate effect:

| Tool | Notes |
|------|-------|
| `garden_play` | Returns `ok` or dispatch error |
| `garden_pause` | " |
| `garden_stop` | " |
| `garden_seek` | " |
| `garden_set_tempo` | " |
| `garden_emergency_pause` | " |
| `garden_create_region` | Returns region_id or error |
| `garden_delete_region` | " |
| `garden_move_region` | " |

## Dispatch Result Types

```rust
pub enum DispatchResult {
    /// Sync result, no job created
    Immediate(ToolResult),

    /// Async job, MCP should poll with given timeout class
    Job {
        job_id: JobId,
        timeout_class: TimeoutClass,
    },

    /// Fire-and-forget acknowledged
    /// Errors here are dispatch failures, not execution failures
    Ack(ToolResult),
}

pub enum TimeoutClass {
    /// ~30s - IO operations
    Short,
    /// ~120s - typical GPU inference
    Medium,
    /// Client manages - return job_id to MCP, don't poll
    Long,
}
```

## MCP Behavior by Timeout Class

```rust
impl Dispatcher {
    pub async fn dispatch_mcp(&self, payload: Payload) -> ToolResult {
        match self.dispatch(payload).await {
            DispatchResult::Immediate(result) => result,

            DispatchResult::Job { job_id, timeout_class } => {
                match timeout_class {
                    TimeoutClass::Short => {
                        self.poll_with_timeout(job_id, Duration::from_secs(30)).await
                    }
                    TimeoutClass::Medium => {
                        self.poll_with_timeout(job_id, Duration::from_secs(120)).await
                    }
                    TimeoutClass::Long => {
                        // Don't poll, return job_id for client to manage
                        Ok(ToolOutput::new(
                            format!("Job {} started. Use job_poll to check status.", job_id),
                            json!({ "job_id": job_id.as_str() })
                        ))
                    }
                }
            }

            DispatchResult::Ack(result) => result,
        }
    }
}
```

## Chaosgarden Integration

Chaosgarden NEVER calls sync dispatch. Two patterns:

### Pattern 1: Fire-and-forget for control

```rust
// Send and move on, don't wait for anything
zmq_client.send(Payload::GardenPlay).await;
```

### Pattern 2: Request + IOPub subscription for content

```rust
// Request generation
zmq_client.send(Payload::OrpheusGenerate { ... }).await;
// Don't wait! Continue real-time work

// Later, IOPub delivers result
fn handle_iopub(&mut self, event: IOPubEvent) {
    match event {
        IOPubEvent::JobComplete { job_id, result } => {
            // Content ready, safe to use
            self.on_content_ready(job_id, result);
        }
        IOPubEvent::JobFailed { job_id, error } => {
            // Log and continue, don't crash
            warn!("Job {} failed: {}", job_id, error);
        }
    }
}
```

## Future: Streaming

For tools that can produce partial results (musicgen audio chunks, progress updates):

```rust
pub enum DispatchResult {
    // ... existing variants ...

    /// Streaming results, subscribe to channel
    Streaming {
        job_id: JobId,
        channel: broadcast::Receiver<StreamChunk>,
    },
}

pub enum StreamChunk {
    Progress { percent: f32, message: String },
    PartialResult { data: Vec<u8> },
    Complete { result: ToolResult },
    Error { message: String },
}
```

This is future work — requires MCP streaming support or SSE integration.

## Migration Path

### Phase 1: Add timing annotations (this PR)
- Document timeout class in Payload comments
- No behavior change yet

### Phase 2: Implement DispatchResult
- Refactor dispatcher to return `DispatchResult`
- MCP adapter handles timeout classes
- Long jobs return job_id to MCP clients

### Phase 3: Optimize sync paths
- Remove job overhead for sync tools
- Direct execution → immediate response

### Phase 4: Streaming (future)
- Add streaming variant
- SSE or WebSocket delivery
