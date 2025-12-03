# Phase 1: Progress Notifications

## Overview

Replace the polling pattern with push-based progress notifications. This is the biggest infrastructure change because it touches the dispatch layer, session management, and all async job-returning tools.

**MCP Methods**:
- Request metadata: `_meta.progressToken` in any request
- Notification: `notifications/progress`

**Impact**: High - changes how agents interact with long-running operations

## Current State

Today, async operations work like this:
```
Agent: orpheus_generate({temperature: 1.0})
Server: {job_id: "job_abc123", status: "pending"}
Agent: job_poll({job_ids: ["job_abc123"], timeout_ms: 5000})
Server: {completed: [], pending: ["job_abc123"]}
Agent: job_poll({job_ids: ["job_abc123"], timeout_ms: 5000})
Server: {completed: [{job_id: "job_abc123", artifact_id: "..."}], pending: []}
```

This requires the agent to repeatedly call `job_poll`, consuming context and adding latency.

## Target State

With progress notifications:
```
Agent: orpheus_generate({temperature: 1.0}, _meta: {progressToken: "tok123"})
Server: [starts job, sends notifications]
Server → Agent: notifications/progress {progressToken: "tok123", progress: 0.25, message: "Tokenizing..."}
Server → Agent: notifications/progress {progressToken: "tok123", progress: 0.75, message: "Generating..."}
Server: {artifact_id: "...", job_id: "job_abc123"}  [final response]
```

The agent gets live updates without polling. `job_poll` remains available for clients that don't support progress.

## Implementation Plan

### Step 1: Add Progress Types to Baton

**File**: `crates/baton/src/types/progress.rs` (new)

```rust
/// Progress notification parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressNotification {
    /// Token linking progress to the original request
    pub progress_token: ProgressToken,

    /// Progress value (0.0 to 1.0, or absolute if total is known)
    pub progress: f64,

    /// Optional total for absolute progress
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<f64>,

    /// Human-readable status message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Progress token (string or integer per MCP spec)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum ProgressToken {
    String(String),
    Integer(i64),
}

/// Request metadata containing progress token
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_token: Option<ProgressToken>,
}
```

### Step 2: Add Progress Sender to Handler Context

Modify the `Handler` trait to accept a progress sender for tool calls:

**File**: `crates/baton/src/protocol/mod.rs`

```rust
/// Context passed to tool calls for sending progress
pub struct ToolContext {
    /// Session ID
    pub session_id: String,
    /// Progress token (if client requested progress)
    pub progress_token: Option<ProgressToken>,
    /// Sender for progress notifications
    pub progress_sender: Option<ProgressSender>,
}

/// Sender for progress notifications
pub type ProgressSender = tokio::sync::mpsc::Sender<ProgressNotification>;

#[async_trait]
pub trait Handler: Send + Sync + 'static {
    // Existing methods...

    /// Execute a tool call with context for progress reporting
    async fn call_tool_with_context(
        &self,
        name: &str,
        arguments: Value,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        // Default: ignore context, call the simpler method
        self.call_tool(name, arguments).await
    }
}
```

### Step 3: Wire Progress into Dispatch

**File**: `crates/baton/src/protocol/mod.rs`

In `handle_call_tool`:
1. Extract `_meta.progressToken` from request params
2. Create a progress channel if token present
3. Spawn task to forward progress to SSE/response stream
4. Pass context to `call_tool_with_context`

```rust
async fn handle_call_tool<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    let params: CallToolParams = /* ... */;

    // Extract progress token from _meta
    let progress_token = request.params
        .as_ref()
        .and_then(|p| p.get("_meta"))
        .and_then(|m| m.get("progressToken"))
        .and_then(|t| serde_json::from_value::<ProgressToken>(t.clone()).ok());

    let (progress_tx, mut progress_rx) = if progress_token.is_some() {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    // Spawn progress forwarder if we have a token
    if let (Some(token), Some(mut rx)) = (progress_token.clone(), progress_rx) {
        let session_tx = state.sessions.get(session_id)
            .and_then(|s| s.tx.clone());

        tokio::spawn(async move {
            while let Some(progress) = rx.recv().await {
                let notification = JsonRpcMessage::notification(
                    "notifications/progress",
                    serde_json::to_value(&progress).unwrap(),
                );
                if let Some(ref tx) = session_tx {
                    let _ = tx.send(notification).await;
                }
            }
        });
    }

    let context = ToolContext {
        session_id: session_id.to_string(),
        progress_token,
        progress_sender: progress_tx,
    };

    state.handler.call_tool_with_context(&params.name, arguments, context).await
}
```

### Step 4: Update HootHandler to Use Progress

**File**: `crates/hootenanny/src/api/handler.rs`

Implement `call_tool_with_context` and pass progress to job-spawning tools:

```rust
async fn call_tool_with_context(
    &self,
    name: &str,
    args: Value,
    context: ToolContext,
) -> Result<CallToolResult, ErrorData> {
    match name {
        "orpheus_generate" => {
            let request: OrpheusGenerateRequest = serde_json::from_value(args)
                .map_err(|e| ErrorData::invalid_params(e.to_string()))?;
            self.server.orpheus_generate_with_progress(request, context.progress_sender).await
        }
        // ... other job-spawning tools
        _ => self.call_tool(name, args).await // Fallback for tools without progress
    }
}
```

### Step 5: Add Progress Reporting to Job Manager

**File**: `crates/hootenanny/src/job_manager.rs` (or wherever jobs are managed)

Add progress callback to job execution:

```rust
pub struct JobOptions {
    pub progress_sender: Option<ProgressSender>,
}

impl JobManager {
    pub async fn spawn_with_progress<F, T>(
        &self,
        name: &str,
        progress: Option<ProgressSender>,
        future: F,
    ) -> JobId
    where
        F: Future<Output = Result<T, Error>> + Send + 'static,
        T: Into<JobResult>,
    {
        // Wrap future to report progress at key points
        // ...
    }
}
```

### Step 6: Update All Job-Spawning Tools

Tools that spawn async jobs:
- `orpheus_generate`
- `orpheus_generate_seeded`
- `orpheus_continue`
- `orpheus_bridge`
- `orpheus_loops`
- `convert_midi_to_wav`
- `musicgen_generate`
- `anticipatory_generate`
- `anticipatory_continue`
- `yue_generate`
- `clap_analyze`
- `beatthis_analyze`

For each:
1. Accept progress sender in the service method
2. Report progress at meaningful points (tokenizing, generating, finalizing)
3. Ensure final response includes the result directly (not just job_id)

### Step 7: Unit Tests

**File**: `crates/baton/src/protocol/progress_tests.rs` (new)

```rust
#[tokio::test]
async fn test_progress_token_extraction() { /* ... */ }

#[tokio::test]
async fn test_progress_notifications_sent() { /* ... */ }

#[tokio::test]
async fn test_no_progress_without_token() { /* ... */ }
```

### Step 8: Live Testing

1. Rebuild hootenanny: `cargo build --release`
2. Prompt human to restart MCP connection
3. Call a generation tool with progress token
4. Verify progress notifications arrive before final result

## Files Changed

| File | Change |
|------|--------|
| `crates/baton/src/types/mod.rs` | Export progress module |
| `crates/baton/src/types/progress.rs` | New - progress types |
| `crates/baton/src/protocol/mod.rs` | Add ToolContext, progress dispatch |
| `crates/hootenanny/src/api/handler.rs` | Implement call_tool_with_context |
| `crates/hootenanny/src/api/service.rs` | Add _with_progress methods |
| Various Orpheus/generation backends | Report progress at key points |

## Verification Checklist

- [ ] Progress types compile and serialize correctly
- [ ] ToolContext flows through dispatch
- [ ] Progress notifications sent for tools with tokens
- [ ] Tools without tokens still work (no regression)
- [ ] `job_poll` still works for fallback
- [ ] Live test with Claude Code shows progress
- [ ] Unit tests pass

## Notes for Next Agent

After this phase:
- The dispatch layer understands `_meta.progressToken`
- All job-spawning tools report progress
- `job_poll` remains functional but should be rarely needed
- You understand baton's dispatch architecture deeply

This sets up Phase 2 (output schemas) which will add structured responses to the results these tools return.
