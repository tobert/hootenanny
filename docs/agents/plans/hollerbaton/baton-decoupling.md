# Plan: Decouple Baton from Hootenanny (Final Phase)

## Status: Ready to Implement

This is the final phase of MCP removal from hootenanny. Previous work removed the MCP server and routing, but `EventDualityServer` methods still return `baton::CallToolResult`. This plan replaces those with hooteproto types, making hootenanny a pure ZMQ backend with zero baton dependency.

## Goal

Remove all baton types from hootenanny. baton stays pure MCP, hooteproto holds shared Hootenanny types.

## Architecture

```
+-------------------------------------------------------------+
| baton (pure MCP)                                            |
| - MCP protocol types (CallToolResult, ErrorData, etc.)      |
| - Generic schemars helper: schema_for<T: JsonSchema>()      |
| - No Hootenanny knowledge                                   |
+-------------------------------------------------------------+
                           |
                           v
+-------------------------------------------------------------+
| hooteproto (Hootenanny protocol)                            |
| - Payload enum (tool messages over ZMQ)                     |
| - ToolInfo with input_schema: Value                         |
| - Request types with #[derive(JsonSchema)]                  |
| - ToolOutput, ToolResult, ToolError (new)                   |
| - Broadcast::Progress (new)                                 |
+-------------------------------------------------------------+
                           |
                           v
+-------------------------------------------------------------+
| holler (MCP frontend)                                       |
| - Uses baton for MCP transport                              |
| - Uses hooteproto types                                     |
| - Generates tool list: baton::schema_for::<CasStore>()      |
| - Integration tests with strict schema validation           |
+-------------------------------------------------------------+
```

## Decisions

- **sampling.rs**: Remove entirely - only makes sense for MCP clients
- **Progress notifications**: Add `Broadcast::Progress` to hooteproto
- **schemars**: baton exposes generic helper, hooteproto types derive JsonSchema
- **ToolResult types**: Live in hooteproto as the shared contract

---

## Phase 1: Add schemars helper to baton

Add generic schema generation helper:

```rust
// crates/baton/src/lib.rs (or schema.rs)
use schemars::JsonSchema;

/// Generate JSON Schema for a type as serde_json::Value
///
/// Used by MCP servers to build tool input schemas:
/// ```
/// let schema = baton::schema_for::<MyRequest>();
/// ```
pub fn schema_for<T: JsonSchema>() -> serde_json::Value {
    let schema = schemars::schema_for!(T);
    serde_json::to_value(schema).expect("schema serialization cannot fail")
}
```

Add schemars to baton's Cargo.toml as a re-export or peer dependency.

---

## Phase 2: Add tool result types to hooteproto

Add to `crates/hooteproto/src/lib.rs`:

```rust
// ============================================================================
// Tool Result Types (used by hootenanny, returned over ZMQ)
// ============================================================================

/// Successful tool output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Human-readable summary
    pub text: String,
    /// Structured data for programmatic use
    pub data: serde_json::Value,
}

impl ToolOutput {
    pub fn new(text: impl Into<String>, data: impl Serialize) -> Self {
        Self {
            text: text.into(),
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
        }
    }

    pub fn text_only(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            data: serde_json::Value::Null,
        }
    }
}

/// Tool execution error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolError {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl ToolError {
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self { code: "invalid_params".into(), message: msg.into(), details: None }
    }

    pub fn not_found(tool: &str) -> Self {
        Self { code: "tool_not_found".into(), message: format!("Unknown tool: {}", tool), details: None }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self { code: "internal_error".into(), message: msg.into(), details: None }
    }
}

/// Result type for tool execution
pub type ToolResult = Result<ToolOutput, ToolError>;
```

---

## Phase 3: Add Progress to Broadcast enum

Add to hooteproto's `Broadcast` enum:

```rust
/// Progress update for long-running operations
Progress {
    job_id: String,
    /// 0.0 to 1.0
    percent: f32,
    message: String,
},
```

---

## Phase 4: Add JsonSchema derives to hooteproto request types

Add schemars dependency to hooteproto and derive JsonSchema on Payload variants that represent tool requests. This allows holler to generate accurate MCP schemas.

```toml
# crates/hooteproto/Cargo.toml
[dependencies]
schemars = "1.0"
```

For each tool request in Payload, add the derive. Example:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrpheusGenerateParams {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    // ... etc
}
```

Note: The Payload enum variants themselves don't need JsonSchema - we create separate param structs that mirror them for schema generation.

---

## Phase 5: Update hootenanny tool implementations

Transform each tool file to use hooteproto types:

```rust
// Before (crates/hootenanny/src/api/tools/cas.rs)
use baton::{CallToolResult, Content, ErrorData as McpError};

pub async fn cas_store(&self, request: CasStoreRequest) -> Result<CallToolResult, McpError> {
    // ...
    Ok(CallToolResult::success(vec![Content::text("done")])
        .with_structured(serde_json::to_value(&response)?))
}

// After
use hooteproto::{ToolOutput, ToolResult, ToolError};

pub async fn cas_store(&self, request: CasStoreRequest) -> ToolResult {
    // ...
    Ok(ToolOutput::new(
        format!("Stored {} bytes as {}", response.size_bytes, hash),
        &response,
    ))
}
```

Files to update:
- `api/tools/abc.rs`
- `api/tools/beat_this.rs`
- `api/tools/cas.rs`
- `api/tools/clap.rs`
- `api/tools/garden.rs`
- `api/tools/graph_context.rs`
- `api/tools/graph_query.rs`
- `api/tools/jobs.rs`
- `api/tools/musicgen.rs`
- `api/tools/orpheus.rs`
- `api/tools/yue.rs`

---

## Phase 6: Simplify dispatch.rs

Remove baton bridge, work directly with hooteproto types:

```rust
// Before
fn tool_result_to_json(result: Result<baton::CallToolResult, baton::ErrorData>) -> DispatchResult

// After - delete tool_result_to_json entirely
// Tools already return ToolResult, dispatch converts to Payload directly:

match dispatch_tool(server, &tool_name, args).await {
    Ok(output) => Payload::Success {
        result: serde_json::json!({
            "text": output.text,
            "data": output.data,
        })
    },
    Err(e) => Payload::Error {
        code: e.code,
        message: e.message,
        details: e.details,
    },
}
```

---

## Phase 7: Update holler to use baton's schema helper

In holler's tool list generation:

```rust
use baton::schema_for;
use hooteproto::{OrpheusGenerateParams, CasStoreParams, /* etc */};

fn build_tool_list() -> Vec<ToolInfo> {
    vec![
        ToolInfo {
            name: "orpheus_generate".into(),
            description: "Generate MIDI with Orpheus model".into(),
            input_schema: schema_for::<OrpheusGenerateParams>(),
        },
        // ... etc
    ]
}
```

---

## Phase 8: Cleanup

1. Delete `hootenanny/src/api/tools/sampling.rs`
2. Update `hootenanny/src/api/tools/mod.rs` (remove sampling)
3. Remove schemars derives from hootenanny's schema.rs/responses.rs (if not needed)
4. Delete `hootenanny/tests/schema_validation.rs`
5. Remove from hootenanny's Cargo.toml:
   - `baton = { path = "../baton" }`
   - `schemars` (if not needed elsewhere)

---

## Phase 9: Add holler integration tests

Create integration tests that validate:
1. Tool schemas match expected JSON Schema structure
2. Round-trip: MCP request -> ZMQ -> hootenanny -> ZMQ -> MCP response
3. Error responses have correct structure

```rust
// crates/holler/tests/schema_validation.rs
#[test]
fn tool_schemas_are_valid_json_schema() {
    let tools = build_tool_list();
    for tool in tools {
        // Validate schema is well-formed
        let schema: schemars::schema::RootSchema =
            serde_json::from_value(tool.input_schema.clone())
                .expect(&format!("{} has invalid schema", tool.name));

        // Could also validate against JSON Schema meta-schema
    }
}
```

---

## Files Summary

| Action | Crate | File |
|--------|-------|------|
| Modify | baton | `src/lib.rs` (add schema_for helper) |
| Modify | baton | `Cargo.toml` (add schemars) |
| Modify | hooteproto | `src/lib.rs` (ToolOutput, ToolError, ToolResult, Broadcast::Progress) |
| Modify | hooteproto | `Cargo.toml` (add schemars) |
| Modify | hootenanny | `api/dispatch.rs` |
| Modify | hootenanny | `api/tools/*.rs` (11 files) |
| Modify | hootenanny | `api/tools/mod.rs` |
| Modify | hootenanny | `Cargo.toml` (remove baton) |
| Delete | hootenanny | `api/tools/sampling.rs` |
| Delete | hootenanny | `tests/schema_validation.rs` |
| Create | holler | `tests/schema_validation.rs` |

---

## Order of Operations

1. **baton**: Add `schema_for<T>()` helper + schemars dep
2. **hooteproto**: Add ToolOutput, ToolError, ToolResult, Broadcast::Progress
3. **hooteproto**: Add schemars dep, create param structs with JsonSchema
4. **hootenanny**: Update tool files one by one (can batch)
5. **hootenanny**: Simplify dispatch.rs
6. **hootenanny**: Delete sampling.rs, update mod.rs
7. **hootenanny**: Remove baton from Cargo.toml
8. **hootenanny**: Delete tests/schema_validation.rs
9. **holler**: Update to use baton::schema_for with hooteproto types
10. **holler**: Add integration tests
11. `cargo test --workspace` to verify everything
