# Phase 5: Logging

## Overview

Implement structured logging from server to client. This allows the MCP server to stream debug info, warnings, and status messages without cluttering tool results.

**MCP Methods**:
- `logging/setLevel` - Client sets desired log level
- `notifications/message` - Server sends log messages

**Impact**: Low-Medium - improves debugging and observability

## Current State

Baton has `LoggingCapability` defined and can be enabled, but:
- No `logging/setLevel` handler
- No mechanism to emit `notifications/message`
- No integration with Rust's `tracing` infrastructure

## Target State

Client sets log level:
```json
{"method": "logging/setLevel", "params": {"level": "info"}}
```

Server sends logs:
```json
{
  "method": "notifications/message",
  "params": {
    "level": "info",
    "logger": "hootenanny::orpheus",
    "message": "Starting generation with temperature 1.0"
  }
}
```

## Implementation Plan

### Step 1: Add Logging Types to Baton

**File**: `crates/baton/src/types/logging.rs` (new)

```rust
//! Logging Types
//!
//! Types for structured logging to MCP clients.
//! Per MCP 2025-06-18 schema.

use serde::{Deserialize, Serialize};

/// Log levels (matching syslog severity)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

impl From<tracing::Level> for LogLevel {
    fn from(level: tracing::Level) -> Self {
        match level {
            tracing::Level::TRACE | tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warning,
            tracing::Level::ERROR => LogLevel::Error,
        }
    }
}

/// Set log level request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLevelParams {
    pub level: LogLevel,
}

/// Log message notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    /// Severity level
    pub level: LogLevel,

    /// Logger name (e.g., module path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,

    /// The log message
    #[serde(rename = "data")]
    pub message: serde_json::Value,
}

impl LogMessage {
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            logger: None,
            message: serde_json::Value::String(message.into()),
        }
    }

    pub fn with_logger(mut self, logger: impl Into<String>) -> Self {
        self.logger = Some(logger.into());
        self
    }

    pub fn debug(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Debug, message)
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Info, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Warning, message)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(LogLevel::Error, message)
    }
}
```

### Step 2: Add Log Level to Session

**File**: `crates/baton/src/session/mod.rs`

```rust
pub struct Session {
    // ... existing fields
    /// Client's requested log level
    pub log_level: LogLevel,
}

impl Session {
    /// Check if a message at this level should be sent
    pub fn should_log(&self, level: LogLevel) -> bool {
        level >= self.log_level
    }
}
```

### Step 3: Add Logging Handler to Dispatch

**File**: `crates/baton/src/protocol/mod.rs`

```rust
async fn dispatch_inner<H: Handler>(/* ... */) -> Result<Value, ErrorData> {
    match message.method.as_str() {
        // ... existing handlers ...

        "logging/setLevel" => handle_set_log_level(state, session_id, message).await,

        // ...
    }
}

async fn handle_set_log_level<H: Handler>(
    state: &Arc<McpState<H>>,
    session_id: &str,
    request: &JsonRpcMessage,
) -> Result<Value, ErrorData> {
    let params: SetLevelParams = request
        .params
        .as_ref()
        .map(|p| serde_json::from_value(p.clone()))
        .transpose()
        .map_err(|e| ErrorData::invalid_params(e.to_string()))?
        .ok_or_else(|| ErrorData::invalid_params("Missing setLevel params"))?;

    if let Some(mut session) = state.sessions.get_mut(session_id) {
        session.log_level = params.level;
        tracing::info!(
            session_id = %session_id,
            level = ?params.level,
            "Log level set"
        );
    }

    Ok(serde_json::json!({}))
}
```

### Step 4: Add Logger to McpState

**File**: `crates/baton/src/transport/mod.rs`

```rust
/// Logger for sending messages to MCP clients
pub struct McpLogger {
    sessions: Arc<dyn SessionStore>,
}

impl McpLogger {
    /// Send a log message to a specific session
    pub async fn log(&self, session_id: &str, message: LogMessage) {
        if let Some(session) = self.sessions.get(session_id) {
            if !session.should_log(message.level) {
                return;
            }

            if let Some(tx) = &session.tx {
                let notification = JsonRpcMessage::notification(
                    "notifications/message",
                    serde_json::to_value(&message).unwrap_or_default(),
                );
                let _ = tx.send(notification).await;
            }
        }
    }

    /// Send to all sessions at or above their log level
    pub async fn log_all(&self, message: LogMessage) {
        for session_id in self.sessions.all_session_ids() {
            self.log(&session_id, message.clone()).await;
        }
    }
}
```

### Step 5: Add Logger to ToolContext

**File**: `crates/baton/src/protocol/mod.rs`

```rust
pub struct ToolContext {
    pub session_id: String,
    pub progress_token: Option<ProgressToken>,
    pub progress_sender: Option<ProgressSender>,
    pub sampler: Option<Sampler>,
    /// Logger for this session
    pub logger: McpLogger,
}

impl ToolContext {
    pub async fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.logger.log(&self.session_id, LogMessage::new(level, message)).await;
    }

    pub async fn log_info(&self, message: impl Into<String>) {
        self.log(LogLevel::Info, message).await;
    }

    pub async fn log_debug(&self, message: impl Into<String>) {
        self.log(LogLevel::Debug, message).await;
    }

    pub async fn log_warning(&self, message: impl Into<String>) {
        self.log(LogLevel::Warning, message).await;
    }
}
```

### Step 6: Use Logging in Hootenanny Tools

**File**: `crates/hootenanny/src/api/service.rs`

```rust
impl EventDualityServer {
    pub async fn orpheus_generate_with_context(
        &self,
        request: OrpheusGenerateRequest,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        context.log_info(format!(
            "Starting Orpheus generation: model={}, temp={}",
            request.model.as_deref().unwrap_or("base"),
            request.temperature.unwrap_or(1.0)
        )).await;

        let job_id = self.spawn_orpheus_job(&request).await?;

        context.log_debug(format!("Job spawned: {}", job_id)).await;

        // ... rest of implementation
    }

    pub async fn cas_store_with_context(
        &self,
        request: CasStoreRequest,
        context: ToolContext,
    ) -> Result<CallToolResult, ErrorData> {
        let size = request.content_base64.len();
        context.log_debug(format!("Storing {} bytes in CAS", size)).await;

        let hash = self.cas.store(&request).await?;

        context.log_info(format!("Stored as {}", hash)).await;

        // ...
    }
}
```

### Step 7: Optional - Tracing Integration

Create a tracing layer that forwards to MCP logging:

**File**: `crates/baton/src/logging/tracing_layer.rs` (new)

```rust
use tracing_subscriber::Layer;

/// Tracing layer that forwards events to MCP clients
pub struct McpTracingLayer {
    logger: Arc<McpLogger>,
}

impl<S> Layer<S> for McpTracingLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let level = LogLevel::from(*event.metadata().level());
        let logger_name = event.metadata().target().to_string();

        // Extract message from event
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let message = LogMessage::new(level, visitor.message)
            .with_logger(logger_name);

        // Fire and forget - async in sync context
        let logger = self.logger.clone();
        tokio::spawn(async move {
            logger.log_all(message).await;
        });
    }
}
```

This is optional but powerful - it means existing `tracing::info!()` calls automatically forward to MCP clients.

### Step 8: Enable Logging Capability

**File**: `crates/hootenanny/src/api/handler.rs`

```rust
fn capabilities(&self) -> ServerCapabilities {
    ServerCapabilities::default()
        .enable_tools()
        .enable_resources()
        .enable_prompts()
        .enable_completions()
        .enable_logging()  // Add this
}
```

### Step 9: Unit Tests

**File**: `crates/baton/src/types/logging_tests.rs`

```rust
#[test]
fn test_log_level_ordering() {
    assert!(LogLevel::Error > LogLevel::Warning);
    assert!(LogLevel::Warning > LogLevel::Info);
    assert!(LogLevel::Info > LogLevel::Debug);
}

#[test]
fn test_log_message_serialization() {
    let msg = LogMessage::info("Test message")
        .with_logger("test::module");

    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["level"], "info");
    assert_eq!(json["logger"], "test::module");
    assert_eq!(json["data"], "Test message");
}

#[test]
fn test_should_log() {
    let mut session = Session::new("test".to_string());
    session.log_level = LogLevel::Warning;

    assert!(session.should_log(LogLevel::Error));
    assert!(session.should_log(LogLevel::Warning));
    assert!(!session.should_log(LogLevel::Info));
    assert!(!session.should_log(LogLevel::Debug));
}
```

### Step 10: Live Testing

1. Rebuild and reconnect MCP
2. Send `logging/setLevel` with level "debug"
3. Call various tools
4. Verify log messages appear in client
5. Change level to "warning"
6. Verify debug/info messages no longer appear

## Files Changed

| File | Change |
|------|--------|
| `crates/baton/src/types/logging.rs` | New - logging types |
| `crates/baton/src/types/mod.rs` | Export logging |
| `crates/baton/src/session/mod.rs` | Add log_level to Session |
| `crates/baton/src/transport/mod.rs` | Add McpLogger |
| `crates/baton/src/protocol/mod.rs` | Handle setLevel, add logger to context |
| `crates/hootenanny/src/api/handler.rs` | Enable logging capability |
| `crates/hootenanny/src/api/service.rs` | Add logging to tools |

## Verification Checklist

- [ ] Logging types compile
- [ ] LoggingCapability advertised
- [ ] logging/setLevel changes session level
- [ ] notifications/message sent for logs
- [ ] Level filtering works correctly
- [ ] Tools emit useful log messages
- [ ] Unit tests pass
- [ ] Live test shows logs in client

## Notes for Next Agent

After this phase:
- Server can stream logs to clients
- Clients can control verbosity
- Tools emit debug info without cluttering results
- Optional tracing integration available

Phase 6 (resource subscriptions) enables push updates for resource changes. Similar infrastructure to progress notifications.
