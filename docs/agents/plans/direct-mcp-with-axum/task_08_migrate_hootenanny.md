# Task 8: Migrate hootenanny to baton

**Objective**: Replace rmcp and custom MCP code in hootenanny with baton.

## Before/After

### Before (current)
```
hootenanny/
├── src/
│   ├── web/
│   │   ├── mcp.rs      # Custom SSE/message handlers
│   │   └── state.rs    # Custom session management
│   ├── api/
│   │   ├── service.rs  # EventDualityServer with rmcp types
│   │   ├── schema.rs   # Request types
│   │   └── tools/      # Tool implementations
│   └── main.rs         # Server setup
```

### After (with baton)
```
hootenanny/
├── src/
│   ├── api/
│   │   ├── handler.rs  # Implements baton::ToolHandler
│   │   ├── schema.rs   # Request types (unchanged)
│   │   └── tools/      # Tool implementations (minimal changes)
│   └── main.rs         # Uses baton::McpServer
```

## Migration Steps

### 1. Update Dependencies

```toml
# Remove:
# rmcp = { git = "...", features = [...] }

# Add:
baton = { path = "../baton" }
```

### 2. Implement ToolHandler

Create `api/handler.rs`:

```rust
use baton::{ToolHandler, Tool, CallToolResult, Content, ErrorData};
use async_trait::async_trait;

pub struct HootToolHandler {
    // Shared state (job store, artifact store, etc.)
    job_store: Arc<JobStore>,
    artifact_store: Arc<FileStore>,
    local_models: Arc<LocalModels>,
    audio_graph: Arc<AudioGraphAdapter>,
    conversation: Arc<Mutex<ConversationState>>,
}

#[async_trait]
impl ToolHandler for HootToolHandler {
    fn tools(&self) -> Vec<Tool> {
        vec![
            Tool::new("play", "Play a musical note with emotional expression")
                .with_schema::<AddNodeRequest>(),
            Tool::new("get_tree_status", "Get the current status of the conversation tree"),
            Tool::new("cas_store", "Store content in the CAS")
                .with_schema::<CasStoreRequest>(),
            // ... rest of tools
        ]
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<CallToolResult, ErrorData> {
        match name {
            "play" => self.play(args).await,
            "get_tree_status" => self.get_tree_status().await,
            "cas_store" => self.cas_store(args).await,
            // ... rest of dispatch
            _ => Err(ErrorData::method_not_found(name)),
        }
    }

    fn server_info(&self) -> baton::Implementation {
        baton::Implementation {
            name: "hootenanny".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn instructions(&self) -> Option<String> {
        Some("Hootenanny is an ensemble performance space for LLM agents and humans to create music together.".to_string())
    }
}
```

### 3. Update Tool Implementations

Change return types from `rmcp::model::CallToolResult` to `baton::CallToolResult`:

```rust
// Before
use rmcp::model::{CallToolResult, Content};

pub async fn play(&self, args: Value) -> Result<CallToolResult, rmcp::ErrorData> {
    // ...
    Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
}

// After
use baton::{CallToolResult, Content, ErrorData};

pub async fn play(&self, args: Value) -> Result<CallToolResult, ErrorData> {
    // ...
    Ok(CallToolResult::success(vec![Content::text(response.to_string())]))
}
```

### 4. Simplify main.rs

```rust
use baton::McpServer;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    // ... initialization (CAS, job store, etc.)

    // Create tool handler with all dependencies
    let handler = Arc::new(HootToolHandler::new(
        job_store,
        artifact_store,
        local_models,
        audio_graph,
        conversation_state,
    ));

    // Build MCP server
    let mcp_server = McpServer::builder()
        .handler(handler)
        .session_ttl(Duration::from_secs(30 * 60))
        .build();

    // Create router
    let app = Router::new()
        .nest("/mcp", mcp_server.router())
        .merge(cas_router);  // Keep CAS routes

    // Spawn cleanup task
    let shutdown = CancellationToken::new();
    mcp_server.spawn_cleanup_task(shutdown.clone());

    // Run server
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await?;

    Ok(())
}
```

### 5. Remove Obsolete Code

Delete:
- `src/web/mcp.rs` - replaced by baton transport
- `src/web/state.rs` - replaced by baton session
- `src/api/service.rs` - replaced by `api/handler.rs`

Update:
- `src/web.rs` - remove mcp/state modules, keep CAS routes
- `src/lib.rs` - remove web::mcp, web::state exports

### 6. Update Tests

Integration tests should work with minimal changes since the MCP protocol is the same.

Update imports in test files:
```rust
// The test client stays the same, it speaks MCP protocol
```

## Migration Checklist

- [ ] Add baton dependency
- [ ] Create `api/handler.rs` implementing `ToolHandler`
- [ ] Update tool return types to `baton::CallToolResult`
- [ ] Update `main.rs` to use `McpServer`
- [ ] Delete `web/mcp.rs`
- [ ] Delete `web/state.rs`
- [ ] Delete `api/service.rs`
- [ ] Update `web.rs` (keep CAS only)
- [ ] Remove rmcp from `Cargo.toml`
- [ ] Run all tests
- [ ] Verify with real MCP client

## Success Criteria

- `cargo build -p hootenanny` succeeds without rmcp
- `cargo test -p hootenanny` passes
- Integration tests pass
- Real MCP clients (Claude, etc.) can connect and use tools
- Telemetry shows rich spans in OTLP collector
