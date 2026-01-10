# 07-mcp: MCP Tools

**File:** `crates/vibeweaver/src/mcp.rs`
**Dependencies:** 04-scheduler, 05-api, 06-broadcast
**Unblocks:** None

---

## Task

Expose MCP tools for AI agents to interact with vibeweaver through hootenanny.

## Deliverables

- `crates/vibeweaver/src/mcp.rs`
- Tool registration with hootenanny
- Response formatting

## Types

```rust
use serde::{Deserialize, Serialize};
use anyhow::Result;

// --- Request types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaveEvalRequest {
    /// Python code to execute
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaveSessionRequest {
    /// Session ID (optional, uses current if not specified)
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaveResetRequest {
    /// If true, also clear session data
    pub clear_session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaveHelpRequest {
    /// Topic: "api", "session", "scheduler", "examples", or None for overview
    pub topic: Option<String>,
}

// --- Response types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaveEvalResponse {
    /// Python repr of result (if any)
    pub result: Option<String>,
    /// Stdout captured during execution
    pub stdout: String,
    /// Stderr captured during execution
    pub stderr: String,
    /// True if execution succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaveSessionResponse {
    pub session_id: String,
    pub session_name: String,
    pub vibe: Option<String>,
    pub tempo_bpm: f64,
    pub active_rules: Vec<RuleSummary>,
    pub markers: Vec<MarkerSummary>,
    pub recent_history: Vec<HistorySummary>,
    pub transport_state: String,
    pub current_beat: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSummary {
    pub id: String,
    pub trigger: String,
    pub action: String,
    pub priority: String,
    pub enabled: bool,
    pub fired_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkerSummary {
    pub name: String,
    pub beat: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySummary {
    pub action: String,
    pub success: bool,
    pub timestamp: String,
}

// --- Tool handlers ---

pub async fn weave_eval(request: WeaveEvalRequest) -> Result<WeaveEvalResponse>;
pub async fn weave_session(request: WeaveSessionRequest) -> Result<WeaveSessionResponse>;
pub async fn weave_reset(request: WeaveResetRequest) -> Result<()>;
pub async fn weave_help(request: WeaveHelpRequest) -> Result<String>;

// --- Tool definitions for registration ---

pub fn tool_definitions() -> Vec<ToolInfo> {
    vec![
        ToolInfo {
            name: "weave_eval",
            description: "Execute Python code in vibeweaver kernel with persistent state",
            schema: weave_eval_schema(),
        },
        ToolInfo {
            name: "weave_session",
            description: "Get current session state including rules, markers, and history",
            schema: weave_session_schema(),
        },
        ToolInfo {
            name: "weave_reset",
            description: "Reset kernel state (optionally clear session)",
            schema: weave_reset_schema(),
        },
        ToolInfo {
            name: "weave_help",
            description: "Get vibeweaver documentation",
            schema: weave_help_schema(),
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: serde_json::Value,
}
```

## Help Content

```
weave_help(topic: None) -> Overview of vibeweaver
weave_help(topic: "api") -> Python API reference
weave_help(topic: "session") -> Session management
weave_help(topic: "scheduler") -> Rule system explained
weave_help(topic: "examples") -> Common patterns
```

## Integration

Vibeweaver registers as a worker with hootenanny on startup:
1. Connect DEALER socket
2. Send `READY` with tool definitions
3. Process `REQUEST` messages, send `RESPONSE`

## Definition of Done

```bash
cargo fmt --check -p vibeweaver
cargo clippy -p vibeweaver -- -D warnings
cargo test -p vibeweaver mcp::
```

## Acceptance Criteria

- [ ] `weave_eval("1 + 1")` returns `{"result": "2", "success": true}`
- [ ] `weave_session()` returns full session summary
- [ ] `weave_reset()` clears kernel, optionally session
- [ ] `weave_help()` returns useful documentation
- [ ] Tools register with hootenanny on startup
