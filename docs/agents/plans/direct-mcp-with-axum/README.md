# Direct MCP Implementation Plan

**Goal**: Create `baton`, a reusable MCP server crate that handles the protocol layer, allowing `hootenanny` to focus on business logic.

**Why**:
1. **Session Resilience**: Full control over session lifecycles for server restarts and network blips
2. **Observability**: Rich OpenTelemetry spans for every MCP operation
3. **Simplicity**: Clean separation between protocol (baton) and application (hootenanny)
4. **Reusability**: Other projects can use baton for MCP server needs
5. **Debuggability**: Own types, own control - no rmcp macro magic

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         hootenanny                                   │
├─────────────────────────────────────────────────────────────────────┤
│  main.rs              │  api/handler.rs        │  api/tools/*       │
│  └─ compose routers   │  HootToolHandler       │  └─ tool impls     │
│                       │  └─ impl ToolHandler   │                    │
├─────────────────────────────────────────────────────────────────────┤
│                            baton                                     │
├─────────────────────────────────────────────────────────────────────┤
│  transport/           │  protocol/             │  types/            │
│  ├─ sse_handler       │  ├─ dispatch           │  ├─ JsonRpc*       │
│  ├─ message_handler   │  ├─ initialize         │  ├─ Tool           │
│  └─ router()          │  └─ tool routing       │  ├─ Content        │
│                       │                        │  └─ Error          │
│  session/             │  telemetry/            │                    │
│  ├─ Session           │  ├─ spans              │                    │
│  ├─ SessionStore      │  └─ attributes         │                    │
│  └─ TTL cleanup       │                        │                    │
├─────────────────────────────────────────────────────────────────────┤
│                            axum                                      │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                         hrcli (client)                               │
├─────────────────────────────────────────────────────────────────────┤
│                            rmcp                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Composable HTTP Services

`baton` returns a standard `axum::Router`, making it easy to run multiple services on one port:

```rust
// hootenanny main.rs
let app = Router::new()
    // MCP protocol (from baton)
    .nest("/mcp", mcp_server.router())

    // Content-addressable storage
    .route("/cas", post(upload_cas))
    .route("/cas/:hash", get(download_cas))

    // Future: REST API for web UI
    .nest("/api/v1", api_router)

    // Future: WebSocket for live MIDI/audio streaming
    .route("/ws/session/:id", get(websocket_handler))

    // Health/metrics
    .route("/health", get(health_check))
    .route("/metrics", get(prometheus_metrics))

    // Shared middleware
    .layer(TraceLayer::new_for_http())
    .layer(CorsLayer::permissive());
```

**Benefits:**
- Single port for all services (simpler deployment, firewall rules)
- Shared middleware (auth, tracing, CORS)
- MCP tools can reference other endpoints (e.g., return CAS URLs)
- Easy to add new services without touching baton

## What Lives Where

### baton (new crate)
| Module | Contents |
|--------|----------|
| `types/` | JSON-RPC, MCP protocol types, Tool, Content, Error |
| `transport/` | SSE handler, message handler, axum router builder |
| `session/` | Session, SessionStore trait, in-memory impl, TTL cleanup |
| `protocol/` | Method dispatch, initialize/list/call handlers |
| `telemetry/` | Span creation, standard attributes, context propagation |
| `server/` | `McpServer` builder, configuration |

### hootenanny (uses baton)
| Module | Contents |
|--------|----------|
| `api/service.rs` | `EventDualityServer` - implements `baton::ToolHandler` |
| `api/tools/*` | Individual tool implementations |
| `api/schema.rs` | Request/response types for tools |
| `main.rs` | Server setup, wiring |

## Task Index

### Completed ✅
- [x] Task 1-5: Initial transport refactor (now being replaced by baton)
- [x] [Task 7: baton Crate](task_07_baton.md) - Full MCP server with dual transport (41 tests)
- [x] [Task 8: Migrate hootenanny](task_08_migrate_hootenanny.md) - Using baton, rmcp removed (35 tests)
- [x] [Task 9: Streamable HTTP](task_09_baton_progress.md) - Claude Code compatible transport
- [x] inputSchema - All 17 tools return proper JSON Schema via schemars
- [x] Graph tools - graph_bind, graph_tag, graph_connect, graph_find wired to audio_graph_db
- [x] hrcli SSE fix - Fixed chunk accumulation, discover now works

### Recently Completed
- [x] [Task 6: Integration Testing](task_06_integration_test.md) - 5 tests passing including session resumption
- [x] Task 10: Resources API - graph://, cas://, artifacts:// URIs working
- [x] Task 11: Prompts API - ensemble-jam, describe-setup, patch-synth, sequence-idea
- [x] Task 12: Cleanup - Removed conversation tree/emotional systems (play, add_node, fork_branch)

### Deferred
- [ ] Rich telemetry span builders

### Removed (no longer applicable)
- session:// resources (conversation tree removed)
- Emotional prompts (emotional-arc, mood-transition, etc.)

---

## Task 10: Resources API ✅

Expose read-only content via URIs for clients to discover and fetch.

### Resources Implemented

| URI Pattern | Description | MIME Type |
|-------------|-------------|-----------|
| `graph://identities` | All audio device identities with tags | `application/json` |
| `graph://connections` | All patch cable connections | `application/json` |
| `graph://identity/{id}` | Single identity with hints and tags | `application/json` |
| `cas://{hash}` | Content from CAS by hash | varies (stored mime) |
| `artifacts://summary` | Aggregate stats on artifacts | `application/json` |
| `artifacts://recent` | 10 most recent artifacts | `application/json` |
| `artifacts://by-tag/{tag}` | Filter artifacts by tag | `application/json` |
| `artifacts://by-creator/{creator}` | Artifacts by creator | `application/json` |

### Implementation

1. Add `resources()` method to `HootHandler` returning `Vec<Resource>`
2. Add `resource_templates()` for parameterized URIs like `graph://identity/{id}`
3. Implement `read_resource(uri)` to fetch content by URI
4. Wire to `EventDualityServer` state (audio_graph_db, conversation state, CAS)

### Example Response

```json
// resources/list
{
  "resources": [
    {"uri": "graph://identities", "name": "Audio Identities", "mimeType": "application/json"},
    {"uri": "artifacts://summary", "name": "Artifact Summary", "mimeType": "application/json"}
  ],
  "resourceTemplates": [
    {"uriTemplate": "graph://identity/{id}", "name": "Identity by ID"},
    {"uriTemplate": "cas://{hash}", "name": "CAS Content"},
    {"uriTemplate": "artifacts://by-tag/{tag}", "name": "Artifacts by Tag"}
  ]
}

// resources/read {uri: "graph://identities"}
{
  "contents": [{
    "uri": "graph://identities",
    "mimeType": "application/json",
    "text": "[{\"id\":\"jdxi\",\"name\":\"Roland JD-Xi\",\"tags\":[\"role:synth\"]}]"
  }]
}
```

---

## Task 11: Prompts API

Provide reusable prompt templates for common ensemble workflows.

### Prompts to Implement

| Name | Arguments | Description |
|------|-----------|-------------|
| `ensemble-jam` | `style`, `duration_bars`, `tempo` | Start a collaborative music session |
| `describe-setup` | `format` (markdown/json) | Document the current studio setup |
| `patch-synth` | `synth_id`, `style`, `character` | Generate a synth patch description |
| `sequence-idea` | `style`, `key`, `bars` | Spark a MIDI sequence idea |
| `connect-devices` | `from_id`, `to_id` | Suggest how to connect two devices |

### Implementation

1. Add `prompts()` method to `HootHandler` returning `Vec<Prompt>`
2. Implement `get_prompt(name, arguments)` to render prompt with args
3. Return `PromptMessage` with role and content for LLM consumption

### Example Response

```json
// prompts/list
{
  "prompts": [
    {
      "name": "ensemble-jam",
      "description": "Start a collaborative music session",
      "arguments": [
        {"name": "style", "description": "Musical style (ambient, techno, jazz)", "required": true},
        {"name": "tempo", "description": "BPM", "required": false},
        {"name": "duration_bars", "description": "Length in bars", "required": false}
      ]
    }
  ]
}

// prompts/get {name: "ensemble-jam", arguments: {style: "ambient", tempo: "72"}}
{
  "description": "Ambient jam session at 72 BPM",
  "messages": [
    {
      "role": "user",
      "content": "Let's create an ambient piece at 72 BPM. The studio has: Roland JD-Xi (synth), Arturia Keystep Pro (controller). Start with a warm pad from the JD-Xi, then layer in a slow arpeggio. Focus on texture and space."
    }
  ]
}
```

### Dynamic Context

Prompts pull live context:
- Current audio graph (what devices are available)
- Recent artifacts (what MIDI has been generated)
- CAS content (stored files)

## baton Crate Design

### Public API

```rust
use baton::{McpServer, ToolHandler, Tool, CallToolResult, Content};

// Application implements this trait
#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn tools(&self) -> Vec<Tool>;
    async fn call(&self, name: &str, args: serde_json::Value) -> Result<CallToolResult, Error>;
}

// Build and run server
let server = McpServer::builder()
    .name("hootenanny")
    .version(env!("CARGO_PKG_VERSION"))
    .handler(my_tool_handler)
    .session_ttl(Duration::from_secs(30 * 60))
    .build();

let router = server.router();  // Returns axum::Router
```

### Telemetry Design

Rich spans for every operation:

```rust
// Request span (parent)
mcp.request
  ├─ mcp.session_id = "abc-123"
  ├─ mcp.method = "tools/call"
  ├─ mcp.request_id = 42
  └─ rpc.system = "jsonrpc"

// Tool call span (child)
mcp.tool.call
  ├─ mcp.tool.name = "orpheus_generate"
  ├─ mcp.tool.duration_ms = 1234
  ├─ mcp.tool.success = true
  └─ mcp.tool.content_count = 1

// Session spans
mcp.session.created { mcp.session_id }
mcp.session.resumed { mcp.session_id, mcp.session.age_ms }
mcp.session.expired { mcp.session_id, mcp.session.lifetime_ms }
```

Standard attributes following OpenTelemetry semantic conventions:
- `rpc.system = "jsonrpc"`
- `rpc.method = "tools/call"`
- `rpc.jsonrpc.version = "2.0"`
- `rpc.jsonrpc.request_id`
- `rpc.jsonrpc.error_code` (on errors)

### Dependencies

```toml
[dependencies]
# Core
axum = "0.8"
tokio = { version = "1", features = ["sync", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Session management
dashmap = "6"
uuid = { version = "1", features = ["v4"] }

# Schema generation
schemars = "1"

# Telemetry
tracing = "0.1"

# Async trait (until RPITIT stabilizes fully)
async-trait = "0.1"
```

### File Structure

```
crates/baton/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public exports, McpServer builder
│   ├── types/
│   │   ├── mod.rs
│   │   ├── jsonrpc.rs   # JSON-RPC 2.0 types
│   │   ├── protocol.rs  # MCP-specific types
│   │   ├── tool.rs      # Tool, Content
│   │   └── error.rs     # ErrorData, error constructors
│   ├── transport/
│   │   ├── mod.rs
│   │   ├── sse.rs       # SSE endpoint handler
│   │   ├── message.rs   # POST message handler
│   │   └── router.rs    # Router builder
│   ├── session/
│   │   ├── mod.rs
│   │   ├── store.rs     # SessionStore trait + InMemoryStore
│   │   └── cleanup.rs   # TTL background task
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── dispatch.rs  # Method routing
│   │   └── handlers.rs  # initialize, list_tools, call_tool
│   └── telemetry/
│       ├── mod.rs
│       └── spans.rs     # Span builders, attributes
```

## References

- MCP Spec: `mcp-schema/2025-06-18/schema.json`
- OpenTelemetry Semantic Conventions: https://opentelemetry.io/docs/specs/semconv/
