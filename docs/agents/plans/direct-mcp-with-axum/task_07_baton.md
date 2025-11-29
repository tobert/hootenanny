# Task 7: baton Crate

**Objective**: Create `baton`, a reusable MCP server library with rich telemetry.

## Overview

`baton` conducts the MCP protocol orchestra - handling JSON-RPC parsing, session management, and tool dispatch so applications can focus on their tools.

## Implementation Order

### Phase 1: Types (`types/`)

Start with the foundation - MCP protocol types derived from the spec.

#### 1.1 JSON-RPC Base (`types/jsonrpc.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse<T = Value> {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub result: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub error: ErrorData,
}

// Request ID can be number or string
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
    Null,
}
```

#### 1.2 Error Types (`types/error.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ErrorData {
    // JSON-RPC standard errors
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    // Constructors
    pub fn parse_error(msg: impl Into<String>) -> Self { ... }
    pub fn invalid_request(msg: impl Into<String>) -> Self { ... }
    pub fn method_not_found(method: &str) -> Self { ... }
    pub fn invalid_params(msg: impl Into<String>) -> Self { ... }
    pub fn internal_error(msg: impl Into<String>) -> Self { ... }
}
```

#### 1.3 MCP Protocol Types (`types/protocol.rs`)

```rust
// Initialize
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: Implementation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: Implementation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    // Add fields as needed
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}
```

#### 1.4 Tool Types (`types/tool.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
}

impl Tool {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self { ... }

    pub fn with_schema<T: JsonSchema>(mut self) -> Self {
        self.input_schema = schemars::schema_for!(T).into();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<Content>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

impl CallToolResult {
    pub fn success(content: Vec<Content>) -> Self { ... }
    pub fn error(message: impl Into<String>) -> Self { ... }
}
```

#### 1.5 Content Types (`types/content.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { uri: String, mime_type: Option<String>, text: Option<String> },
}

impl Content {
    pub fn text(s: impl Into<String>) -> Self {
        Content::Text { text: s.into() }
    }

    pub fn image_base64(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Content::Image { data: data.into(), mime_type: mime_type.into() }
    }
}
```

### Phase 2: Session Management (`session/`)

#### 2.1 Session Types (`session/mod.rs`)

```rust
pub struct Session {
    pub id: String,
    pub created_at: Instant,
    pub last_seen: Instant,
    pub client_info: Option<Implementation>,
    tx: Option<Sender<SseEvent>>,
}

pub struct SessionStats {
    pub total: usize,
    pub connected: usize,
    pub zombie: usize,
}
```

#### 2.2 Session Store (`session/store.rs`)

```rust
pub trait SessionStore: Send + Sync {
    fn get_or_create(&self, id_hint: Option<&str>) -> String;
    fn get(&self, id: &str) -> Option<SessionRef>;
    fn touch(&self, id: &str);
    fn register_connection(&self, id: &str, tx: Sender<SseEvent>);
    fn cleanup(&self, zombie_ttl: Duration, disconnected_ttl: Duration) -> usize;
    fn stats(&self) -> SessionStats;
}

pub struct InMemorySessionStore {
    sessions: DashMap<String, Session>,
}
```

### Phase 3: Telemetry (`telemetry/`)

#### 3.1 Span Helpers (`telemetry/spans.rs`)

```rust
use tracing::{span, Level, Span};

pub fn request_span(method: &str, request_id: &RequestId, session_id: &str) -> Span {
    span!(
        Level::INFO,
        "mcp.request",
        rpc.system = "jsonrpc",
        rpc.method = %method,
        mcp.request_id = ?request_id,
        mcp.session_id = %session_id,
        otel.kind = "server",
    )
}

pub fn tool_call_span(name: &str) -> Span {
    span!(
        Level::INFO,
        "mcp.tool.call",
        mcp.tool.name = %name,
        mcp.tool.success = tracing::field::Empty,
        mcp.tool.duration_ms = tracing::field::Empty,
        mcp.tool.content_count = tracing::field::Empty,
    )
}

pub fn session_span(event: &str, session_id: &str) -> Span {
    span!(
        Level::INFO,
        "mcp.session",
        mcp.session.event = %event,
        mcp.session_id = %session_id,
    )
}
```

### Phase 4: Transport (`transport/`)

#### 4.1 SSE Handler (`transport/sse.rs`)

```rust
pub async fn sse_handler<H: ToolHandler>(
    State(state): State<Arc<McpState<H>>>,
    Query(params): Query<SseParams>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let _span = session_span("connect", &session_id).entered();
    // ... implementation
}
```

#### 4.2 Message Handler (`transport/message.rs`)

```rust
pub async fn message_handler<H: ToolHandler>(
    State(state): State<Arc<McpState<H>>>,
    Query(params): Query<MessageParams>,
    Json(body): Json<Value>,
) -> Response {
    let request: JsonRpcRequest = serde_json::from_value(body)?;
    let span = request_span(&request.method, &request.id, &params.session_id);
    let _guard = span.enter();

    // Dispatch to protocol handlers
    // ...
}
```

### Phase 5: Protocol Dispatch (`protocol/`)

#### 5.1 Method Dispatch (`protocol/dispatch.rs`)

```rust
pub async fn dispatch<H: ToolHandler>(
    handler: &H,
    session: &Session,
    request: &JsonRpcRequest,
) -> Result<Value, ErrorData> {
    match request.method.as_str() {
        "initialize" => handle_initialize(request).await,
        "notifications/initialized" => Ok(Value::Null), // notification, no response
        "ping" => Ok(json!({})),
        "tools/list" => handle_list_tools(handler).await,
        "tools/call" => handle_call_tool(handler, request).await,
        _ => Err(ErrorData::method_not_found(&request.method)),
    }
}
```

### Phase 6: Server Builder (`lib.rs`)

```rust
pub struct McpServer<H: ToolHandler> {
    name: String,
    version: String,
    handler: Arc<H>,
    sessions: Arc<dyn SessionStore>,
    config: ServerConfig,
}

impl<H: ToolHandler> McpServer<H> {
    pub fn builder() -> McpServerBuilder<H> { ... }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/sse", get(sse_handler::<H>))
            .route("/message", post(message_handler::<H>))
            .with_state(self.state())
    }

    pub fn spawn_cleanup_task(&self, cancel: CancellationToken) -> JoinHandle<()> { ... }
}

#[async_trait]
pub trait ToolHandler: Send + Sync + 'static {
    fn tools(&self) -> Vec<Tool>;
    async fn call_tool(&self, name: &str, arguments: Value) -> Result<CallToolResult, ErrorData>;

    // Optional overrides
    fn server_info(&self) -> Implementation { ... }
    fn capabilities(&self) -> ServerCapabilities { ... }
    fn instructions(&self) -> Option<String> { None }
}
```

## Testing Strategy

### Unit Tests
- JSON-RPC serialization round-trips
- Error construction
- Session TTL logic

### Integration Tests
- Full request/response cycle with test ToolHandler
- Session resumption
- Concurrent connections

## Success Criteria

- [ ] All types serialize/deserialize correctly per MCP spec
- [ ] Sessions persist across reconnections
- [ ] Rich telemetry spans for all operations
- [ ] Clean `ToolHandler` trait for applications
- [ ] `cargo test -p baton` passes
- [ ] Documentation with examples

## Files to Create

```
crates/baton/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs
│   ├── types/
│   │   ├── mod.rs
│   │   ├── jsonrpc.rs
│   │   ├── protocol.rs
│   │   ├── tool.rs
│   │   ├── content.rs
│   │   └── error.rs
│   ├── session/
│   │   ├── mod.rs
│   │   ├── store.rs
│   │   └── cleanup.rs
│   ├── transport/
│   │   ├── mod.rs
│   │   ├── sse.rs
│   │   ├── message.rs
│   │   └── router.rs
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── dispatch.rs
│   │   └── handlers.rs
│   └── telemetry/
│       ├── mod.rs
│       └── spans.rs
```
