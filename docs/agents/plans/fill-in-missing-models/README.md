# Fill In Missing Model Tools

This plan adds MCP tools for music model services that are running but not yet exposed via the MCP interface.

## Overview

| Task | Tool Name | Service Port | Type | Priority |
|------|-----------|--------------|------|----------|
| 01 | `orpheus_loops` | 2003 | MIDI generation | High |
| 02 | `orpheus_classify` | 2001 | Classification | Medium |
| ✅ 03 | `orpheus_bridge` | 2002 | MIDI generation (fix stub) | High |
| 04 | `musicgen_generate` | 2006 | Audio generation | High |
| 05 | `yue_generate` | 2008 | Song generation | Medium |
| 06 | `anticipatory_*` | 2011 | MIDI generation + embedding | Medium |
| 07 | `clap_analyze` | 2007 | Audio analysis | High |

## Service Port Reference

```
orpheus-base       2000  ✅ Already exposed
orpheus-classifier 2001  ❌ Task 02
orpheus-bridge     2002  ✅ Task 03 complete
orpheus-loops      2003  ❌ Task 01
orpheus-children   2004  ✅ Via model param
orpheus-mono       2005  ✅ Via model param
musicgen           2006  ❌ Task 04
clap               2007  ❌ Task 07
yue                2008  ❌ Task 05
stable-audio       2009  (not planned)
audioldm2          2010  (not planned)
anticipatory       2011  ❌ Task 06
beat-this          2012  ✅ Already exposed
llmchat            2020  ✅ Already exposed
```

## Implementation Pattern

All tools follow the established pattern in the codebase:

### 1. Schema Definition (`schema.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MyToolRequest {
    #[schemars(description = "Parameter description")]
    pub param: Option<String>,
    // ... standard artifact fields
}
```

### 2. HTTP Client (`local_models.rs` or dedicated file)

```rust
pub async fn run_my_tool(&self, ...) -> Result<MyResult> {
    let request_body = serde_json::json!({...});
    let builder = self.client.post(format!("{}/predict", url))
        .json(&request_body);
    let builder = self.inject_trace_context(builder);
    let resp = builder.send().await?;
    // Parse response, store in CAS, return result
}
```

### 3. Tool Implementation (`api/tools/*.rs`)

```rust
impl EventDualityServer {
    #[tracing::instrument(name = "mcp.tool.my_tool", skip(self, request))]
    pub async fn my_tool(&self, request: MyToolRequest) -> Result<CallToolResult, McpError> {
        // For slow operations: create job, spawn background task
        // For fast operations: call service directly, return result
    }
}
```

### 4. Handler Registration (`handler.rs`)

```rust
// In tools()
Tool::new("my_tool", "Description")
    .with_input_schema(schema_for::<MyToolRequest>()),

// In call_tool()
"my_tool" => {
    let request: MyToolRequest = serde_json::from_value(args)?;
    self.server.my_tool(request).await
}
```

### 5. Module Export (`api/tools/mod.rs`)

```rust
mod my_tool;
```

## Tool Categories

### Async (Job-based) Tools
These are slow operations that return a `job_id` immediately:
- `orpheus_loops`
- `orpheus_bridge`
- `musicgen_generate`
- `yue_generate`
- `anticipatory_generate`
- `anticipatory_continue`

### Sync (Direct) Tools
These are fast operations that return results immediately:
- `orpheus_classify` (~100ms)
- `anticipatory_embed` (~200ms)
- `clap_analyze` (~100ms)

## Artifact Tags

Generated artifacts should include appropriate tags:

| Tool | Tags |
|------|------|
| `orpheus_loops` | `type:midi`, `phase:generation`, `tool:orpheus_loops`, `style:drums` |
| `orpheus_classify` | N/A (no artifacts created) |
| `orpheus_bridge` | `type:midi`, `phase:generation`, `tool:orpheus_bridge` |
| `musicgen_generate` | `type:audio`, `format:wav`, `phase:generation`, `tool:musicgen_generate` |
| `yue_generate` | `type:audio`, `format:mp3`, `phase:generation`, `tool:yue_generate`, `has:vocals` |
| `anticipatory_*` | `type:midi`, `phase:generation`, `tool:anticipatory_*` |
| `clap_analyze` | N/A (analysis tool) |

## Error Handling

All HTTP clients must implement robust error handling for GPU service reliability.

### HTTP 429 (GPU Busy) - Retry with Backoff

GPU services return 429 when busy. Implement exponential backoff:

```rust
/// Retry configuration for GPU services
const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 30000;

async fn call_with_retry<F, Fut, T>(
    mut operation: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<reqwest::Response>>,
{
    let mut attempt = 0;
    loop {
        let resp = operation().await?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            attempt += 1;
            if attempt > MAX_RETRIES {
                anyhow::bail!("GPU busy after {} retries", MAX_RETRIES);
            }

            // Use Retry-After header if present, otherwise exponential backoff
            let wait_ms = resp.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(|s| s * 1000)
                .unwrap_or_else(|| {
                    std::cmp::min(
                        INITIAL_BACKOFF_MS * 2_u64.pow(attempt - 1),
                        MAX_BACKOFF_MS
                    )
                });

            tracing::warn!(
                attempt = attempt,
                wait_ms = wait_ms,
                "GPU busy, retrying after {}ms",
                wait_ms
            );

            tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
            continue;
        }

        return Ok(resp);
    }
}
```

### Client Timeouts

Configure timeouts based on tool type:

| Tool Type | Timeout | Rationale |
|-----------|---------|-----------|
| Sync analysis (`clap_analyze`, `orpheus_classify`) | 30s | Fast inference, but allow headroom |
| Async generation (orpheus, musicgen, anticipatory) | 120s | GPU inference varies |
| Long-running (`yue_generate`) | 600s (10 min) | Multi-stage generation |

```rust
// Sync tool client - shorter timeout
let sync_client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(30))
    .build()?;

// Async tool client - standard timeout (job spawns, so less critical)
let async_client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(120))
    .build()?;

// YuE client - very long timeout
let yue_client = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(600))
    .build()?;
```

### Error Response Handling

Always capture error body for debugging:

```rust
if !resp.status().is_success() {
    let status = resp.status();
    let error_body = resp.text().await
        .unwrap_or_else(|_| "<failed to read body>".to_string());

    tracing::error!(
        status = %status,
        body = %error_body,
        "Service request failed"
    );

    anyhow::bail!("Service error {}: {}", status, error_body);
}
```

### Service Unavailable

Handle connection failures gracefully:

```rust
match client.post(url).json(&body).send().await {
    Ok(resp) => { /* process */ }
    Err(e) if e.is_connect() => {
        anyhow::bail!(
            "Service unavailable at {} - is it running? Error: {}",
            url, e
        )
    }
    Err(e) if e.is_timeout() => {
        anyhow::bail!("Service timeout after {}s", timeout_secs)
    }
    Err(e) => anyhow::bail!("HTTP error: {}", e),
}
```

## Dependencies

Some tools require new HTTP clients with specific configurations:

| Client | Port | Timeout | Notes |
|--------|------|---------|-------|
| **OrpheusLoopsClient** | 2003 | 120s | Async, uses retry |
| **OrpheusClassifierClient** | 2001 | 30s | Sync, fast inference |
| **OrpheusBridgeClient** | 2002 | 120s | Async, uses retry |
| **MusicGenClient** | 2006 | 120s | Async, uses retry |
| **ClapClient** | 2007 | 30s | Sync, fast inference |
| **YueClient** | 2008 | 600s | Async, very long |
| **AnticipatoryClient** | 2011 | 120s | Async (gen/continue), 30s (embed) |

These should be added to `EventDualityServer` and initialized in `main.rs`.

## CLI Arguments

Consider adding to `main.rs`:

```rust
#[arg(long, default_value = "2003")]
orpheus_loops_port: u16,

#[arg(long, default_value = "2006")]
musicgen_port: u16,

#[arg(long, default_value = "2007")]
clap_port: u16,

#[arg(long, default_value = "2008")]
yue_port: u16,

#[arg(long, default_value = "2011")]
anticipatory_port: u16,
```

Or use a config file for all service ports.

## Execution Order

Recommended implementation order:

1. **Task 03: orpheus_bridge** - Fix existing stub (smallest change)
2. **Task 01: orpheus_loops** - Similar to existing orpheus tools
3. **Task 02: orpheus_classify** - Simple sync tool
4. **Task 07: clap_analyze** - Useful for all audio work
5. **Task 04: musicgen_generate** - Text-to-audio
6. **Task 06: anticipatory_*** - Three related tools
7. **Task 05: yue_generate** - Most complex (long-running)

## Testing

Each plan includes testing examples. After implementation, verify:

1. Tool appears in `tools()` list
2. Schema validates correctly
3. Service responds at expected port
4. CAS storage works for outputs
5. Artifacts created with correct tags
6. Job system works for async tools
7. OpenTelemetry traces propagate

## Files to Modify

| File | Changes |
|------|---------|
| `schema.rs` | Add request schemas |
| `handler.rs` | Register tools in `tools()` and `call_tool()` |
| `service.rs` | Add new client fields to `EventDualityServer` |
| `main.rs` | Add CLI args, initialize clients |
| `api/tools/mod.rs` | Export new modules |
| `local_models.rs` | Add HTTP client methods (or create new files) |

New files:
- `api/tools/loops.rs`
- `api/tools/classify.rs`
- `api/tools/musicgen.rs`
- `api/tools/yue.rs`
- `api/tools/anticipatory.rs`
- `api/tools/clap.rs`

## Authors

This plan was created by:

🤖 Claude <claude@anthropic.com>

Based on analysis of:
- Running services from `~/src/halfremembered-music-models/`
- Existing tool patterns in `crates/hootenanny/src/api/tools/`
- Service APIs defined in Python

## Review

Reviewed by:

💎 Gemini <gemini@google.com>

Feedback incorporated:
- ✅ Added comprehensive error handling section (429 retry, timeouts, connection failures)
- ⏸️ Client proliferation concern noted - consider generic `ModelClient` during implementation
- ⏸️ Config scalability concern noted - consider `ServiceConfig` struct during implementation
