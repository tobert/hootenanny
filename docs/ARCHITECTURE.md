# halfremembered-mcp Architecture

## Overview

halfremembered-mcp is a WebSocket-based MCP (Model Context Protocol) server that enables multi-agent collaboration. It provides local LLM tool integration and serves as the foundation for a human-AI music ensemble.

## System Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                      Multi-Agent Clients                          │
│                                                                    │
│  ┌──────────────┐  ┌──────────────┐  ┌────────┐  ┌───────────┐  │
│  │ 🤖 Claude    │  │ 💎 Gemini    │  │ 🦙 GUI │  │ 🎹 VST    │  │
│  │    Code      │  │              │  │ Client │  │  Plugin   │  │
│  └──────────────┘  └──────────────┘  └────────┘  └───────────┘  │
│         │                 │               │            │          │
└─────────┼─────────────────┼───────────────┼────────────┼──────────┘
          │                 │               │            │
          └─────────────────┴───────────────┴────────────┘
                                  │
                            WebSocket :8080
                        (Multi-client transport)
                                  │
          ┌───────────────────────▼───────────────────────┐
          │        halfremembered_mcp MCP Server          │
          │                                                │
          │  ┌──────────────────────────────────────────┐ │
          │  │     rmcp::ServerHandler                  │ │
          │  │  - Tool discovery                        │ │
          │  │  - Tool invocation                       │ │
          │  │  - Shared state (future)                 │ │
          │  └──────────────────────────────────────────┘ │
          └────────────┬─────────────┬──────────┬─────────┘
                       │             │          │
          ┌────────────▼────┐   ┌────▼─────┐  ┌▼──────────┐
          │   DeepSeek      │   │   Lua    │  │   Music   │
          │   Tool Box      │   │  Tools   │  │   Tools   │
          │   (Phase 1)     │   │(Phase 2) │  │ (Phase 3) │
          └────────┬────────┘   └────┬─────┘  └─┬─────────┘
                   │                 │           │
          ┌────────▼────────┐   ┌────▼─────┐  ┌─▼─────────┐
          │    Ollama       │   │   sled   │  │  Music    │
          │  DeepSeek-33B   │   │  State   │  │  Models   │
          │  (local model)  │   │   DB     │  │  (local)  │
          └─────────────────┘   └──────────┘  └───────────┘
```

## Key Design Decisions

### 1. WebSocket Transport (vs stdio)

**Decision**: Use `rmcp::transport::websocket` on `127.0.0.1:8080`

**Rationale**:
- **Multi-client support**: Multiple agents can connect simultaneously
- **Shared state**: All clients see same MCP server instance
- **Ensemble foundation**: Critical for multi-agent music collaboration
- **Real-time**: WebSocket provides bidirectional real-time communication

**Trade-offs**:
- ✅ Enables parallel agent work (Claude + Gemini + local models)
- ✅ Foundation for VST integration (experimental)
- ⚠️ Slightly more complex than stdio (but worth it)
- ⚠️ Requires port management (currently localhost-only, no auth needed)

**Discoverer**: 💎 Gemini (2025-11-15)

### 2. Rust + Tokio + rmcp

**Stack**:
- **Language**: Rust (edition 2021)
- **Async**: Tokio runtime (full features)
- **MCP SDK**: rmcp from https://github.com/modelcontextprotocol/rust-sdk
- **Serialization**: serde + serde_json
- **Error Handling**: anyhow::Result with context

**Why Rust**:
- Type safety prevents bugs (critical for multi-agent systems)
- Async performance (Tokio) for WebSocket handling
- Rich type system for modeling domains ("enums as storytellers")
- Compiler as creative partner (philosophy: fail at compile time, not runtime)

**Philosophy**: "Expressiveness over Performance" - use rich types even for simple concepts

### 3. Three-Phase Roadmap

```
Phase 0: Documentation ✅
   ↓
Phase 1: DeepSeek Tools (MVP) ← We are here
   ↓
Phase 2: Lua Extension System
   ↓
Phase 3: Music Ensemble (Vision)
```

Each phase builds on previous:
- **MVP first**: Ship working code review tools
- **Extend with Lua**: Add dynamic tool creation (no recompilation)
- **Music later**: Leverage extensibility for creative collaboration

## Component Details

### MCP Server Core

**File**: `src/main.rs`
**Responsibilities**:
- Set up tracing/logging (env_filter)
- Create `ServerHandler` with registered tools
- Bind WebSocket transport to `127.0.0.1:8080`
- Run async event loop

**Key Code Pattern**:
```rust
use rmcp::{ServerHandler, transport::websocket};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set up logging
    tracing_subscriber::fmt()
        .with_env_filter("halfremembered_mcp=info")
        .init();

    // Create handler with tools
    let handler = ServerHandler::new(/* server info */)
        .register_tool_box(DeepSeekTools::new(ollama_client));

    // Run WebSocket transport
    let addr = "127.0.0.1:8080";
    websocket::websocket_transport(handler, addr).await
}
```

### Tool System (Phase 1: DeepSeek)

**File**: `src/tools/deepseek.rs`
**Pattern**: `#[tool_box]` macro for grouping tools

**Tools**:
1. `review_code(code: String) -> Review`
   - Ask DeepSeek to review code for bugs, style, improvements

2. `explain_code(code: String) -> Explanation`
   - Get detailed explanation of what code does

3. `suggest_improvements(code: String, context: String) -> Suggestions`
   - Context-aware improvement suggestions

**Implementation Pattern**:
```rust
#[tool_box]
impl DeepSeekTools {
    #[tool(description = "Review code for bugs and improvements")]
    async fn review_code(&self, code: String) -> Result<Review> {
        let prompt = format!("Review this code:\n{}", code);
        self.ollama.generate("deepseek-coder:33b", prompt).await
    }
}
```

### Ollama Client (Phase 1)

**File**: `src/llm/ollama.rs`
**Responsibilities**:
- HTTP client to Ollama API (http://localhost:11434)
- `generate()` with timeout (30s default)
- Error handling with context

**Key Features**:
- Async with `tokio::time::timeout`
- Fail loud: clear errors if Ollama not running
- Configurable model names

### Lua Tool System (Phase 2)

**Directory**: `~/.config/halfremembered-mcp/tools/`
**Pattern**: File-based hot-reloading

**Example Tool**:
```
~/.config/halfremembered-mcp/tools/my_tool/
├── tool.lua           # Tool logic
├── manifest.toml      # Metadata + permissions
└── state/             # Per-tool state (sled)
```

**Hot-Reload**: File watcher detects changes, reloads Lua without restart

**Permissions** (Phase 2 sandbox):
```toml
[permissions]
read = ["~/code/**"]
write = []
network = ["localhost:*"]
```

### Music Tools (Phase 3 - Vision)

**Future Integration Points**:
- Music generation models (local, ROCm GPU)
- VST plugin protocol over MCP (experimental)
- Real-time "micro-batch" generation
- Multi-agent composition (Claude + Gemini + music models)

**Not Yet Designed**: Open research area

## Data Flow

### Tool Invocation Flow

```
1. Client (Claude) sends MCP request
      ↓ WebSocket
2. MCP Server receives request
      ↓ rmcp deserializes
3. ServerHandler routes to tool
      ↓ Async call
4. DeepSeekTools::review_code()
      ↓ HTTP request
5. Ollama generates response
      ↓ HTTP response
6. Result serialized to JSON
      ↓ WebSocket
7. Client receives MCP response
```

### Multi-Client Scenario

```
┌─────────────────────────────────────────────────┐
│  🤖 Claude: "Review this Rust code"             │
│  💎 Gemini: "Explain this Python function"      │
└───────────┬─────────────────────┬───────────────┘
            │                     │
       WebSocket              WebSocket
            │                     │
      ┌─────▼─────────────────────▼─────┐
      │  MCP Server (shared instance)   │
      │  - Concurrent request handling  │
      │  - Tokio async runtime          │
      └─────┬─────────────────────┬─────┘
            │                     │
     DeepSeek review       DeepSeek explain
            │                     │
      ┌─────▼─────────────────────▼─────┐
      │  Ollama (queued requests)        │
      │  - Sequential model inference    │
      └──────────────────────────────────┘
```

**Note**: Ollama handles one request at a time; MCP server queues concurrent requests.

## Error Handling Philosophy

**Principle**: "Fail Loud and Clear"

**Pattern**:
```rust
ollama_client
    .generate(model, prompt)
    .await
    .context("Failed to call Ollama")?
    .context(format!("Model '{}' not found. Run: ollama pull {}", model, model))?
```

**Benefits**:
- No silent failures
- Actionable error messages
- Fast debugging (errors point to exact fix)

## Security Model

### Phase 1 (Current)
- **Localhost-only**: `127.0.0.1:8080` (no external access)
- **No authentication**: Trusted local environment
- **Tool permissions**: None (all tools trusted)

### Phase 2 (Lua Tools)
- **Sandbox permissions**: Read/write/network controls per tool
- **File system isolation**: Tools can't escape designated directories
- **Network limits**: Whitelist of allowed endpoints

### Phase 3 (Music)
- **TBD**: Will depend on VST integration approach

## State Management

### Phase 1
- **Stateless**: No persistence between requests
- **Server state**: In-memory only (lost on restart)

### Phase 2 (Lua + sled)
- **Per-tool state**: `sled` embedded database
- **State directory**: `~/.config/halfremembered-mcp/tools/{tool_name}/state/`
- **Isolation**: Each Lua tool has own sled instance

### Phase 3 (Music)
- **TBD**: Likely real-time composition state, timeline/sequence tracking

## Extension Points

### Adding New Tools (Phase 1)

1. Create `src/tools/new_tool.rs`
2. Implement with `#[tool_box]` macro
3. Register in `main.rs`: `handler.register_tool_box(NewTools::new())`
4. Rebuild and restart

### Adding Lua Tools (Phase 2)

1. Create `~/.config/halfremembered-mcp/tools/my_tool/tool.lua`
2. Add `manifest.toml` with metadata
3. Server auto-detects and hot-reloads (no restart)

### Future: VST Integration (Phase 3)

**Experimental Idea** (💎 Gemini):
- VST plugins as MCP clients
- MCP server sends MIDI/audio generation commands
- Real-time music performance over WebSocket

**Status**: Exploratory, not yet designed

## Performance Considerations

### Current (Phase 1)
- **Bottleneck**: Ollama model inference (10-60s per request)
- **Concurrency**: Tokio handles multiple WebSocket clients
- **Memory**: Minimal (no large state, streaming responses from Ollama)

### Phase 2 (Lua)
- **Lua overhead**: Negligible (<1ms per tool invocation)
- **Hot-reload cost**: File watcher + reload on change (~10ms)
- **State persistence**: sled is fast (embedded KV store)

### Phase 3 (Music)
- **Real-time requirements**: TBD based on music model latency
- **GPU utilization**: ROCm-enabled models (local hardware)

## Testing Strategy

### Phase 1
- **Manual**: MCP Inspector (WebSocket client)
- **Scenario tests**: See `test-scenarios.md`
- **Unit tests**: Ollama client with mocked HTTP

### Phase 2
- **Lua sandbox tests**: Ensure permissions enforced
- **Hot-reload tests**: File changes trigger reload
- **State persistence**: sled CRUD operations

## Deployment

### Phase 1 (MVP)
```bash
# Build
cargo build --release

# Run (requires Ollama running)
./target/release/halfremembered_mcp

# Connect clients
# Claude Code: Configure MCP server at ws://localhost:8080
# MCP Inspector: npx @modelcontextprotocol/inspector ws://localhost:8080
```

### System Requirements
- **OS**: Linux (tested), macOS (should work), Windows (untested)
- **Ollama**: Running at `localhost:11434`
- **Model**: `ollama pull deepseek-coder:33b` (or 7B variant)
- **Rust**: 1.70+ (edition 2021)

## Multi-Agent Collaboration Architecture

### Agent Memory System

See `docs/agents/` for full details. Key points:

**Parallel Agents**:
- Amy runs Claude and Gemini simultaneously
- Each agent has section in `docs/agents/NOW.md`
- No merge conflicts (explicit ownership)

**Shared Knowledge**:
- `PATTERNS.md`: Attributed discoveries
- `CONTEXT.md`: Architecture decisions, roadmap
- `jj` commits: Full reasoning narratives

**Coordination**:
- Coordination Notes section in NOW.md
- Explicit sync points when work converges
- Attribution for all architectural decisions

## References

- **MCP Specification**: https://modelcontextprotocol.io
- **rmcp SDK**: https://github.com/modelcontextprotocol/rust-sdk
- **Ollama**: https://ollama.ai
- **Development Guidelines**: `docs/BOTS.md`
- **Project Context**: `docs/agents/CONTEXT.md`
- **Implementation Plans**: `docs/agents/plans/`

---

**Last Updated**: 2025-11-15 by 🤖 Claude
**Current Phase**: Phase 0 complete, Phase 1 (MVP) ready to start
**Architecture Status**: Stable for MVP, extensible for Phases 2-3
