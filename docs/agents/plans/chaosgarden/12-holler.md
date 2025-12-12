# 12-holler: MCP Gateway

**Prerequisite**: 10-hootenanny-zmq ✅
**Status**: ✅ Complete (All phases done)

**Note**: hooteproto (Phase 1) and holler CLI (Phase 2) can be built before any backend is converted. These phases validate the protocol design. Phases 3+ depend on backends being converted to ZMQ (see 11-luanette-zmq).

## Implementation Summary

### Crates Created
- **hooteproto** (`crates/hooteproto/`) - Protocol types for ZMQ messaging
- **holler** (`crates/holler/`) - MCP gateway and ZMQ CLI

### Usage

```bash
# Start hootenanny with ZMQ ROUTER + PUB
./target/debug/hootenanny --port 8080 \
  --zmq-bind tcp://0.0.0.0:5580 \
  --zmq-pub tcp://0.0.0.0:5581

# Test connectivity with holler CLI
./target/debug/holler ping tcp://127.0.0.1:5580

# Run holler as MCP gateway (with broadcast subscriptions)
./target/debug/holler serve --port 8081 \
  --hootenanny tcp://127.0.0.1:5580 \
  --hootenanny-pub tcp://127.0.0.1:5581

# Connect to SSE for notifications
curl -N http://127.0.0.1:8081/sse
```

### Test Results

```
cargo test -p hooteproto: 9 passed
cargo test -p holler: 8 passed (3 ZMQ roundtrip + 2 integration + 3 subscriber)
```

### Broadcast Flow (Complete)

```
orpheus_generate → BroadcastPublisher → ZMQ PUB → holler SUB → SSE clients
```

## Goal

Holler is the thin MCP gateway that bridges HTTP/MCP clients to the ZMQ backend services. It owns only protocol translation and tool routing - no business logic. All MCP clients (Claude, agents, web UIs) talk to Holler; Holler talks to backends over ZMQ.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                           CLIENTS                                    │
│              (Claude, agents, web UIs, any MCP client)              │
└─────────────────────────────┬───────────────────────────────────────┘
                              │ HTTP/MCP (Streamable HTTP transport)
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                            HOLLER                                    │
│                    (MCP Gateway - thin bridge)                       │
│                                                                      │
│  HTTP :8080                                                          │
│  ├─ POST /mcp (Streamable HTTP)                                     │
│  ├─ GET /mcp (SSE for server-initiated)                             │
│  └─ GET /health                                                      │
│                                                                      │
│  Tool Routing (by prefix):                                           │
│  ├─ lua_*, job_*, script_*  ──► Luanette :5570                      │
│  ├─ cas_*, artifact_*, graph_* ──► Hootenanny :5580                 │
│  └─ transport_*, timeline_*  ──► Chaosgarden :5555                  │
│                                                                      │
│  ZMQ DEALER connections:                                             │
│  ├─ tcp://luanette:5570                                             │
│  ├─ tcp://hootenanny:5580                                           │
│  └─ tcp://chaosgarden:5555                                          │
└─────────────────────────────────────────────────────────────────────┘
          │                    │                    │
          ▼                    ▼                    ▼
┌──────────────────┐ ┌──────────────────┐ ┌──────────────────┐
│    LUANETTE      │ │   HOOTENANNY     │ │   CHAOSGARDEN    │
│  ZMQ ROUTER      │ │  ZMQ ROUTER      │ │  ZMQ ROUTER      │
│    :5570         │ │    :5580         │ │    :5555         │
│                  │ │                  │ │                  │
│ Lua orchestration│ │ CAS, artifacts,  │ │ Real-time engine │
│ Job system       │ │ graph queries    │ │ Timeline, audio  │
└──────────────────┘ └──────────────────┘ └──────────────────┘
          │                    │                    │
          └────────────────────┴────────────────────┘
                               │
                               ▼
              ┌────────────────────────────────────┐
              │        SHARED FILESYSTEM           │
              │            (CAS Store)             │
              └────────────────────────────────────┘


Direct access (bypasses `holler serve`):
┌──────────────────┐
│   holler CLI     │
│  (ZMQ DEALER)    │────► Any backend directly
│                  │      e.g. `holler lua luanette "return 1+1"`
└──────────────────┘
```

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Holler is stateless | No business logic, just routing | Backends own their state; Holler is replaceable |
| Tool routing by prefix | Simple string match | No complex routing tables; conventions over config |
| MCP Streamable HTTP | Modern transport | Bidirectional, supports SSE, widely supported |
| ZMQ DEALER per backend | Independent connections | Backends can restart independently |
| No CAS in Holler | Doesn't touch files | File access happens in backends; Holler passes hashes |

## Tool Routing

Holler routes MCP tool calls to backends based on tool name prefix:

| Prefix | Backend | Port | Examples |
|--------|---------|------|----------|
| `lua_` | Luanette | 5570 | `lua_eval`, `lua_describe` |
| `job_` | Luanette | 5570 | `job_execute`, `job_status`, `job_poll` |
| `script_` | Luanette | 5570 | `script_store`, `script_search` |
| `cas_` | Hootenanny | 5580 | `cas_store`, `cas_inspect` |
| `artifact_` | Hootenanny | 5580 | `artifact_get`, `artifact_list` |
| `graph_` | Hootenanny | 5580 | `graph_query`, `graph_bind` |
| `transport_` | Chaosgarden | 5555 | `transport_play`, `transport_stop` |
| `timeline_` | Chaosgarden | 5555 | `timeline_query`, `timeline_add_marker` |
| `orpheus_` | Luanette | 5570 | `orpheus_generate`, `orpheus_continue` |
| `musicgen_` | Luanette | 5570 | `musicgen_generate` |

If a tool doesn't match any prefix, Holler returns an error.

## Protocol Translation

### MCP → ZMQ

```rust
// Incoming MCP tool call
let mcp_request = CallToolRequest {
    name: "lua_eval",
    arguments: json!({"code": "return 1 + 1"}),
};

// Translate to hooteproto Envelope
let envelope = Envelope {
    id: Uuid::new_v4(),
    traceparent: extract_traceparent(&http_headers),
    payload: Payload::LuaEval {
        code: args["code"].as_str().unwrap().to_string(),
        params: args.get("params").cloned(),
    },
};

// Send to Luanette, await reply
let reply = luanette_dealer.send(envelope).await?;

// Translate back to MCP
match reply.payload {
    Payload::Success { result } => CallToolResult::success(result),
    Payload::Error { code, message, .. } => CallToolResult::error(code, message),
    _ => CallToolResult::error("unexpected_reply", "Backend sent unexpected response"),
}
```

### Tool Discovery

Holler aggregates tool lists from all backends:

```rust
async fn list_tools(&self) -> Vec<ToolInfo> {
    let mut tools = Vec::new();

    // Query each backend for its tools
    for (backend, dealer) in &self.backends {
        let reply = dealer.send(Payload::ListTools).await?;
        if let Payload::ToolList { tools: backend_tools } = reply.payload {
            tools.extend(backend_tools);
        }
    }

    tools
}
```

## What Holler Owns

- HTTP server (axum)
- MCP Streamable HTTP transport
- Session management (for SSE streams)
- ZMQ DEALER sockets to each backend
- Tool routing table
- OTEL trace propagation (HTTP headers → traceparent in Envelope)

## What Holler Does NOT Own

- Business logic (all in backends)
- File access / CAS (backends read files directly)
- Job tracking (Luanette owns jobs)
- Artifact storage (Hootenanny owns artifacts)
- Audio routing (Chaosgarden owns PipeWire)

## Implementation Plan

### Phase 1: hooteproto Crate ✅
- [x] Create `crates/hooteproto/`
- [x] Define Envelope, Payload, Broadcast types
- [x] Serialization with serde (JSON for debugging, msgpack optional)
- [x] Unit tests for roundtrip serialization (9 tests passing)

### Phase 2: holler CLI (ZMQ subcommands) ✅
Build holler CLI as the first hooteproto client - validates the protocol before building the HTTP gateway.

- [x] `holler ping <endpoint>` - test connectivity
- [x] `holler send <endpoint> <payload.json>` - raw message send
- [x] `holler lua <endpoint> <code>` - lua_eval shorthand
- [x] `holler job <endpoint> status <job_id>` - job queries
- [x] Integration tests against a mock ROUTER (3 tests)

This gives us a test harness before any backend is converted. hrcli continues to work via MCP/HTTP.

The `holler serve` subcommand (Phase 3+) runs the MCP gateway.

### Phase 3: Holler Crate Setup ✅
- [x] Create `crates/holler/`
- [x] Add dependencies: axum, hooteproto, zeromq, tokio, tracing
- [x] CLI with clap: `holler serve`, `holler ping`, etc.

### Phase 4: ZMQ Client Layer ✅
- [x] Create `client.rs` - DEALER socket wrapper
- [x] Create `backend.rs` - BackendPool for backend connections
- [x] Implement timeout handling
- [x] Handle backend unavailability gracefully

### Phase 5: Tool Router ✅
- [x] Prefix-based routing in `BackendPool::route_tool()`
- [x] MCP ↔ hooteproto conversion in `tool_to_payload()`
- [x] Tool discovery aggregation from all backends
- [x] Handle unknown tools with clear error

### Phase 6: MCP HTTP Server (`holler serve`) ✅
- [x] Create `serve.rs` - axum setup with graceful shutdown
- [x] Create `mcp.rs` - Streamable HTTP handlers (POST /mcp)
- [x] Create `sse.rs` - SSE endpoint for server-initiated notifications (GET /sse)
- [x] Health check endpoint (GET /health)
- [x] Broadcast channel for forwarding events to SSE clients
- [x] Create `subscriber.rs` - ZMQ SUB sockets for backend PUB broadcasts
- [x] CLI flags: `--luanette-pub`, `--hootenanny-pub`, `--chaosgarden-pub`

### Phase 7: OTEL Integration ✅
- [x] Extract traceparent from HTTP headers (`telemetry::extract_traceparent`)
- [x] Inject into Envelope for backend calls (`request_with_trace`)
- [x] Propagate span context through the gateway
- [x] Default endpoint: localhost:4317 (standard OTLP gRPC)
- [x] **Backend span extraction**: `HooteprotoServer` parses traceparent and creates child spans
- [x] `telemetry::parse_traceparent()` converts W3C traceparent to OTel Context
- [x] Unit tests for traceparent parsing (6 tests)

### Phase 8: Testing ✅
- [x] Unit tests for protocol serialization (hooteproto) - 9 tests
- [x] ZMQ roundtrip tests (holler) - 3 tests
- [x] Integration tests: mock backend + traceparent propagation - 2 tests
- [ ] End-to-end test: live MCP client → Holler → Hootenanny (manual)

## CLI

### MCP Gateway Server
```bash
holler serve \
    --port 8080 \
    --luanette tcp://127.0.0.1:5570 \
    --hootenanny tcp://127.0.0.1:5580 \
    --chaosgarden tcp://127.0.0.1:5555 \
    --otlp-endpoint 127.0.0.1:35991
```

### Direct ZMQ Commands
```bash
# Test connectivity
holler ping tcp://127.0.0.1:5570

# Send raw hooteproto message
holler send tcp://127.0.0.1:5570 '{"type": "lua_eval", "code": "return 1+1"}'

# Lua eval shorthand
holler lua tcp://127.0.0.1:5570 "return midi.note(60, 0.5)"

# Job queries
holler job tcp://127.0.0.1:5570 status abc123
holler job tcp://127.0.0.1:5570 list
holler job tcp://127.0.0.1:5570 poll abc123 def456 --timeout 30000
```

### Environment Variables
```bash
export HOLLER_LUANETTE=tcp://127.0.0.1:5570
export HOLLER_HOOTENANNY=tcp://127.0.0.1:5580
export HOLLER_CHAOSGARDEN=tcp://127.0.0.1:5555

# Then just:
holler ping luanette
holler lua luanette "return 1+1"
```

## Configuration

Holler can also read from a config file:

```toml
# holler.toml
[http]
port = 8080

[backends]
luanette = "tcp://127.0.0.1:5570"
hootenanny = "tcp://127.0.0.1:5580"
chaosgarden = "tcp://127.0.0.1:5555"

[timeouts]
luanette_ms = 30000
hootenanny_ms = 5000
chaosgarden_ms = 1000

[otlp]
endpoint = "127.0.0.1:35991"
```

## Port Allocation

| Port | Protocol | Service | Purpose |
|------|----------|---------|---------|
| 8080 | HTTP | Holler | MCP gateway |
| 5555 | ZMQ | Chaosgarden | Real-time engine |
| 5570 | ZMQ | Luanette | Lua orchestration |
| 5580 | ZMQ | Hootenanny | CAS, artifacts, graph |

## Acceptance Criteria

- [ ] Holler starts and binds HTTP :8080
- [ ] Connects to all three backends via ZMQ DEALER
- [ ] Routes lua_* tools to Luanette
- [ ] Routes cas_* tools to Hootenanny
- [ ] Routes transport_* tools to Chaosgarden
- [ ] Aggregates tool list from all backends
- [ ] Propagates OTEL traces through gateway
- [ ] Returns clear errors for unavailable backends
- [ ] Health endpoint reports backend connectivity

## Future Work

- WebSocket transport option
- Rate limiting per client
- Authentication/authorization layer
- Metrics endpoint (Prometheus)
- Admin API for runtime config
- Multi-holler load balancing (multiple Holler instances)
